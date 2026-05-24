//! Boundary extension via per-cell local homography reprojection.
//!
//! For each candidate cell just past the labelled bbox, finds the K nearest
//! labelled corners in `(i, j)`-space, fits a 4-point or DLT-style local
//! homography, gates the candidate via the local-fit residual, and runs the
//! same per-cell ladder as [`crate::refine::extend_global`]. The per-cell
//! variant is materially more forgiving under heavy radial distortion or
//! split-region perspective because a poor local fit aborts only that
//! candidate, not the whole pass.

use std::collections::{BinaryHeap, HashMap, HashSet};
use std::time::Instant;

use nalgebra::Point2;

use crate::diagnostics::{DiagnosticSink, Event, GrowRejectReason, Stage};
use crate::feature::Observation;
use crate::float::{lit, Float};
use crate::geometry::homography::estimate_homography;
use crate::grow::attach::{choose_unambiguous, collect_candidates, AmbiguityReason};
use crate::grow::context::SquareGrowContext;
use crate::grow::params::GrowResult;
use crate::lattice::Coord;

use super::extend_global::{
    build_unlabelled_tree, cardinal_edges_ok, enumerate_extension_cells, refresh_bbox,
    reprojection_residuals, ExtensionStats,
};

/// Tunables for [`extend_via_local_homography`].
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct LocalExtensionParams<F: Float> {
    /// Number of nearest labelled corners (by grid Manhattan distance) used
    /// to fit each candidate cell's local homography. Default `12`.
    pub k_nearest: usize,
    /// Minimum support below which a candidate cell is skipped. Must be `≥ 4`
    /// for the local DLT to be solvable. Default `6`.
    pub min_k: usize,
    /// How far past the labelled bbox each row / column may extend per pass.
    /// Default `3` (deeper than the global-H pass; local-H typically needs
    /// more reach to propagate into a missing border).
    pub max_extension_depth: i32,
    /// Per-candidate trust gate: the worst residual on the K support points,
    /// relative to `cell_size`. Default `0.30`.
    pub max_local_residual_rel: F,
    /// Per-candidate search radius around the local-H prediction, relative
    /// to `cell_size`. Default `0.40`.
    pub search_rel: F,
    /// Acceptance ambiguity factor. Default `2.5`.
    pub ambiguity_factor: F,
    /// Maximum number of passes. Default `8`.
    pub max_iters: usize,
}

impl<F: Float> Default for LocalExtensionParams<F> {
    fn default() -> Self {
        Self {
            k_nearest: 12,
            min_k: 6,
            max_extension_depth: 3,
            max_local_residual_rel: lit::<F>(0.30_f32),
            search_rel: lit::<F>(0.40_f32),
            ambiguity_factor: lit::<F>(2.5_f32),
            max_iters: 8,
        }
    }
}

impl<F: Float> LocalExtensionParams<F> {
    /// Construct local-H extension params from the K-NN setting and the
    /// per-candidate trust gate; other knobs take their defaults.
    pub fn new(k_nearest: usize, min_k: usize, max_local_residual_rel: F) -> Self {
        Self {
            k_nearest,
            min_k,
            max_local_residual_rel,
            ..Self::default()
        }
    }

    /// Override the maximum extension depth per iteration.
    #[must_use]
    pub fn with_max_extension_depth(mut self, depth: i32) -> Self {
        self.max_extension_depth = depth;
        self
    }

    /// Override the maximum iteration count.
    #[must_use]
    pub fn with_max_iters(mut self, n: usize) -> Self {
        self.max_iters = n;
        self
    }
}

/// Extend the labelled grid outward using per-candidate local homographies.
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
pub fn extend_via_local_homography<F, C>(
    observations: &[Observation<F>],
    grow: &mut GrowResult<F>,
    params: &LocalExtensionParams<F>,
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

    if grow.labelled.len() < params.min_k.max(4) {
        sink.emit(Event::StageFinished {
            stage: Stage::Refine,
            duration: start.elapsed(),
        });
        return stats;
    }

    let positions: Vec<Point2<F>> = observations.iter().map(|o| o.position).collect();
    let cell_size = grow.cell_size;

    let search_radius = params.search_rel * cell_size;
    let max_residual_px = params.max_local_residual_rel * cell_size;

    let mut all_local_medians: Vec<F> = Vec::new();

    for _ in 0..params.max_iters {
        let (tree, slot_to_idx) = build_unlabelled_tree(observations, grow, ctx);
        let in_use: HashSet<usize> = grow.labelled.values().copied().collect();
        let cells = enumerate_extension_cells(&grow.labelled, params.max_extension_depth);
        let mut attached_this_iter = 0usize;
        for cell in cells {
            if grow.labelled.contains_key(&cell) {
                continue;
            }
            let nearest = nearest_labelled_by_grid(&grow.labelled, cell, params.k_nearest);
            if nearest.len() < params.min_k {
                sink.emit(Event::GrowRejected {
                    coord: cell,
                    reason: GrowRejectReason::NoCandidate,
                });
                stats.n_rejected += 1;
                continue;
            }
            let grid_pts: Vec<Point2<F>> = nearest
                .iter()
                .map(|&(i, j, _)| Point2::new(lit::<F>(i as f32), lit::<F>(j as f32)))
                .collect();
            let img_pts: Vec<Point2<F>> =
                nearest.iter().map(|&(_, _, idx)| positions[idx]).collect();
            let Some(h) = estimate_homography(&grid_pts, &img_pts) else {
                stats.n_rejected += 1;
                continue;
            };
            let (median_res, max_res) = reprojection_residuals(&h, &grid_pts, &img_pts);
            all_local_medians.push(median_res);
            if max_res > max_residual_px {
                sink.emit(Event::GrowRejected {
                    coord: cell,
                    reason: GrowRejectReason::EdgeFailure,
                });
                stats.n_rejected += 1;
                continue;
            }

            let prediction = h.apply(Point2::new(
                lit::<F>(cell.0 as f32),
                lit::<F>(cell.1 as f32),
            ));
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
            stats.h_trusted = true;
        }
        stats.iterations += 1;
        if attached_this_iter == 0 {
            break;
        }
    }

    if !all_local_medians.is_empty() {
        all_local_medians.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mid = all_local_medians[all_local_medians.len() / 2];
        let scaled = mid / cell_size;
        stats.median_residual_rel = Some(nalgebra::convert_unchecked::<F, f64>(scaled));
    }

    refresh_bbox(grow);
    sink.emit(Event::StageFinished {
        stage: Stage::Refine,
        duration: start.elapsed(),
    });
    stats
}

/// K-nearest labelled corners by `(i, j)`-Manhattan distance to `target`.
///
/// Deterministic tiebreak by `(distance, i, j, idx)` ascending; bounded
/// max-heap keeps the cost `O(L log K)` instead of `O(L log L)`.
fn nearest_labelled_by_grid(
    labelled: &HashMap<Coord, usize>,
    target: Coord,
    k: usize,
) -> Vec<(i32, i32, usize)> {
    if k == 0 || labelled.is_empty() {
        return Vec::new();
    }
    let cap = k.min(labelled.len());
    let mut heap: BinaryHeap<KnnEntry> = BinaryHeap::with_capacity(cap);
    for (&(i, j), &idx) in labelled {
        let d = (i - target.0).abs() + (j - target.1).abs();
        let entry = KnnEntry { d, i, j, idx };
        if heap.len() < k {
            heap.push(entry);
        } else if entry < *heap.peek().unwrap() {
            heap.pop();
            heap.push(entry);
        }
    }
    heap.into_sorted_vec()
        .into_iter()
        .map(|e| (e.i, e.j, e.idx))
        .collect()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct KnnEntry {
    d: i32,
    i: i32,
    j: i32,
    idx: usize,
}

impl Ord for KnnEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.d
            .cmp(&other.d)
            .then_with(|| self.i.cmp(&other.i))
            .then_with(|| self.j.cmp(&other.j))
            .then_with(|| self.idx.cmp(&other.idx))
    }
}

impl PartialOrd for KnnEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
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

    fn assert_local_h_extends_clean_grid<F>()
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
        let params: LocalExtensionParams<F> = LocalExtensionParams {
            min_k: 4,
            k_nearest: 8,
            ..LocalExtensionParams::default()
        };
        let stats = extend_via_local_homography(&obs, &mut grow, &params, &ctx, &mut sink);
        assert!(stats.h_trusted);
        assert!(
            grow.labelled.len() > starting,
            "local-H extension should attach at least one corner"
        );
    }

    fn assert_local_h_no_op_when_too_few_labels<F>()
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
        let params: LocalExtensionParams<F> = LocalExtensionParams {
            min_k: 8,
            ..LocalExtensionParams::default()
        };
        let stats = extend_via_local_homography(&obs, &mut grow, &params, &ctx, &mut sink);
        assert_eq!(stats.n_attached, 0);
        assert!(!stats.h_trusted);
    }

    fn assert_knn_returns_k_closest_in_deterministic_order() {
        let mut labelled: HashMap<Coord, usize> = HashMap::new();
        let mut idx = 0;
        for j in 0..5 {
            for i in 0..5 {
                labelled.insert((i, j), idx);
                idx += 1;
            }
        }
        let result = nearest_labelled_by_grid(&labelled, (2, 2), 3);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], (2, 2, 12));
        assert_eq!(result[1], (1, 2, 11));
        assert_eq!(result[2], (2, 1, 7));
    }

    #[test]
    fn local_h_extends_clean_grid_f32() {
        assert_local_h_extends_clean_grid::<f32>();
    }
    #[test]
    fn local_h_extends_clean_grid_f64() {
        assert_local_h_extends_clean_grid::<f64>();
    }
    #[test]
    fn local_h_no_op_when_too_few_labels_f32() {
        assert_local_h_no_op_when_too_few_labels::<f32>();
    }
    #[test]
    fn local_h_no_op_when_too_few_labels_f64() {
        assert_local_h_no_op_when_too_few_labels::<f64>();
    }
    #[test]
    fn knn_returns_k_closest_in_deterministic_order_test() {
        assert_knn_returns_k_closest_in_deterministic_order();
    }
}
