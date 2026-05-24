//! BFS engine for square seed-and-grow.
//!
//! Successor to the legacy `square::grow::bfs_grow`. The legacy file was
//! 1007 LOC mixing BFS, prediction, KD-tree management, validator dispatch,
//! and rebasing; the new design extracts:
//!
//! * prediction → [`predict`](super::predict)
//! * candidate acceptance → [`attach`](super::attach)
//! * context trait + open default → [`context`](super::context)
//! * params + result → [`params`](super::params)
//!
//! Leaves this file focused on the BFS loop itself plus its private
//! scaffolding (mutable state, per-attempt context, helpers).
//!
//! ## Events
//!
//! Emits `Event::StageStarted { stage: Grow }`,
//! `Event::StageFinished { stage: Grow, duration }`,
//! `Event::GrowAttached`, and `Event::GrowRejected` for every BFS step.
//! `Event::GrowAttempted` is gated behind
//! [`GrowParams::emit_growth_attempted`] (off by default — the event is
//! high-volume on large grids).

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

use kiddo::KdTree;
use nalgebra::{Point2, Vector2};

use crate::diagnostics::{DiagnosticSink, Event, GrowRejectReason, Stage};
use crate::feature::Observation;
use crate::float::{lit, Float};
use crate::lattice::Coord;
use crate::seed::SeedOutput;

use super::attach::{choose_unambiguous, collect_candidates, AmbiguityReason};
use super::context::SquareGrowContext;
use super::params::{GrowParams, GrowResult};
use super::predict::{predict_from_neighbours, LabelledNeighbour, PredictCtx};

/// Grow a labelled `(i, j)` grid from a 2×2 seed using BFS over the lattice
/// boundary.
///
/// Emits `Event::StageStarted { stage: Grow }`,
/// `Event::StageFinished`, plus per-step `Event::GrowAttached` and
/// `Event::GrowRejected` events. The returned grid is rebased so the
/// bounding-box minimum is `(0, 0)`.
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "info",
        skip_all,
        fields(num_observations = observations.len(), cell_size = ?seed.cell_size),
    )
)]
pub fn bfs_grow<F, C>(
    observations: &[Observation<F>],
    seed: &SeedOutput<F>,
    params: &GrowParams<F>,
    ctx: &C,
    sink: &mut impl DiagnosticSink<F>,
) -> GrowResult<F>
where
    F: Float + kiddo::float::kdtree::Axis,
    C: SquareGrowContext<F> + ?Sized,
{
    let start = Instant::now();
    sink.emit(Event::StageStarted { stage: Stage::Grow });

    let cell_size = seed.cell_size;
    let (axis_i, axis_j) = derive_seed_axes(observations, seed);
    let (tree, slot_to_idx) = build_eligible_tree(observations, ctx);

    let mut state = GrowState::with_seed(seed);
    let mut n_rejected = 0usize;
    let search_radius = params.attach_search_rel * cell_size;

    while let Some(coord) = state.boundary.pop_front() {
        if state.labelled.contains_key(&coord) {
            continue;
        }
        let attempt = AttemptCtx {
            observations,
            ctx,
            params,
            tree: &tree,
            slot_to_idx: &slot_to_idx,
            cell_size,
            axis_i,
            axis_j,
            search_radius,
        };
        match try_attach(coord, &state, &attempt, sink) {
            Some(idx) => {
                state.attach(coord, idx);
                if params.emit_growth_attempted {
                    sink.emit(Event::GrowAttempted {
                        from: coord,
                        to: coord,
                        idx: Some(idx),
                    });
                }
            }
            None => {
                n_rejected += 1;
            }
        }
    }

    let (rebased_labelled, bbox, n_attached) = rebase_and_summarise(state.labelled);

    let duration = start.elapsed();
    sink.emit(Event::StageFinished {
        stage: Stage::Grow,
        duration,
    });

    GrowResult {
        labelled: rebased_labelled,
        cell_size,
        bbox,
        n_attached,
        n_rejected,
    }
}

// ---- Internal scaffolding (private) ----

/// Mutable BFS state: labelled map, observations-in-use set, queue.
struct GrowState {
    labelled: HashMap<Coord, usize>,
    by_obs: HashSet<usize>,
    boundary: VecDeque<Coord>,
    enqueued: HashSet<Coord>,
}

impl GrowState {
    fn with_seed<F: Float>(seed: &SeedOutput<F>) -> Self {
        let mut state = Self {
            labelled: HashMap::new(),
            by_obs: HashSet::new(),
            boundary: VecDeque::new(),
            enqueued: HashSet::new(),
        };
        for (coord, idx) in [
            ((0, 0), seed.seed.a),
            ((1, 0), seed.seed.b),
            ((0, 1), seed.seed.c),
            ((1, 1), seed.seed.d),
        ] {
            state.labelled.insert(coord, idx);
            state.by_obs.insert(idx);
        }
        for coord in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            state.enqueue_cardinal(coord);
        }
        state
    }

    fn enqueue_cardinal(&mut self, coord: Coord) {
        for (di, dj) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let next = (coord.0 + di, coord.1 + dj);
            if self.labelled.contains_key(&next) {
                continue;
            }
            if self.enqueued.insert(next) {
                self.boundary.push_back(next);
            }
        }
    }

    fn attach(&mut self, coord: Coord, idx: usize) {
        self.labelled.insert(coord, idx);
        self.by_obs.insert(idx);
        self.enqueue_cardinal(coord);
    }
}

/// Per-attempt context bundle; stays under the workspace `too_many_arguments`
/// lint by carrying every shared reference through one struct.
struct AttemptCtx<'a, F, C>
where
    F: Float + kiddo::float::kdtree::Axis,
    C: SquareGrowContext<F> + ?Sized,
{
    observations: &'a [Observation<F>],
    ctx: &'a C,
    params: &'a GrowParams<F>,
    tree: &'a KdTree<F, 2>,
    slot_to_idx: &'a [usize],
    cell_size: F,
    axis_i: Vector2<F>,
    axis_j: Vector2<F>,
    search_radius: F,
}

fn try_attach<F, C>(
    coord: Coord,
    state: &GrowState,
    attempt: &AttemptCtx<'_, F, C>,
    sink: &mut impl DiagnosticSink<F>,
) -> Option<usize>
where
    F: Float + kiddo::float::kdtree::Axis,
    C: SquareGrowContext<F> + ?Sized,
{
    let neighbours = collect_labelled_neighbours(coord, state, attempt.observations);
    if neighbours.is_empty() {
        emit_grow_rejected(sink, coord, GrowRejectReason::NoCandidate);
        return None;
    }

    let predict_ctx = PredictCtx {
        target_coord: coord,
        neighbours: &neighbours,
        global_axes: [attempt.axis_i, attempt.axis_j],
        global_cell_size: attempt.cell_size,
        local_step_fallback: attempt.params.local_step_fallback,
    };
    let Some(prediction) = predict_from_neighbours(predict_ctx) else {
        emit_grow_rejected(sink, coord, GrowRejectReason::NoCandidate);
        return None;
    };

    let positions: Vec<Point2<F>> = attempt.observations.iter().map(|o| o.position).collect();
    let candidates = collect_candidates(
        prediction.position,
        attempt.search_radius,
        attempt.tree,
        attempt.slot_to_idx,
        &state.by_obs,
        &positions,
    );
    if candidates.is_empty() {
        emit_grow_rejected(sink, coord, GrowRejectReason::NoCandidate);
        return None;
    }

    let choice = match choose_unambiguous(&candidates, attempt.params.attach_ambiguity_factor) {
        Ok(c) => c,
        Err(AmbiguityReason::Empty) => {
            emit_grow_rejected(sink, coord, GrowRejectReason::NoCandidate);
            return None;
        }
        Err(AmbiguityReason::TooClose {
            nearest,
            second,
            ratio,
        }) => {
            emit_grow_rejected(
                sink,
                coord,
                GrowRejectReason::Ambiguous {
                    nearest,
                    second,
                    ratio,
                },
            );
            return None;
        }
    };

    let policy = attempt.ctx.label_policy();
    if !policy.is_eligible(choice.idx) {
        emit_grow_rejected(sink, coord, GrowRejectReason::Ineligible);
        return None;
    }
    if !policy.agrees(choice.idx, coord) {
        emit_grow_rejected(sink, coord, GrowRejectReason::PolicyDisagreed);
        return None;
    }

    if !cardinal_edges_ok(coord, choice.idx, state, attempt) {
        emit_grow_rejected(sink, coord, GrowRejectReason::EdgeFailure);
        return None;
    }
    if !attempt.ctx.accept_candidate(coord, choice.idx) {
        emit_grow_rejected(sink, coord, GrowRejectReason::PolicyDisagreed);
        return None;
    }

    let residual = (candidates[0].position - prediction.position).norm();
    sink.emit(Event::GrowAttached {
        coord,
        idx: choice.idx,
        residual,
    });
    Some(choice.idx)
}

fn emit_grow_rejected<F: Float>(
    sink: &mut impl DiagnosticSink<F>,
    coord: Coord,
    reason: GrowRejectReason<F>,
) {
    sink.emit(Event::GrowRejected { coord, reason });
}

fn collect_labelled_neighbours<F: Float>(
    coord: Coord,
    state: &GrowState,
    observations: &[Observation<F>],
) -> Vec<LabelledNeighbour<F>> {
    let mut out = Vec::new();
    for dj in -1..=1 {
        for di in -1..=1 {
            if di == 0 && dj == 0 {
                continue;
            }
            let at = (coord.0 + di, coord.1 + dj);
            if let Some(&idx) = state.labelled.get(&at) {
                let mut nb = LabelledNeighbour::new(at, observations[idx].position);
                if let Some(step) = local_step_at(at, (1, 0), state, observations) {
                    nb = nb.with_local_step_u(step);
                }
                if let Some(step) = local_step_at(at, (0, 1), state, observations) {
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
    state: &GrowState,
    observations: &[Observation<F>],
) -> Option<Vector2<F>> {
    let here_idx = *state.labelled.get(&at)?;
    let here = observations[here_idx].position;
    let fwd = (at.0 + step.0, at.1 + step.1);
    let bwd = (at.0 - step.0, at.1 - step.1);
    let fwd_pos = state.labelled.get(&fwd).map(|&i| observations[i].position);
    let bwd_pos = state.labelled.get(&bwd).map(|&i| observations[i].position);
    match (fwd_pos, bwd_pos) {
        (Some(f), Some(b)) => Some((f - b) * lit::<F>(0.5_f32)),
        (Some(f), None) => Some(f - here),
        (None, Some(b)) => Some(here - b),
        (None, None) => None,
    }
}

fn cardinal_edges_ok<F, C>(
    coord: Coord,
    candidate_idx: usize,
    state: &GrowState,
    attempt: &AttemptCtx<'_, F, C>,
) -> bool
where
    F: Float + kiddo::float::kdtree::Axis,
    C: SquareGrowContext<F> + ?Sized,
{
    let mut found_any = false;
    let to_pos = attempt.observations[candidate_idx].position;
    for (di, dj) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let neigh = (coord.0 + di, coord.1 + dj);
        if let Some(&n_idx) = state.labelled.get(&neigh) {
            found_any = true;
            let edge = super::context::EdgeCtx {
                from_coord: neigh,
                to_coord: coord,
                from_position: attempt.observations[n_idx].position,
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

fn derive_seed_axes<F: Float>(
    observations: &[Observation<F>],
    seed: &SeedOutput<F>,
) -> (Vector2<F>, Vector2<F>) {
    let eps = lit::<F>(1e-6_f32);
    let a = observations[seed.seed.a].position;
    let b = observations[seed.seed.b].position;
    let c = observations[seed.seed.c].position;
    let raw_u = b - a;
    let raw_v = c - a;
    let nu = raw_u.norm();
    let nv = raw_v.norm();
    let nu = if nu > eps { nu } else { eps };
    let nv = if nv > eps { nv } else { eps };
    (raw_u / nu, raw_v / nv)
}

fn build_eligible_tree<F, C>(observations: &[Observation<F>], ctx: &C) -> (KdTree<F, 2>, Vec<usize>)
where
    F: Float + kiddo::float::kdtree::Axis,
    C: SquareGrowContext<F> + ?Sized,
{
    let mut tree: KdTree<F, 2> = KdTree::new();
    let mut slot_to_idx = Vec::new();
    let policy = ctx.label_policy();
    for (idx, obs) in observations.iter().enumerate() {
        if policy.is_eligible(idx) {
            tree.add(&[obs.position.x, obs.position.y], slot_to_idx.len() as u64);
            slot_to_idx.push(idx);
        }
    }
    (tree, slot_to_idx)
}

fn rebase_and_summarise(
    labelled: HashMap<Coord, usize>,
) -> (HashMap<Coord, usize>, (Coord, Coord), usize) {
    if labelled.is_empty() {
        return (HashMap::new(), ((0, 0), (0, 0)), 0);
    }
    let (min_i, min_j) = labelled
        .keys()
        .fold((i32::MAX, i32::MAX), |(a, b), &(i, j)| (a.min(i), b.min(j)));
    let (max_i, max_j) = labelled
        .keys()
        .fold((i32::MIN, i32::MIN), |(a, b), &(i, j)| (a.max(i), b.max(j)));
    let rebased: HashMap<Coord, usize> = labelled
        .into_iter()
        .map(|((i, j), idx)| ((i - min_i, j - min_j), idx))
        .collect();
    let bbox = ((0, 0), (max_i - min_i, max_j - min_j));
    let total = rebased.len();
    // Four come from the seed; the rest are BFS-attached.
    let n_attached = total.saturating_sub(4);
    (rebased, bbox, n_attached)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::{NoOpSink, RecordingSink};
    use crate::grow::context::OpenContext;
    use crate::seed::{Seed, SeedOutput};

    fn axis_aligned_grid<F>(rows: i32, cols: i32, s: F) -> Vec<Observation<F>>
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

    fn seed_first_2x2<F>(observations: &[Observation<F>], cols: i32) -> SeedOutput<F>
    where
        F: Float + kiddo::float::kdtree::Axis,
    {
        // For a row-major grid with `cols` per row, the (i, j) -> idx map is
        // j * cols + i. The first 2×2 quad is (0,0)=0, (1,0)=1, (0,1)=cols,
        // (1,1)=cols+1.
        let c = cols as usize;
        let a = observations[0].position;
        let b = observations[1].position;
        let cell = (b - a).norm();
        SeedOutput::new(Seed::new(0, 1, c, c + 1), cell)
    }

    fn assert_open_context_grows_clean_grid<F>()
    where
        F: Float + kiddo::float::kdtree::Axis,
    {
        let s = lit::<F>(20.0_f32);
        let rows = 6_i32;
        let cols = 6_i32;
        let obs = axis_aligned_grid::<F>(rows, cols, s);
        let seed = seed_first_2x2(&obs, cols);
        let ctx = OpenContext::<F>::new(obs.len());
        let mut sink = NoOpSink;
        let result = bfs_grow(&obs, &seed, &GrowParams::default(), &ctx, &mut sink);
        assert_eq!(result.labelled.len(), (rows * cols) as usize);
        assert_eq!(result.bbox, ((0, 0), (cols - 1, rows - 1)));
        let (mi, mj) = result
            .labelled
            .keys()
            .fold((i32::MAX, i32::MAX), |(a, b), &(i, j)| (a.min(i), b.min(j)));
        assert_eq!((mi, mj), (0, 0));
    }

    fn assert_emits_stage_and_attached_events<F>()
    where
        F: Float + kiddo::float::kdtree::Axis,
    {
        let s = lit::<F>(20.0_f32);
        let cols = 4_i32;
        let obs = axis_aligned_grid::<F>(4, cols, s);
        let seed = seed_first_2x2(&obs, cols);
        let ctx = OpenContext::<F>::new(obs.len());
        let mut sink = RecordingSink::<F>::new();
        bfs_grow(&obs, &seed, &GrowParams::default(), &ctx, &mut sink);
        let events = sink.events();
        assert!(events
            .iter()
            .any(|e| matches!(e, Event::StageStarted { stage: Stage::Grow })));
        assert!(events.iter().any(|e| matches!(
            e,
            Event::StageFinished {
                stage: Stage::Grow,
                ..
            }
        )));
        let attached_count = events
            .iter()
            .filter(|e| matches!(e, Event::GrowAttached { .. }))
            .count();
        // 4 seed corners + BFS attaches the remaining 12 -> 12 attach events.
        assert_eq!(attached_count, 12);
    }

    fn assert_rebases_origin_when_seed_off_zero<F>()
    where
        F: Float + kiddo::float::kdtree::Axis,
    {
        // Synthetic 4x4 grid; pick seed at (2,2)/(3,2)/(2,3)/(3,3) to force
        // a non-zero rebase.
        let s = lit::<F>(20.0_f32);
        let rows = 4_i32;
        let cols = 4_i32;
        let obs = axis_aligned_grid::<F>(rows, cols, s);
        let a = 2 + 2 * cols as usize;
        let b = 3 + 2 * cols as usize;
        let c = 2 + 3 * cols as usize;
        let d = 3 + 3 * cols as usize;
        let cell = (obs[b].position - obs[a].position).norm();
        let seed = SeedOutput::new(Seed::new(a, b, c, d), cell);
        let ctx = OpenContext::<F>::new(obs.len());
        let mut sink = NoOpSink;
        let result = bfs_grow(&obs, &seed, &GrowParams::default(), &ctx, &mut sink);
        assert_eq!(result.labelled.len(), (rows * cols) as usize);
        // Rebase must always make min = (0, 0).
        let (mi, mj) = result
            .labelled
            .keys()
            .fold((i32::MAX, i32::MAX), |(a, b), &(i, j)| (a.min(i), b.min(j)));
        assert_eq!((mi, mj), (0, 0));
    }

    #[test]
    fn open_context_grows_clean_grid_f32() {
        assert_open_context_grows_clean_grid::<f32>();
    }
    #[test]
    fn open_context_grows_clean_grid_f64() {
        assert_open_context_grows_clean_grid::<f64>();
    }
    #[test]
    fn emits_stage_and_attached_events_f32() {
        assert_emits_stage_and_attached_events::<f32>();
    }
    #[test]
    fn emits_stage_and_attached_events_f64() {
        assert_emits_stage_and_attached_events::<f64>();
    }
    #[test]
    fn rebases_origin_when_seed_off_zero_f32() {
        assert_rebases_origin_when_seed_off_zero::<f32>();
    }
    #[test]
    fn rebases_origin_when_seed_off_zero_f64() {
        assert_rebases_origin_when_seed_off_zero::<f64>();
    }
}
