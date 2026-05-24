//! Interior hole fill using the shared prediction helper from `grow/predict`.
//!
//! Post-grow refinement pass. Enumerates `(i, j)` cells inside the labelled
//! bounding box that are still empty, plus row / column extensions immediately
//! beyond the bbox, and tries to attach a candidate at each one using the
//! **same** per-cell ladder as [`crate::grow::engine::bfs_grow`]:
//!
//! 1. [`crate::grow::predict::predict_from_neighbours`] for the per-cell
//!    prediction (closes Gap 6 — no copy of the prediction logic lives here).
//! 2. [`crate::grow::attach::collect_candidates`] + [`crate::grow::attach::choose_unambiguous`]
//!    for the KD-tree radius search and runner-up gate.
//! 3. The same `SquareGrowContext` hooks: `is_eligible`, `agrees`, `edge_ok`,
//!    `accept_candidate`. The fill pass passes through the `EdgeCtx` so a
//!    consumer's BFS edge invariant is preserved.
//!
//! ## Precision contract
//!
//! Fill attachments must obey the same invariants as BFS-grow attachments.
//! False positives here are unrecoverable for downstream calibration just as
//! they are in the BFS engine; fill never relaxes a gate.
//!
//! ## Events
//!
//! Emits `Event::StageStarted/Finished { stage: Refine }` bookends and per-cell
//! `Event::GrowAttached` / `Event::GrowRejected` events — same shape as the
//! BFS engine. The bench harness sees the refine stage as a continuation of
//! the BFS attach trace.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use kiddo::KdTree;
use nalgebra::{Point2, Vector2};

use crate::diagnostics::{DiagnosticSink, Event, GrowRejectReason, Stage};
use crate::feature::Observation;
use crate::float::{lit, Float};
use crate::grow::attach::{choose_unambiguous, collect_candidates, AmbiguityReason};
use crate::grow::context::{EdgeCtx, SquareGrowContext};
use crate::grow::params::GrowResult;
use crate::grow::predict::{predict_from_neighbours, LabelledNeighbour, PredictCtx};
use crate::lattice::Coord;

/// Tunables for [`fill_grid_holes`].
///
/// `search_rel` and `ambiguity_factor` mirror the BFS knobs in
/// [`crate::grow::params::GrowParams`]; the fill defaults are intentionally
/// tighter (smaller search radius, larger ambiguity factor) than the BFS
/// defaults because interior-hole gaps carry less geometric support.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct FillParams<F: Float> {
    /// Candidate-search radius around each per-cell prediction, expressed as
    /// a fraction of `cell_size`. Default `0.45`.
    pub search_rel: F,
    /// Acceptance ambiguity factor: `second >= ambiguity_factor * nearest`.
    /// Default `1.5` (tighter than the BFS `1.3` because fill predictions
    /// have less support).
    pub ambiguity_factor: F,
    /// Maximum number of interior-fill passes. Defaults to `1` — most grids
    /// converge in one scan and additional passes only help when one
    /// attachment unlocks another in its 3×3 window.
    pub max_interior_passes: usize,
    /// How far past the labelled bbox each row / column extends. `1` is the
    /// legacy single-step behaviour; larger values let the fill reach a
    /// missing corner two cells past the bbox in one call.
    pub line_extension_depth: i32,
}

impl<F: Float> Default for FillParams<F> {
    fn default() -> Self {
        Self {
            search_rel: lit::<F>(0.45_f32),
            ambiguity_factor: lit::<F>(1.5_f32),
            max_interior_passes: 1,
            line_extension_depth: 1,
        }
    }
}

impl<F: Float> FillParams<F> {
    /// Construct fill parameters from the search radius and the ambiguity
    /// factor; other knobs take their defaults.
    pub fn new(search_rel: F, ambiguity_factor: F) -> Self {
        Self {
            search_rel,
            ambiguity_factor,
            ..Self::default()
        }
    }

    /// Override the maximum interior-pass count.
    #[must_use]
    pub fn with_max_interior_passes(mut self, max: usize) -> Self {
        self.max_interior_passes = max;
        self
    }

    /// Override the per-row / per-column line extension depth.
    #[must_use]
    pub fn with_line_extension_depth(mut self, depth: i32) -> Self {
        self.line_extension_depth = depth;
        self
    }
}

/// Counters returned by [`fill_grid_holes`].
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct FillStats {
    /// Number of corners attached across all interior passes plus line extensions.
    pub n_attached: usize,
    /// Number of candidate cells that failed at least one acceptance gate.
    pub n_rejected: usize,
    /// Number of interior passes actually performed (≤ `max_interior_passes`).
    pub passes_run: usize,
}

/// Run the fill pass: interior hole-fill (iterated) plus per-row / per-column
/// line extensions just past the labelled bbox.
///
/// Mutates `grow.labelled` in place and updates `grow.n_attached`. Emits
/// `Event::StageStarted/Finished { stage: Refine }` bookends plus per-cell
/// `Event::GrowAttached` / `Event::GrowRejected` events.
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "info",
        skip_all,
        fields(num_observations = observations.len(), n_labelled = grow.labelled.len()),
    )
)]
pub fn fill_grid_holes<F, C>(
    observations: &[Observation<F>],
    grow: &mut GrowResult<F>,
    params: &FillParams<F>,
    ctx: &C,
    sink: &mut impl DiagnosticSink<F>,
) -> FillStats
where
    F: Float + kiddo::float::kdtree::Axis,
    C: SquareGrowContext<F> + ?Sized,
{
    let start = Instant::now();
    sink.emit(Event::StageStarted {
        stage: Stage::Refine,
    });

    let mut stats = FillStats::default();
    if grow.labelled.is_empty() {
        sink.emit(Event::StageFinished {
            stage: Stage::Refine,
            duration: start.elapsed(),
        });
        return stats;
    }

    let positions: Vec<Point2<F>> = observations.iter().map(|o| o.position).collect();
    let cell_size = grow.cell_size;

    // The global axes are derived from any pair of cardinally-adjacent labels
    // so the fill ladder can fall back when no per-neighbour local step is
    // available. We pick `(0, 0) -> (1, 0)` and `(0, 0) -> (0, 1)` when both
    // exist; otherwise we scan for any cardinal pair in the labelled set.
    let (axis_i, axis_j) = derive_axes_from_labelled(&grow.labelled, &positions, cell_size);

    let max_passes = params.max_interior_passes.max(1);
    let search_radius = params.search_rel * cell_size;

    for _ in 0..max_passes {
        stats.passes_run += 1;
        let (tree, slot_to_idx) = build_unlabelled_tree(observations, grow, ctx);
        let in_use: HashSet<usize> = grow.labelled.values().copied().collect();
        let cells = enumerate_fill_cells(&grow.labelled, params.line_extension_depth);
        let pass_ctx = FillAttempt {
            positions: &positions,
            ctx,
            params,
            tree: &tree,
            slot_to_idx: &slot_to_idx,
            cell_size,
            axis_i,
            axis_j,
            search_radius,
        };
        let mut attached_this_pass = 0usize;
        for cell in cells {
            if grow.labelled.contains_key(&cell) {
                continue;
            }
            match try_fill_cell(cell, grow, &in_use, &pass_ctx, sink) {
                Some(idx) => {
                    // Update the in-use set transiently via the labelled map;
                    // the KD-tree is rebuilt each pass so we don't have to
                    // remove the freshly-attached slot here.
                    grow.labelled.insert(cell, idx);
                    grow.n_attached += 1;
                    stats.n_attached += 1;
                    attached_this_pass += 1;
                }
                None => {
                    stats.n_rejected += 1;
                }
            }
        }
        if attached_this_pass == 0 {
            break;
        }
    }

    // Refresh the bbox in case fill extended one of the rows / columns past the
    // previous extent.
    refresh_bbox(grow);

    sink.emit(Event::StageFinished {
        stage: Stage::Refine,
        duration: start.elapsed(),
    });

    stats
}

// ---- Private helpers ----

struct FillAttempt<'a, F, C>
where
    F: Float + kiddo::float::kdtree::Axis,
    C: SquareGrowContext<F> + ?Sized,
{
    positions: &'a [Point2<F>],
    ctx: &'a C,
    params: &'a FillParams<F>,
    tree: &'a KdTree<F, 2>,
    slot_to_idx: &'a [usize],
    cell_size: F,
    axis_i: Vector2<F>,
    axis_j: Vector2<F>,
    search_radius: F,
}

fn try_fill_cell<F, C>(
    cell: Coord,
    grow: &GrowResult<F>,
    in_use: &HashSet<usize>,
    attempt: &FillAttempt<'_, F, C>,
    sink: &mut impl DiagnosticSink<F>,
) -> Option<usize>
where
    F: Float + kiddo::float::kdtree::Axis,
    C: SquareGrowContext<F> + ?Sized,
{
    let neighbours = collect_labelled_neighbours(cell, &grow.labelled, attempt.positions);
    if neighbours.is_empty() {
        sink.emit(Event::GrowRejected {
            coord: cell,
            reason: GrowRejectReason::NoCandidate,
        });
        return None;
    }

    let predict_ctx = PredictCtx {
        target_coord: cell,
        neighbours: &neighbours,
        global_axes: [attempt.axis_i, attempt.axis_j],
        global_cell_size: attempt.cell_size,
        local_step_fallback: true,
    };
    let prediction = match predict_from_neighbours(predict_ctx) {
        Some(p) => p,
        None => {
            sink.emit(Event::GrowRejected {
                coord: cell,
                reason: GrowRejectReason::NoCandidate,
            });
            return None;
        }
    };

    let candidates = collect_candidates(
        prediction.position,
        attempt.search_radius,
        attempt.tree,
        attempt.slot_to_idx,
        in_use,
        attempt.positions,
    );
    if candidates.is_empty() {
        sink.emit(Event::GrowRejected {
            coord: cell,
            reason: GrowRejectReason::NoCandidate,
        });
        return None;
    }

    let choice = match choose_unambiguous(&candidates, attempt.params.ambiguity_factor) {
        Ok(c) => c,
        Err(AmbiguityReason::Empty) => {
            sink.emit(Event::GrowRejected {
                coord: cell,
                reason: GrowRejectReason::NoCandidate,
            });
            return None;
        }
        Err(AmbiguityReason::TooClose {
            nearest,
            second,
            ratio,
        }) => {
            sink.emit(Event::GrowRejected {
                coord: cell,
                reason: GrowRejectReason::Ambiguous {
                    nearest,
                    second,
                    ratio,
                },
            });
            return None;
        }
    };

    let policy = attempt.ctx.label_policy();
    if !policy.is_eligible(choice.idx) {
        sink.emit(Event::GrowRejected {
            coord: cell,
            reason: GrowRejectReason::Ineligible,
        });
        return None;
    }
    if !policy.agrees(choice.idx, cell) {
        sink.emit(Event::GrowRejected {
            coord: cell,
            reason: GrowRejectReason::PolicyDisagreed,
        });
        return None;
    }
    if !cardinal_edges_ok(cell, choice.idx, grow, attempt) {
        sink.emit(Event::GrowRejected {
            coord: cell,
            reason: GrowRejectReason::EdgeFailure,
        });
        return None;
    }
    if !attempt.ctx.accept_candidate(cell, choice.idx) {
        sink.emit(Event::GrowRejected {
            coord: cell,
            reason: GrowRejectReason::PolicyDisagreed,
        });
        return None;
    }

    let residual = (candidates[0].position - prediction.position).norm();
    sink.emit(Event::GrowAttached {
        coord: cell,
        idx: choice.idx,
        residual,
    });
    Some(choice.idx)
}

fn cardinal_edges_ok<F, C>(
    cell: Coord,
    candidate_idx: usize,
    grow: &GrowResult<F>,
    attempt: &FillAttempt<'_, F, C>,
) -> bool
where
    F: Float + kiddo::float::kdtree::Axis,
    C: SquareGrowContext<F> + ?Sized,
{
    let mut found_any = false;
    let to_pos = attempt.positions[candidate_idx];
    for (di, dj) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let neigh = (cell.0 + di, cell.1 + dj);
        if let Some(&n_idx) = grow.labelled.get(&neigh) {
            found_any = true;
            let edge = EdgeCtx {
                from_coord: neigh,
                to_coord: cell,
                from_position: attempt.positions[n_idx],
                to_position: to_pos,
                from_idx: n_idx,
                to_idx: candidate_idx,
                global_cell_size: attempt.cell_size,
            };
            if attempt.ctx.edge_ok(edge) {
                return true;
            }
        }
    }
    !found_any
}

/// Collect the 8-connected labelled neighbours of `cell`, including central
/// differences along each axis when a forward + backward pair is available.
fn collect_labelled_neighbours<F: Float>(
    cell: Coord,
    labelled: &HashMap<Coord, usize>,
    positions: &[Point2<F>],
) -> Vec<LabelledNeighbour<F>> {
    let mut out = Vec::new();
    for dj in -1..=1 {
        for di in -1..=1 {
            if di == 0 && dj == 0 {
                continue;
            }
            let at = (cell.0 + di, cell.1 + dj);
            if let Some(&idx) = labelled.get(&at) {
                let mut nb = LabelledNeighbour::new(at, positions[idx]);
                if let Some(step) = local_step_at(at, (1, 0), labelled, positions) {
                    nb = nb.with_local_step_u(step);
                }
                if let Some(step) = local_step_at(at, (0, 1), labelled, positions) {
                    nb = nb.with_local_step_v(step);
                }
                out.push(nb);
            }
        }
    }
    out
}

fn local_step_at<F: Float>(
    at: Coord,
    step: (i32, i32),
    labelled: &HashMap<Coord, usize>,
    positions: &[Point2<F>],
) -> Option<Vector2<F>> {
    let here_idx = *labelled.get(&at)?;
    let here = positions[here_idx];
    let fwd = (at.0 + step.0, at.1 + step.1);
    let bwd = (at.0 - step.0, at.1 - step.1);
    let fwd_pos = labelled.get(&fwd).map(|&i| positions[i]);
    let bwd_pos = labelled.get(&bwd).map(|&i| positions[i]);
    match (fwd_pos, bwd_pos) {
        (Some(f), Some(b)) => Some((f - b) * lit::<F>(0.5_f32)),
        (Some(f), None) => Some(f - here),
        (None, Some(b)) => Some(here - b),
        (None, None) => None,
    }
}

/// Cells to try this pass: every unlabelled coord inside the labelled bbox,
/// plus the rows and columns extended outward by `line_extension_depth`.
fn enumerate_fill_cells(labelled: &HashMap<Coord, usize>, depth: i32) -> Vec<Coord> {
    if labelled.is_empty() {
        return Vec::new();
    }
    let mut min_i = i32::MAX;
    let mut max_i = i32::MIN;
    let mut min_j = i32::MAX;
    let mut max_j = i32::MIN;
    let mut rows: HashSet<i32> = HashSet::new();
    let mut cols: HashSet<i32> = HashSet::new();
    for &(i, j) in labelled.keys() {
        min_i = min_i.min(i);
        max_i = max_i.max(i);
        min_j = min_j.min(j);
        max_j = max_j.max(j);
        rows.insert(j);
        cols.insert(i);
    }
    let depth = depth.max(0);
    let mut out: HashSet<Coord> = HashSet::new();
    for j in min_j..=max_j {
        for i in min_i..=max_i {
            if !labelled.contains_key(&(i, j)) {
                out.insert((i, j));
            }
        }
    }
    for d in 1..=depth {
        for &j in &rows {
            out.insert((min_i - d, j));
            out.insert((max_i + d, j));
        }
        for &i in &cols {
            out.insert((i, min_j - d));
            out.insert((i, max_j + d));
        }
    }
    let mut v: Vec<Coord> = out.into_iter().collect();
    v.sort_unstable();
    v
}

fn build_unlabelled_tree<F, C>(
    observations: &[Observation<F>],
    grow: &GrowResult<F>,
    ctx: &C,
) -> (KdTree<F, 2>, Vec<usize>)
where
    F: Float + kiddo::float::kdtree::Axis,
    C: SquareGrowContext<F> + ?Sized,
{
    let mut tree: KdTree<F, 2> = KdTree::new();
    let mut slot_to_idx = Vec::new();
    let in_use: HashSet<usize> = grow.labelled.values().copied().collect();
    let policy = ctx.label_policy();
    for (idx, obs) in observations.iter().enumerate() {
        if in_use.contains(&idx) {
            continue;
        }
        if !policy.is_eligible(idx) {
            continue;
        }
        tree.add(&[obs.position.x, obs.position.y], slot_to_idx.len() as u64);
        slot_to_idx.push(idx);
    }
    (tree, slot_to_idx)
}

fn derive_axes_from_labelled<F: Float>(
    labelled: &HashMap<Coord, usize>,
    positions: &[Point2<F>],
    cell_size: F,
) -> (Vector2<F>, Vector2<F>) {
    let eps = lit::<F>(1e-6_f32);
    let mut axis_i = Vector2::new(F::one(), F::zero());
    let mut axis_j = Vector2::new(F::zero(), F::one());

    // Prefer the canonical (0, 0) -> (1, 0) and (0, 0) -> (0, 1) pair.
    let pick_pair = |a: Coord, b: Coord| -> Option<Vector2<F>> {
        let &ia = labelled.get(&a)?;
        let &ib = labelled.get(&b)?;
        let v = positions[ib] - positions[ia];
        let n = v.norm();
        if n > eps {
            Some(v / n)
        } else {
            None
        }
    };

    if let Some(u) = pick_pair((0, 0), (1, 0)) {
        axis_i = u;
    } else {
        for (&(i, j), _) in labelled.iter() {
            if let Some(u) = pick_pair((i, j), (i + 1, j)) {
                axis_i = u;
                break;
            }
        }
    }
    if let Some(v) = pick_pair((0, 0), (0, 1)) {
        axis_j = v;
    } else {
        for (&(i, j), _) in labelled.iter() {
            if let Some(v) = pick_pair((i, j), (i, j + 1)) {
                axis_j = v;
                break;
            }
        }
    }
    let _ = cell_size;
    (axis_i, axis_j)
}

fn refresh_bbox<F: Float>(grow: &mut GrowResult<F>) {
    if grow.labelled.is_empty() {
        grow.bbox = ((0, 0), (0, 0));
        return;
    }
    let mut min_i = i32::MAX;
    let mut max_i = i32::MIN;
    let mut min_j = i32::MAX;
    let mut max_j = i32::MIN;
    for &(i, j) in grow.labelled.keys() {
        min_i = min_i.min(i);
        max_i = max_i.max(i);
        min_j = min_j.min(j);
        max_j = max_j.max(j);
    }
    grow.bbox = ((min_i, min_j), (max_i, max_j));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::NoOpSink;
    use crate::grow::context::OpenContext;

    fn axis_aligned<F>(rows: i32, cols: i32, s: F) -> Vec<Observation<F>>
    where
        F: Float + kiddo::float::kdtree::Axis,
    {
        let mut out = Vec::with_capacity((rows * cols) as usize);
        let origin = lit::<F>(50.0_f32);
        for j in 0..rows {
            for i in 0..cols {
                let x = lit::<F>(i as f32) * s + origin;
                let y = lit::<F>(j as f32) * s + origin;
                out.push(Observation::new(Point2::new(x, y)));
            }
        }
        out
    }

    fn grow_result_with_holes<F: Float>(
        rows: i32,
        cols: i32,
        holes: &[Coord],
        cell_size: F,
    ) -> GrowResult<F> {
        let mut labelled: HashMap<Coord, usize> = HashMap::new();
        for j in 0..rows {
            for i in 0..cols {
                let coord = (i, j);
                if holes.contains(&coord) {
                    continue;
                }
                let idx = (j * cols + i) as usize;
                labelled.insert(coord, idx);
            }
        }
        let n_total = labelled.len();
        GrowResult {
            labelled,
            cell_size,
            bbox: ((0, 0), (cols - 1, rows - 1)),
            n_attached: n_total.saturating_sub(4),
            n_rejected: 0,
        }
    }

    fn assert_fill_recovers_interior_holes<F>()
    where
        F: Float + kiddo::float::kdtree::Axis,
    {
        let rows = 5_i32;
        let cols = 5_i32;
        let s = lit::<F>(20.0_f32);
        let obs = axis_aligned::<F>(rows, cols, s);
        let holes: Vec<Coord> = vec![(1, 1), (2, 2), (3, 1), (1, 3)];
        let mut grow = grow_result_with_holes::<F>(rows, cols, &holes, s);
        let ctx = OpenContext::<F>::new(obs.len());
        let mut sink = NoOpSink;
        let stats = fill_grid_holes(&obs, &mut grow, &FillParams::default(), &ctx, &mut sink);
        assert_eq!(stats.n_attached, holes.len());
        for h in &holes {
            assert!(grow.labelled.contains_key(h), "{h:?} not recovered");
        }
    }

    fn assert_fill_does_not_attach_off_grid_noise<F>()
    where
        F: Float + kiddo::float::kdtree::Axis,
    {
        let rows = 5_i32;
        let cols = 5_i32;
        let s = lit::<F>(20.0_f32);
        let mut obs = axis_aligned::<F>(rows, cols, s);
        // Inject 3 off-grid noise points sitting between cells.
        obs.push(Observation::new(Point2::new(
            lit::<F>(60.0_f32),
            lit::<F>(60.0_f32),
        )));
        obs.push(Observation::new(Point2::new(
            lit::<F>(85.0_f32),
            lit::<F>(95.0_f32),
        )));
        obs.push(Observation::new(Point2::new(
            lit::<F>(110.0_f32),
            lit::<F>(115.0_f32),
        )));
        let holes: Vec<Coord> = vec![(2, 2)];
        let mut grow = grow_result_with_holes::<F>(rows, cols, &holes, s);
        let ctx = OpenContext::<F>::new(obs.len());
        let mut sink = NoOpSink;
        let stats = fill_grid_holes(&obs, &mut grow, &FillParams::default(), &ctx, &mut sink);
        // Hole at (2, 2) should be recovered from observation index 12.
        assert!(grow.labelled.contains_key(&(2, 2)));
        // The three noise observations sit at indices 25, 26, 27.
        let labelled_idx: HashSet<usize> = grow.labelled.values().copied().collect();
        for noise_idx in 25..=27 {
            assert!(
                !labelled_idx.contains(&noise_idx),
                "off-grid noise {noise_idx} must not be labelled"
            );
        }
        assert_eq!(stats.n_attached, 1);
    }

    fn assert_fill_with_blocked_policy_attaches_none<F>()
    where
        F: Float + kiddo::float::kdtree::Axis,
    {
        // Build a labelled set with one hole, but mark the only candidate
        // (and every other observation) as ineligible. Fill must attach zero.
        use crate::policy::LabelPolicy;
        let rows = 3_i32;
        let cols = 3_i32;
        let s = lit::<F>(20.0_f32);
        let obs = axis_aligned::<F>(rows, cols, s);
        let holes: Vec<Coord> = vec![(1, 1)];
        let mut grow = grow_result_with_holes::<F>(rows, cols, &holes, s);

        // A policy where every observation is ineligible.
        struct BlockedContext<F: Float>(LabelPolicy<F>);
        impl<F: Float> SquareGrowContext<F> for BlockedContext<F> {
            fn label_policy(&self) -> &LabelPolicy<F> {
                &self.0
            }
        }
        let mut builder = LabelPolicy::<F>::builder(obs.len());
        for i in 0..obs.len() {
            builder = builder.with_eligibility(i, false);
        }
        let ctx = BlockedContext(builder.build());
        let mut sink = NoOpSink;
        let stats = fill_grid_holes(&obs, &mut grow, &FillParams::default(), &ctx, &mut sink);
        assert_eq!(stats.n_attached, 0);
        assert!(!grow.labelled.contains_key(&(1, 1)));
    }

    #[test]
    fn fill_recovers_interior_holes_f32() {
        assert_fill_recovers_interior_holes::<f32>();
    }
    #[test]
    fn fill_recovers_interior_holes_f64() {
        assert_fill_recovers_interior_holes::<f64>();
    }
    #[test]
    fn fill_does_not_attach_off_grid_noise_f32() {
        assert_fill_does_not_attach_off_grid_noise::<f32>();
    }
    #[test]
    fn fill_does_not_attach_off_grid_noise_f64() {
        assert_fill_does_not_attach_off_grid_noise::<f64>();
    }
    #[test]
    fn fill_with_blocked_policy_attaches_none_f32() {
        assert_fill_with_blocked_policy_attaches_none::<f32>();
    }
    #[test]
    fn fill_with_blocked_policy_attaches_none_f64() {
        assert_fill_with_blocked_policy_attaches_none::<f64>();
    }
}
