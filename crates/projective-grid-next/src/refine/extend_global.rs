//! Boundary extension via a global homography fit.
//!
//! Fits a single planar homography `H : (i, j) → pixel` from the labelled
//! set and walks the boundary outward by projecting integer cells through `H`,
//! gating each attachment via the same `SquareGrowContext` hooks as the BFS
//! engine.
//!
//! The global fit is cheap and reads well on uniform images. Under heavy
//! radial distortion (or split-region perspective) one global H cannot fit
//! the whole board; in that case the median-residual gate refuses to
//! extrapolate. Callers that need a more forgiving extender should use the
//! per-cell variant in [`crate::refine::extend_local`].

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use kiddo::KdTree;
use nalgebra::Point2;

use crate::diagnostics::{DiagnosticSink, Event, GrowRejectReason, Stage};
use crate::feature::Observation;
use crate::float::{lit, Float};
use crate::geometry::homography::{estimate_homography, Homography};
use crate::grow::attach::{choose_unambiguous, collect_candidates, AmbiguityReason};
use crate::grow::context::{EdgeCtx, SquareGrowContext};
use crate::grow::params::GrowResult;
use crate::lattice::Coord;

/// Tunables for [`extend_via_global_homography`].
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct ExtensionParams<F: Float> {
    /// Minimum labelled count below which the function returns early — the
    /// DLT solve is under-determined when the labelled set is too small.
    /// Default `12` (three times the 4-DOF minimum, matching the legacy
    /// crate's `min_labels_for_h`).
    pub min_labels_for_h: usize,
    /// Maximum median reprojection residual / `cell_size` on the labelled
    /// support set. The pass refuses to extrapolate when this gate fails.
    /// Default `0.10`.
    pub max_median_residual_rel: F,
    /// Maximum worst-case reprojection residual / `cell_size` on the labelled
    /// support set. Default `0.30`.
    pub max_max_residual_rel: F,
    /// Per-candidate search radius around each projected cell position,
    /// relative to `cell_size`. Default `0.40`.
    pub search_rel: F,
    /// Acceptance ambiguity factor: `second >= ambiguity_factor * nearest`.
    /// Default `2.5` (tighter than BFS's `1.3` because boundary errors are
    /// less recoverable downstream).
    pub ambiguity_factor: F,
    /// Maximum number of extension passes. Default `5`.
    pub max_iters: usize,
    /// How far past the labelled bbox each row / column may extend per pass.
    /// Default `1`.
    pub max_extension_depth: i32,
}

impl<F: Float> Default for ExtensionParams<F> {
    fn default() -> Self {
        Self {
            min_labels_for_h: 12,
            max_median_residual_rel: lit::<F>(0.10_f32),
            max_max_residual_rel: lit::<F>(0.30_f32),
            search_rel: lit::<F>(0.40_f32),
            ambiguity_factor: lit::<F>(2.5_f32),
            max_iters: 5,
            max_extension_depth: 1,
        }
    }
}

impl<F: Float> ExtensionParams<F> {
    /// Construct extension params from the residual gate and search radius;
    /// other knobs take their defaults.
    pub fn new(
        max_median_residual_rel: F,
        max_max_residual_rel: F,
        search_rel: F,
        ambiguity_factor: F,
    ) -> Self {
        Self {
            max_median_residual_rel,
            max_max_residual_rel,
            search_rel,
            ambiguity_factor,
            ..Self::default()
        }
    }

    /// Override the minimum labelled count.
    #[must_use]
    pub fn with_min_labels_for_h(mut self, n: usize) -> Self {
        self.min_labels_for_h = n;
        self
    }

    /// Override the maximum extension iteration count.
    #[must_use]
    pub fn with_max_iters(mut self, n: usize) -> Self {
        self.max_iters = n;
        self
    }

    /// Override the maximum extension depth per iteration.
    #[must_use]
    pub fn with_max_extension_depth(mut self, depth: i32) -> Self {
        self.max_extension_depth = depth;
        self
    }
}

/// Counters returned by either extension strategy.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct ExtensionStats {
    /// Number of corners attached across all iterations.
    pub n_attached: usize,
    /// Number of candidate cells that failed at least one acceptance gate.
    pub n_rejected: usize,
    /// Median reprojection residual / cell_size on the labelled support set,
    /// or `None` when the labelled set was too small to fit a homography.
    pub median_residual_rel: Option<f64>,
    /// Whether the homography passed the residual gate. `false` when the
    /// pass returned early either for too-few labels or for failing the
    /// residual threshold.
    pub h_trusted: bool,
    /// Number of extension passes actually run.
    pub iterations: usize,
}

/// Extend the labelled grid outward using a globally-fit homography.
///
/// Mutates `grow.labelled` in place. Emits `Event::StageStarted/Finished
/// { stage: Refine }` bookends plus per-cell `Event::GrowAttached` /
/// `Event::GrowRejected` events.
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "info",
        skip_all,
        fields(num_observations = observations.len(), n_labelled = grow.labelled.len()),
    )
)]
pub fn extend_via_global_homography<F, C>(
    observations: &[Observation<F>],
    grow: &mut GrowResult<F>,
    params: &ExtensionParams<F>,
    ctx: &C,
    sink: &mut impl DiagnosticSink<F>,
) -> ExtensionStats
where
    F: Float + kiddo::float::kdtree::Axis,
    C: SquareGrowContext<F> + ?Sized,
{
    let start = Instant::now();
    sink.emit(Event::StageStarted {
        stage: Stage::Refine,
    });

    let mut stats = ExtensionStats::default();

    if grow.labelled.len() < params.min_labels_for_h {
        sink.emit(Event::StageFinished {
            stage: Stage::Refine,
            duration: start.elapsed(),
        });
        return stats;
    }

    let positions: Vec<Point2<F>> = observations.iter().map(|o| o.position).collect();
    let cell_size = grow.cell_size;

    // Fit the homography from `(i, j)` → pixel.
    let mut grid_pts: Vec<Point2<F>> = Vec::with_capacity(grow.labelled.len());
    let mut img_pts: Vec<Point2<F>> = Vec::with_capacity(grow.labelled.len());
    for (&coord, &idx) in &grow.labelled {
        grid_pts.push(Point2::new(
            lit::<F>(coord.0 as f32),
            lit::<F>(coord.1 as f32),
        ));
        img_pts.push(positions[idx]);
    }
    let Some(h) = estimate_homography(&grid_pts, &img_pts) else {
        sink.emit(Event::StageFinished {
            stage: Stage::Refine,
            duration: start.elapsed(),
        });
        return stats;
    };

    // Reprojection residuals on the labelled support set.
    let (median_res, max_res) = reprojection_residuals(&h, &grid_pts, &img_pts);
    stats.median_residual_rel = Some(scalar_to_f64(median_res / cell_size));
    let median_thresh = params.max_median_residual_rel * cell_size;
    let max_thresh = params.max_max_residual_rel * cell_size;
    if median_res > median_thresh || max_res > max_thresh {
        sink.emit(Event::StageFinished {
            stage: Stage::Refine,
            duration: start.elapsed(),
        });
        return stats;
    }
    stats.h_trusted = true;

    let search_radius = params.search_rel * cell_size;

    for _ in 0..params.max_iters {
        let (tree, slot_to_idx) = build_unlabelled_tree(observations, grow, ctx);
        let in_use: HashSet<usize> = grow.labelled.values().copied().collect();
        let cells = enumerate_extension_cells(&grow.labelled, params.max_extension_depth);
        let mut attached_this_iter = 0usize;
        for cell in cells {
            if grow.labelled.contains_key(&cell) {
                continue;
            }
            let cell_grid_pt = Point2::new(lit::<F>(cell.0 as f32), lit::<F>(cell.1 as f32));
            let prediction = h.apply(cell_grid_pt);
            let candidates = collect_candidates(
                prediction,
                search_radius,
                &tree,
                &slot_to_idx,
                &in_use,
                &positions,
            );
            if candidates.is_empty() {
                sink.emit(Event::GrowRejected {
                    coord: cell,
                    reason: GrowRejectReason::NoCandidate,
                });
                stats.n_rejected += 1;
                continue;
            }
            let choice = match choose_unambiguous(&candidates, params.ambiguity_factor) {
                Ok(c) => c,
                Err(AmbiguityReason::Empty) => {
                    sink.emit(Event::GrowRejected {
                        coord: cell,
                        reason: GrowRejectReason::NoCandidate,
                    });
                    stats.n_rejected += 1;
                    continue;
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
                    stats.n_rejected += 1;
                    continue;
                }
            };

            let policy = ctx.label_policy();
            if !policy.is_eligible(choice.idx) {
                sink.emit(Event::GrowRejected {
                    coord: cell,
                    reason: GrowRejectReason::Ineligible,
                });
                stats.n_rejected += 1;
                continue;
            }
            if !policy.agrees(choice.idx, cell) {
                sink.emit(Event::GrowRejected {
                    coord: cell,
                    reason: GrowRejectReason::PolicyDisagreed,
                });
                stats.n_rejected += 1;
                continue;
            }
            if !cardinal_edges_ok(cell, choice.idx, grow, &positions, ctx, cell_size) {
                sink.emit(Event::GrowRejected {
                    coord: cell,
                    reason: GrowRejectReason::EdgeFailure,
                });
                stats.n_rejected += 1;
                continue;
            }
            if !ctx.accept_candidate(cell, choice.idx) {
                sink.emit(Event::GrowRejected {
                    coord: cell,
                    reason: GrowRejectReason::PolicyDisagreed,
                });
                stats.n_rejected += 1;
                continue;
            }

            let residual = (candidates[0].position - prediction).norm();
            sink.emit(Event::GrowAttached {
                coord: cell,
                idx: choice.idx,
                residual,
            });
            grow.labelled.insert(cell, choice.idx);
            grow.n_attached += 1;
            stats.n_attached += 1;
            attached_this_iter += 1;
        }
        stats.iterations += 1;
        if attached_this_iter == 0 {
            break;
        }
    }

    refresh_bbox(grow);
    sink.emit(Event::StageFinished {
        stage: Stage::Refine,
        duration: start.elapsed(),
    });
    stats
}

// ---- Helpers shared with extend_local (re-imported privately there) ----

pub(super) fn reprojection_residuals<F: Float>(
    h: &Homography<F>,
    grid_pts: &[Point2<F>],
    img_pts: &[Point2<F>],
) -> (F, F) {
    let mut residuals: Vec<F> = grid_pts
        .iter()
        .zip(img_pts.iter())
        .map(|(g, p)| {
            let pred = h.apply(*g);
            ((pred.x - p.x).powi(2) + (pred.y - p.y).powi(2)).sqrt()
        })
        .collect();
    residuals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = if residuals.is_empty() {
        F::zero()
    } else {
        residuals[residuals.len() / 2]
    };
    let max = residuals
        .iter()
        .copied()
        .fold(F::zero(), |acc, x| if x > acc { x } else { acc });
    (median, max)
}

pub(super) fn scalar_to_f64<F: Float>(v: F) -> f64 {
    // Every `F: Float` is a superset of `f64` (both `f32` and `f64` round-trip
    // through `convert_unchecked` losslessly within their representable range).
    // Same routing used by `topological::delaunay::point_to_f64`.
    nalgebra::convert_unchecked::<F, f64>(v)
}

pub(super) fn enumerate_extension_cells(
    labelled: &HashMap<Coord, usize>,
    depth: i32,
) -> Vec<Coord> {
    if labelled.is_empty() || depth < 1 {
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
    let mut out: HashSet<Coord> = HashSet::new();
    // Interior holes.
    for j in min_j..=max_j {
        for i in min_i..=max_i {
            if !labelled.contains_key(&(i, j)) {
                out.insert((i, j));
            }
        }
    }
    // Per-row / per-column extension.
    for d in 1..=depth {
        for &j in &rows {
            out.insert((min_i - d, j));
            out.insert((max_i + d, j));
        }
        for &i in &cols {
            out.insert((i, min_j - d));
            out.insert((i, max_j + d));
        }
        for d2 in 1..=depth {
            out.insert((min_i - d, min_j - d2));
            out.insert((min_i - d, max_j + d2));
            out.insert((max_i + d, min_j - d2));
            out.insert((max_i + d, max_j + d2));
        }
    }
    let mut v: Vec<Coord> = out.into_iter().collect();
    v.sort_unstable();
    v
}

pub(super) fn build_unlabelled_tree<F, C>(
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

pub(super) fn cardinal_edges_ok<F, C>(
    cell: Coord,
    candidate_idx: usize,
    grow: &GrowResult<F>,
    positions: &[Point2<F>],
    ctx: &C,
    cell_size: F,
) -> bool
where
    F: Float + kiddo::float::kdtree::Axis,
    C: SquareGrowContext<F> + ?Sized,
{
    let mut found_any = false;
    let to_pos = positions[candidate_idx];
    for (di, dj) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let neigh = (cell.0 + di, cell.1 + dj);
        if let Some(&n_idx) = grow.labelled.get(&neigh) {
            found_any = true;
            let edge = EdgeCtx {
                from_coord: neigh,
                to_coord: cell,
                from_position: positions[n_idx],
                to_position: to_pos,
                from_idx: n_idx,
                to_idx: candidate_idx,
                global_cell_size: cell_size,
            };
            if ctx.edge_ok(edge) {
                return true;
            }
        }
    }
    !found_any
}

pub(super) fn refresh_bbox<F: Float>(grow: &mut GrowResult<F>) {
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

    fn synthetic_grid<F>(rows: i32, cols: i32, s: F) -> Vec<Observation<F>>
    where
        F: Float + kiddo::float::kdtree::Axis,
    {
        let mut out = Vec::with_capacity((rows * cols) as usize);
        for j in 0..rows {
            for i in 0..cols {
                let x = lit::<F>(i as f32) * s + lit::<F>(100.0_f32);
                let y = lit::<F>(j as f32) * s + lit::<F>(50.0_f32);
                out.push(Observation::new(Point2::new(x, y)));
            }
        }
        out
    }

    fn label_subgrid<F: Float>(
        cols: i32,
        i_range: std::ops::Range<i32>,
        j_range: std::ops::Range<i32>,
        cell_size: F,
    ) -> GrowResult<F> {
        let mut labelled = HashMap::new();
        for j in j_range {
            for i in i_range.clone() {
                let idx = (j * cols + i) as usize;
                labelled.insert((i, j), idx);
            }
        }
        let n_total = labelled.len();
        GrowResult {
            labelled,
            cell_size,
            bbox: ((0, 0), (cols - 1, cols - 1)),
            n_attached: n_total.saturating_sub(4),
            n_rejected: 0,
        }
    }

    fn assert_extends_clean_perspective_grid<F>()
    where
        F: Float + kiddo::float::kdtree::Axis,
    {
        let cols = 6_i32;
        let rows = 4_i32;
        let s = lit::<F>(50.0_f32);
        let obs = synthetic_grid::<F>(rows, cols, s);
        let mut grow = label_subgrid::<F>(cols, 1..5, 1..3, s);
        let starting = grow.labelled.len();
        assert_eq!(starting, 8);
        let ctx = OpenContext::<F>::new(obs.len());
        let mut sink = NoOpSink;
        let stats = extend_via_global_homography(
            &obs,
            &mut grow,
            &ExtensionParams::default().with_min_labels_for_h(4),
            &ctx,
            &mut sink,
        );
        assert!(stats.h_trusted, "H must be trusted on clean affine grid");
        assert!(
            grow.labelled.len() > starting,
            "extension should attach at least one corner"
        );
    }

    fn assert_no_op_when_too_few_labels<F>()
    where
        F: Float + kiddo::float::kdtree::Axis,
    {
        let cols = 4_i32;
        let rows = 4_i32;
        let s = lit::<F>(50.0_f32);
        let obs = synthetic_grid::<F>(rows, cols, s);
        let mut grow = label_subgrid::<F>(cols, 0..2, 0..2, s);
        let ctx = OpenContext::<F>::new(obs.len());
        let mut sink = NoOpSink;
        let stats = extend_via_global_homography(
            &obs,
            &mut grow,
            &ExtensionParams::default(),
            &ctx,
            &mut sink,
        );
        assert_eq!(stats.n_attached, 0);
        assert!(!stats.h_trusted);
    }

    #[test]
    fn extends_clean_perspective_grid_f32() {
        assert_extends_clean_perspective_grid::<f32>();
    }
    #[test]
    fn extends_clean_perspective_grid_f64() {
        assert_extends_clean_perspective_grid::<f64>();
    }
    #[test]
    fn no_op_when_too_few_labels_f32() {
        assert_no_op_when_too_few_labels::<f32>();
    }
    #[test]
    fn no_op_when_too_few_labels_f64() {
        assert_no_op_when_too_few_labels::<f64>();
    }
}
