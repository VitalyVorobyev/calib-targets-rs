use crate::params::{ChessboardGraphMode, GridGraphParams};
use calib_targets_core::{AxisEstimate, Corner};
use nalgebra::{Point2, Vector2};
use projective_grid::global_step::{estimate_global_cell_size, GlobalStepParams};
use projective_grid::local_step::{
    estimate_local_steps, LocalStep, LocalStepParams, LocalStepPointData,
};
use projective_grid::{GridGraph, NeighborCandidate, NeighborDirection, NeighborValidator};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub use projective_grid::{assign_grid_coordinates, connected_components};

/// Reason a candidate edge was rejected by a chessboard validator.
///
/// Used purely for diagnostics — the public [`NeighborValidator`] contract
/// still returns `Option<(NeighborDirection, f32)>`. When a [`RejectionCounter`]
/// is attached to a validator, each rejection is tallied against the matching
/// reason so downstream sweeps can see *why* each stage dropped candidates.
///
/// Each reason is emitted by at most one validator:
/// * `NoAxisMatchSource`, `NoAxisMatchCandidate`: both the simple and
///   two-axis validators emit these when neither axis at an endpoint
///   aligns with the edge direction (axes-only contract). The simple
///   validator keeps `NotOrthogonal` and `EdgeAxisAngleMismatch` around
///   as backwards-compatible no-op buckets (never incremented).
/// * `AxisLineDisagree`, `OutOfStepWindow`: `ChessboardTwoAxisValidator`.
/// * `MissingCluster`, `SameClusterLegacy`, `LowAlignment`:
///   `ChessboardClusterValidator`.
/// * `OutOfDistanceWindow`: emitted by both legacy validators when the
///   candidate distance falls outside the absolute-pixel spacing window.
/// * `ClusterPolarityFlip`, `LocalHomographyResidual`: Phase B stubs reserved
///   for future validator / pruning stages. Not emitted yet.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeRejectReason {
    /// Simple validator: the two corner orientations are not within
    /// `orientation_tolerance_deg` of being orthogonal.
    NotOrthogonal,
    /// Legacy validators: `|offset|` fell outside `[min_spacing_pix, max_spacing_pix]`.
    OutOfDistanceWindow,
    /// Simple validator: edge direction did not make a 45° angle with either
    /// endpoint's diagonal within tolerance.
    EdgeAxisAngleMismatch,
    /// Cluster validator: one or both endpoints have no orientation-cluster
    /// label (clustering failed or corner fell outside both clusters).
    MissingCluster,
    /// Cluster validator: both endpoints share the same cluster — they are on
    /// the same diagonal family, not a valid 4-connected neighbor.
    SameClusterLegacy,
    /// Cluster validator: edge direction has low dot-product alignment with
    /// either of the canonical grid axes derived from the diagonals.
    LowAlignment,
    /// Two-axis validator: no axis at the source endpoint lies within the
    /// angular tolerance of the edge direction.
    NoAxisMatchSource,
    /// Two-axis validator: same at the candidate endpoint.
    NoAxisMatchCandidate,
    /// Two-axis validator: the matched axes at the two endpoints do not agree
    /// on the same underlying axis line.
    AxisLineDisagree,
    /// Two-axis validator: `|offset|` does not lie in `[min_step_rel, max_step_rel]`
    /// times the local step estimate at either endpoint.
    OutOfStepWindow,
    /// Phase B stub: endpoints share the same orientation-cluster label,
    /// violating the expected polarity flip between adjacent chessboard
    /// corners. Not emitted yet.
    ClusterPolarityFlip,
    /// Phase B stub: corner position disagrees with local-homography
    /// prediction by more than the configured threshold. Not emitted yet.
    LocalHomographyResidual,
}

impl EdgeRejectReason {
    /// Stable lowercase-snake-case tag, usable as a map key or CSV column.
    pub fn as_str(self) -> &'static str {
        match self {
            EdgeRejectReason::NotOrthogonal => "not_orthogonal",
            EdgeRejectReason::OutOfDistanceWindow => "out_of_distance_window",
            EdgeRejectReason::EdgeAxisAngleMismatch => "edge_axis_angle_mismatch",
            EdgeRejectReason::MissingCluster => "missing_cluster",
            EdgeRejectReason::SameClusterLegacy => "same_cluster_legacy",
            EdgeRejectReason::LowAlignment => "low_alignment",
            EdgeRejectReason::NoAxisMatchSource => "no_axis_match_source",
            EdgeRejectReason::NoAxisMatchCandidate => "no_axis_match_candidate",
            EdgeRejectReason::AxisLineDisagree => "axis_line_disagree",
            EdgeRejectReason::OutOfStepWindow => "out_of_step_window",
            EdgeRejectReason::ClusterPolarityFlip => "cluster_polarity_flip",
            EdgeRejectReason::LocalHomographyResidual => "local_homography_residual",
        }
    }
}

/// Per-reason counter for candidate-edge rejections during graph construction.
///
/// Attach to a validator via `with_counter(...)` (or construct a validator
/// with `counter: Some(Rc::new(RefCell::new(RejectionCounter::default())))`).
/// Each rejection increments the matching [`EdgeRejectReason`]. Calling
/// [`RejectionCounter::total`] returns the sum across reasons.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RejectionCounter {
    /// One entry per reason seen. Missing keys mean zero rejections for that
    /// reason.
    pub counts: HashMap<EdgeRejectReason, u64>,
}

impl RejectionCounter {
    /// Increment the count for `reason`.
    #[inline]
    pub fn record(&mut self, reason: EdgeRejectReason) {
        *self.counts.entry(reason).or_insert(0) += 1;
    }

    /// Total rejections summed across all reasons.
    pub fn total(&self) -> u64 {
        self.counts.values().copied().sum()
    }

    /// Rejections for a specific reason (0 if never seen).
    pub fn count_of(&self, reason: EdgeRejectReason) -> u64 {
        self.counts.get(&reason).copied().unwrap_or(0)
    }
}

/// Interior-mutable handle to a shared counter, passed from the detector
/// through the graph builder into each validator without requiring `&mut` on
/// the validator (the [`NeighborValidator`] trait takes `&self`).
pub type RejectionCounterCell = Rc<RefCell<RejectionCounter>>;

#[inline]
fn record_reason(sink: &Option<RejectionCounterCell>, reason: EdgeRejectReason) {
    if let Some(cell) = sink {
        cell.borrow_mut().record(reason);
    }
}

/// Small helper: angle between an undirected axis `axis_angle`
/// (defined modulo π) and a directed vector angle `vec_angle`.
/// Returns a value in `[0, π/2]`.
fn axis_vec_diff(axis_angle: f32, vec_angle: f32) -> f32 {
    let two_pi = 2.0 * std::f32::consts::PI;

    // Difference in [-π, π).
    let mut diff = (vec_angle - axis_angle).rem_euclid(two_pi);
    if diff >= std::f32::consts::PI {
        diff -= two_pi;
    }
    let diff_abs = diff.abs();

    // Axis is undirected: θ and θ+π describe the same line.
    diff_abs.min(std::f32::consts::PI - diff_abs)
}

/// Convert angle (radians) to unit 2D vector.
fn angle_to_unit(theta: f32) -> nalgebra::Vector2<f32> {
    nalgebra::Vector2::new(theta.cos(), theta.sin())
}

fn direction_quadrant(vec_to_neighbor: &Vector2<f32>) -> NeighborDirection {
    if vec_to_neighbor.x.abs() > vec_to_neighbor.y.abs() {
        if vec_to_neighbor.x >= 0.0 {
            NeighborDirection::Right
        } else {
            NeighborDirection::Left
        }
    } else if vec_to_neighbor.y >= 0.0 {
        NeighborDirection::Down
    } else {
        NeighborDirection::Up
    }
}

/// Per-corner data needed for chessboard neighbor validation.
///
/// Carries the two-axis descriptor, the optional orientation-cluster label
/// (used by [`ChessboardClusterValidator`]), and the local-step estimate
/// consumed by [`ChessboardTwoAxisValidator`].
pub struct ChessboardPointData {
    pub orientation_cluster: Option<usize>,
    pub axes: [AxisEstimate; 2],
    pub local_step: LocalStep<f32>,
}

impl ChessboardPointData {
    pub fn from_corners(corners: &[Corner]) -> Vec<Self> {
        Self::from_corners_with_steps(corners, &vec![LocalStep::<f32>::default(); corners.len()])
    }

    pub fn from_corners_with_steps(corners: &[Corner], steps: &[LocalStep<f32>]) -> Vec<Self> {
        debug_assert_eq!(corners.len(), steps.len());
        corners
            .iter()
            .zip(steps.iter())
            .map(|(c, s)| Self {
                orientation_cluster: c.orientation_cluster,
                axes: c.axes,
                local_step: *s,
            })
            .collect()
    }
}

/// Estimate per-corner local grid step by delegating to
/// [`projective_grid::local_step::estimate_local_steps`]. Uses the corner's
/// two-axis descriptor (`axes`) directly; when a corner has not been populated
/// from a 0.6 `CornerDescriptor` (sigma == π on both axes), its local step is
/// still computed from position-only KD-tree neighbors (because sector binning
/// tolerates any axis as a best-guess).
pub fn estimate_corner_local_steps(corners: &[Corner]) -> Vec<LocalStep<f32>> {
    let pts: Vec<LocalStepPointData<f32>> = corners
        .iter()
        .map(|c| LocalStepPointData {
            position: Point2::new(c.position.x, c.position.y),
            axis_u: c.axes[0].angle,
            axis_v: c.axes[1].angle,
        })
        .collect();
    estimate_local_steps(&pts, &LocalStepParams::<f32>::default())
}

/// Validator that matches the edge direction against each endpoint's
/// two-axis descriptor. Under the axes-only contract:
///
/// 1. `axes[0]` and `axes[1]` at each corner are orthogonal by construction
///    (the upstream ChESS detector guarantees it), so no separate
///    orthogonality check between endpoints is needed.
/// 2. The edge direction `B − A` must align (within
///    `orientation_tolerance_deg`) with one of `A.axes[k]` AND one of
///    `B.axes[k']`. Slot identity across endpoints is NOT required — a
///    parity flip (`axes[0]` ↔ `axes[1]` between adjacent corners) is
///    expected on a chessboard.
/// 3. The candidate distance must lie in `[min_spacing_pix, max_spacing_pix]`.
pub struct ChessboardSimpleValidator {
    pub min_spacing_pix: f32,
    pub max_spacing_pix: f32,
    pub orientation_tolerance_deg: f32,
    /// Optional shared counter. When `Some`, each reject increments the
    /// matching [`EdgeRejectReason`].
    pub counter: Option<RejectionCounterCell>,
}

impl NeighborValidator for ChessboardSimpleValidator {
    type PointData = ChessboardPointData;

    fn validate(
        &self,
        _source_index: usize,
        source_data: &Self::PointData,
        candidate: &NeighborCandidate,
        candidate_data: &Self::PointData,
    ) -> Option<(NeighborDirection, f32)> {
        let tol = self.orientation_tolerance_deg.to_radians();

        // 1. Distance window.
        if candidate.distance < self.min_spacing_pix || candidate.distance > self.max_spacing_pix {
            record_reason(&self.counter, EdgeRejectReason::OutOfDistanceWindow);
            return None;
        }

        // 2. Axis match at both endpoints. The edge direction must line
        //    up (line-folded) with some axis at the source and some axis
        //    at the candidate. Slot identity is NOT required across
        //    endpoints — chessboard canonicalisation can put the same
        //    underlying axis in slot 0 at one corner and slot 1 at the
        //    next.
        let edge_angle = candidate.offset.y.atan2(candidate.offset.x);
        let Some((_src_idx, diff_src, _)) = pick_best_axis(&source_data.axes, edge_angle, tol)
        else {
            record_reason(&self.counter, EdgeRejectReason::NoAxisMatchSource);
            return None;
        };
        let Some((_cand_idx, diff_cand, _)) = pick_best_axis(&candidate_data.axes, edge_angle, tol)
        else {
            record_reason(&self.counter, EdgeRejectReason::NoAxisMatchCandidate);
            return None;
        };

        // 3. Classify neighbor direction in image space.
        let direction = direction_quadrant(&candidate.offset);

        // Score: lower is better. Sum of axis-alignment residuals, which
        // collapses to zero for a perfectly grid-aligned edge.
        let score = diff_src + diff_cand;
        Some((direction, score))
    }
}

/// Validator that uses orientation-cluster labels plus two canonical grid
/// axes (the cluster centers).
///
/// Under the axes-only contract, the cluster centers emitted by
/// `cluster_orientations` (in `calib-targets-core`) are the two grid
/// axes themselves — not grid diagonals. The `grid_diagonals` field is
/// therefore a historical name preserved for downstream callers;
/// geometrically it carries the two axis angles `[θ, θ+π/2 (mod π)]`.
pub struct ChessboardClusterValidator {
    pub min_spacing_pix: f32,
    pub max_spacing_pix: f32,
    pub orientation_tolerance_deg: f32,
    /// Two cluster-center angles in [0, π). Despite the legacy name,
    /// these are the grid AXES (not diagonals) under the axes-only
    /// contract.
    pub grid_diagonals: [f32; 2],
    /// Optional shared counter. When `Some`, each reject increments the
    /// matching [`EdgeRejectReason`].
    pub counter: Option<RejectionCounterCell>,
}

impl NeighborValidator for ChessboardClusterValidator {
    type PointData = ChessboardPointData;

    fn validate(
        &self,
        _source_index: usize,
        source_data: &Self::PointData,
        candidate: &NeighborCandidate,
        candidate_data: &Self::PointData,
    ) -> Option<(NeighborDirection, f32)> {
        // 0. Need valid orientation clusters for both corners. Adjacent
        //    chessboard corners carry opposite labels under the
        //    canonical-vs-swapped axis pairing — so an edge between same-
        //    label corners is not a direct 4-neighbor.
        let (Some(ci), Some(cj)) = (
            source_data.orientation_cluster,
            candidate_data.orientation_cluster,
        ) else {
            record_reason(&self.counter, EdgeRejectReason::MissingCluster);
            return None;
        };
        if ci == cj {
            record_reason(&self.counter, EdgeRejectReason::SameClusterLegacy);
            return None;
        }

        // 1. Distance window.
        if candidate.distance < self.min_spacing_pix || candidate.distance > self.max_spacing_pix {
            record_reason(&self.counter, EdgeRejectReason::OutOfDistanceWindow);
            return None;
        }

        // 2. Edge direction vs. the two grid axes (cluster centers).
        //
        // Canonicalise so that direction classification is INDEPENDENT
        // of which cluster slot (0 or 1) the clusterer happened to
        // produce first:
        // - Pick as `axis_u` (the horizontal / Left-Right selector)
        //   whichever of the two centres is closer to image-x (largest
        //   |cos|); the other becomes `axis_v` (Up/Down).
        // - Flip `axis_u` to have non-negative x so `dot_u > 0` means
        //   the edge points RIGHT.
        // - Flip `axis_v` so the pair forms a right-handed frame
        //   (u × v ≥ 0 in y-down coords), giving `dot_v > 0` → DOWN.
        //
        // Axes are undirected lines (period π), so sign flips do not
        // change the absolute dot products used for alignment scoring.
        let e = candidate.offset / candidate.distance;
        let c0 = angle_to_unit(self.grid_diagonals[0]);
        let c1 = angle_to_unit(self.grid_diagonals[1]);
        let (mut axis_u, mut axis_v) = if c0.x.abs() >= c1.x.abs() {
            (c0, c1)
        } else {
            (c1, c0)
        };
        if axis_u.x < 0.0 {
            axis_u = -axis_u;
        }
        // Pick the `axis_v` sign so {axis_u, axis_v} forms a right-handed
        // frame (cross product u × v = u.x * v.y − u.y * v.x ≥ 0). With
        // y-down image coords that means positive dot_v selects DOWN.
        if axis_u.x * axis_v.y - axis_u.y * axis_v.x < 0.0 {
            axis_v = -axis_v;
        }

        let dot_u = axis_u.dot(&e);
        let dot_v = axis_v.dot(&e);
        let best_alignment = dot_u.abs().max(dot_v.abs());

        if best_alignment < self.orientation_tolerance_deg.to_radians().cos() {
            record_reason(&self.counter, EdgeRejectReason::LowAlignment);
            return None;
        }

        // 3. Direction classification.
        let direction = if dot_u.abs() >= dot_v.abs() {
            if dot_u >= 0.0 {
                NeighborDirection::Right
            } else {
                NeighborDirection::Left
            }
        } else if dot_v >= 0.0 {
            NeighborDirection::Down
        } else {
            NeighborDirection::Up
        };

        let score = 1.0 - best_alignment;
        Some((direction, score))
    }
}

/// Step-consistent two-axis chessboard neighbor validator.
///
/// Edge `(A, B)` accepted iff all of:
/// 1. Orientation-axis match at A: `B − A` aligns (within `angular_tol_rad`)
///    with one of `±A.axes[0]` or `±A.axes[1]`. Tolerance scales up to 2× by
///    the matched axis's sigma.
/// 2. Reciprocal axis match at B: `A − B` aligns similarly with B's matched
///    axis, and `|A.axes[u].angle − B.axes[u].angle|` is within the same
///    scaled tolerance (the two endpoints agree on which axis they share).
/// 3. Step match: `|B − A|` lies in `[min_step_rel, max_step_rel] × step_u_or_v`
///    at both endpoints (when local-step confidence is non-zero; otherwise
///    falls back to `step_fallback_pix`).
pub struct ChessboardTwoAxisValidator {
    pub min_step_rel: f32,
    pub max_step_rel: f32,
    pub angular_tol_rad: f32,
    pub step_fallback_pix: f32,
    /// Optional shared counter. When `Some`, each reject increments the
    /// matching [`EdgeRejectReason`].
    pub counter: Option<RejectionCounterCell>,
}

impl ChessboardTwoAxisValidator {
    fn effective_step(step: &LocalStep<f32>, axis: usize, fallback: f32) -> f32 {
        let s = match axis {
            0 => step.step_u,
            _ => step.step_v,
        };
        if step.confidence > 0.0 && s > 0.0 {
            s
        } else {
            fallback
        }
    }
}

impl NeighborValidator for ChessboardTwoAxisValidator {
    type PointData = ChessboardPointData;

    fn validate(
        &self,
        _source_index: usize,
        source_data: &Self::PointData,
        candidate: &NeighborCandidate,
        candidate_data: &Self::PointData,
    ) -> Option<(NeighborDirection, f32)> {
        let edge_angle = candidate.offset.y.atan2(candidate.offset.x);

        // Pick whichever of A's two axes best matches the edge direction.
        let Some((src_idx, diff_src, tol_src)) =
            pick_best_axis(&source_data.axes, edge_angle, self.angular_tol_rad)
        else {
            record_reason(&self.counter, EdgeRejectReason::NoAxisMatchSource);
            return None;
        };
        // Same at the candidate: match against EITHER axis — chessboard
        // canonicalization can swap which angle gets the `[0]` vs `[1]` slot
        // on adjacent corners (polarity flip), so slot-index equality is not
        // a reliable cross-endpoint invariant. The *line* is.
        let Some((cand_idx, diff_cand, tol_cand)) =
            pick_best_axis(&candidate_data.axes, edge_angle, self.angular_tol_rad)
        else {
            record_reason(&self.counter, EdgeRejectReason::NoAxisMatchCandidate);
            return None;
        };

        // The two endpoints must agree on the underlying axis line (mod π).
        let axis_agreement_tol = tol_src.max(tol_cand) * 2.0;
        let axis_agreement = axis_vec_diff(
            source_data.axes[src_idx].angle,
            candidate_data.axes[cand_idx].angle,
        );
        if axis_agreement > axis_agreement_tol {
            record_reason(&self.counter, EdgeRejectReason::AxisLineDisagree);
            return None;
        }

        // Step consistency: compare |B − A| to the matched axis's local step
        // at both endpoints.
        let step_src =
            Self::effective_step(&source_data.local_step, src_idx, self.step_fallback_pix);
        let step_cand =
            Self::effective_step(&candidate_data.local_step, cand_idx, self.step_fallback_pix);
        let min_src = self.min_step_rel * step_src;
        let max_src = self.max_step_rel * step_src;
        let min_cand = self.min_step_rel * step_cand;
        let max_cand = self.max_step_rel * step_cand;
        if candidate.distance < min_src
            || candidate.distance > max_src
            || candidate.distance < min_cand
            || candidate.distance > max_cand
        {
            record_reason(&self.counter, EdgeRejectReason::OutOfStepWindow);
            return None;
        }

        // Direction classification uses only the geometry (edge offset in
        // image space) so that the LRUD labelling stays consistent across
        // corners regardless of which slot `axes[0]` happens to occupy at
        // each endpoint.
        let direction = direction_quadrant(&candidate.offset);

        // Score: lower is better. Combine angular residuals, relative step
        // error, and joint axis sigma. Weights are rough but predictable.
        let step_error = ((candidate.distance - step_src).abs() / step_src.max(1e-3)).min(1.0);
        let sigma_sum = source_data.axes[src_idx].sigma + candidate_data.axes[cand_idx].sigma;
        let score = diff_src + diff_cand + step_error + 0.1 * sigma_sum;

        Some((direction, score))
    }
}

/// Pick whichever axis best matches `edge_angle` within tolerance. Returns
/// `(axis_index, diff, tol)` where `diff` is the angular residual (line-
/// folded) and `tol` is the adjusted tolerance incorporating the axis sigma.
fn pick_best_axis(
    axes: &[AxisEstimate; 2],
    edge_angle: f32,
    base_tol: f32,
) -> Option<(usize, f32, f32)> {
    let mut best: Option<(usize, f32, f32)> = None;
    for (idx, axis) in axes.iter().enumerate() {
        let diff = axis_vec_diff(axis.angle, edge_angle);
        let tol = (base_tol + axis.sigma).min(2.0 * base_tol);
        if diff <= tol && best.map(|b| diff < b.1).unwrap_or(true) {
            best = Some((idx, diff, tol));
        }
    }
    best
}

/// Build a chessboard grid graph from corners.
///
/// Wraps [`build_chessboard_grid_graph_instrumented`] without a counter sink.
pub fn build_chessboard_grid_graph(
    corners: &[Corner],
    params: &GridGraphParams,
    grid_diagonals: Option<[f32; 2]>,
) -> GridGraph {
    build_chessboard_grid_graph_instrumented(corners, params, grid_diagonals, None)
}

/// Build a chessboard grid graph with an optional per-reason rejection counter.
///
/// When `counter_sink` is `Some`, every candidate edge a validator rejects
/// increments the matching [`EdgeRejectReason`] bucket. Callers should clear
/// or replace the sink between builds.
pub fn build_chessboard_grid_graph_instrumented(
    corners: &[Corner],
    params: &GridGraphParams,
    grid_diagonals: Option<[f32; 2]>,
    counter_sink: Option<&mut RejectionCounter>,
) -> GridGraph {
    let cell: Option<RejectionCounterCell> = counter_sink
        .as_ref()
        .map(|_| Rc::new(RefCell::new(RejectionCounter::default())));

    let positions: Vec<_> = corners.iter().map(|c| c.position).collect();

    let graph = match params.mode {
        ChessboardGraphMode::Legacy => {
            let point_data = ChessboardPointData::from_corners(corners);
            let graph_params = projective_grid::GridGraphParams {
                k_neighbors: params.k_neighbors,
                max_distance: params.max_spacing_pix,
            };

            if let Some(diags) = grid_diagonals {
                let validator = ChessboardClusterValidator {
                    min_spacing_pix: params.min_spacing_pix,
                    max_spacing_pix: params.max_spacing_pix,
                    orientation_tolerance_deg: params.orientation_tolerance_deg,
                    grid_diagonals: diags,
                    counter: cell.clone(),
                };
                GridGraph::build(&positions, &point_data, &validator, &graph_params)
            } else {
                let validator = ChessboardSimpleValidator {
                    min_spacing_pix: params.min_spacing_pix,
                    max_spacing_pix: params.max_spacing_pix,
                    orientation_tolerance_deg: params.orientation_tolerance_deg,
                    counter: cell.clone(),
                };
                GridGraph::build(&positions, &point_data, &validator, &graph_params)
            }
        }
        ChessboardGraphMode::TwoAxis => {
            // Auto cell-size estimation: derive the grid spacing from the
            // corner cloud itself. Falls back to `params.step_fallback_pix`
            // only when the estimator fails (too few corners, no signal).
            let fallback_step =
                if params.step_fallback_pix.is_finite() && params.step_fallback_pix > 0.0 {
                    params.step_fallback_pix
                } else {
                    50.0
                };
            let global_step =
                estimate_global_cell_size(&positions, &GlobalStepParams::<f32>::default())
                    .map(|e| e.cell_size)
                    .unwrap_or(fallback_step);
            log::debug!("[two_axis graph] estimated global cell size = {global_step:.2}");

            let local_steps = estimate_corner_local_steps(corners);
            let point_data = ChessboardPointData::from_corners_with_steps(corners, &local_steps);
            // KD-tree window intentionally wider than `global_step × max_step_rel`:
            // the global nearest-neighbor mode can underestimate the true grid
            // step when marker-internal corners sit closer to the board than
            // adjacent board corners. The per-edge bounds inside the validator
            // still enforce `±max_step_rel × local_step`.
            let kd_window_factor = (params.max_step_rel * 2.0).max(2.5);
            let graph_params = projective_grid::GridGraphParams {
                k_neighbors: params.k_neighbors.max(20),
                max_distance: global_step * kd_window_factor,
            };
            let validator = ChessboardTwoAxisValidator {
                min_step_rel: params.min_step_rel,
                max_step_rel: params.max_step_rel,
                angular_tol_rad: params.angular_tol_deg.to_radians(),
                step_fallback_pix: global_step,
                counter: cell.clone(),
            };
            GridGraph::build(&positions, &point_data, &validator, &graph_params)
        }
    };

    if let (Some(sink), Some(cell)) = (counter_sink, cell) {
        // Only we and the already-dropped validator held the Rc, so the
        // strong count is 1 after GridGraph::build returns.
        let counted = Rc::try_unwrap(cell)
            .expect("rejection counter cell uniquely held after build")
            .into_inner();
        *sink = counted;
    }

    graph
}

#[cfg(test)]
mod tests {
    use super::*;
    use calib_targets_core::Corner;
    use nalgebra::Point2;
    use projective_grid::NodeNeighbor;
    use std::collections::HashMap;
    use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

    /// Test helper: build a corner whose `axes[0]` is set to `axis0` and
    /// `axes[1]` to the orthogonal direction. Matches the Phase-0 migration
    /// contract that orientation is described by `axes` directly.
    fn make_corner(x: f32, y: f32, axis0: f32) -> Corner {
        Corner {
            position: Point2::new(x, y),
            orientation_cluster: None,
            axes: [
                AxisEstimate {
                    angle: axis0,
                    sigma: 0.05,
                },
                AxisEstimate {
                    angle: axis0 + std::f32::consts::FRAC_PI_2,
                    sigma: 0.05,
                },
            ],
            strength: 1.0,
            ..Corner::default()
        }
    }

    /// Test helper: like `make_corner` but with `axes[0]` and `axes[1]`
    /// slots swapped, to simulate the chessboard parity flip between
    /// adjacent corners.
    fn make_corner_swapped(x: f32, y: f32, axis0: f32) -> Corner {
        Corner {
            position: Point2::new(x, y),
            orientation_cluster: None,
            axes: [
                AxisEstimate {
                    angle: axis0 + std::f32::consts::FRAC_PI_2,
                    sigma: 0.05,
                },
                AxisEstimate {
                    angle: axis0,
                    sigma: 0.05,
                },
            ],
            strength: 1.0,
            ..Corner::default()
        }
    }

    fn neighbor_map(neighbors: &[NodeNeighbor]) -> HashMap<NeighborDirection, &NodeNeighbor> {
        neighbors.iter().map(|n| (n.direction, n)).collect()
    }

    #[test]
    fn finds_axis_neighbors_in_regular_grid() {
        let spacing = 10.0;
        let cols = 3;
        let rows = 3;

        // Axes-only contract: grid axes point along image axes (0, π/2).
        // Alternate corners use the slot-swapped variant to simulate the
        // parity flip between adjacent chessboard corners.
        let mut corners = Vec::new();
        for j in 0..rows {
            for i in 0..cols {
                let x = i as f32 * spacing;
                let y = j as f32 * spacing;
                if (i + j) % 2 == 0 {
                    corners.push(make_corner(x, y, 0.0));
                } else {
                    corners.push(make_corner_swapped(x, y, 0.0));
                }
            }
        }

        let params = GridGraphParams {
            min_spacing_pix: 5.0,
            max_spacing_pix: 15.0,
            ..Default::default()
        };
        let graph = build_chessboard_grid_graph(&corners, &params, None);

        let idx = |i: usize, j: usize| j * cols + i;

        let center = neighbor_map(&graph.neighbors[idx(1, 1)]);
        assert_eq!(4, center.len());
        assert_eq!(idx(0, 1), center[&NeighborDirection::Left].index);
        assert_eq!(idx(2, 1), center[&NeighborDirection::Right].index);
        assert_eq!(idx(1, 0), center[&NeighborDirection::Up].index);
        assert_eq!(idx(1, 2), center[&NeighborDirection::Down].index);
        for dir in [
            NeighborDirection::Left,
            NeighborDirection::Right,
            NeighborDirection::Up,
            NeighborDirection::Down,
        ] {
            assert!((center[&dir].distance - spacing).abs() < 1e-4);
        }

        let top_left = neighbor_map(&graph.neighbors[idx(0, 0)]);
        assert_eq!(2, top_left.len());
        assert!(top_left.contains_key(&NeighborDirection::Right));
        assert!(top_left.contains_key(&NeighborDirection::Down));

        let top_mid = neighbor_map(&graph.neighbors[idx(1, 0)]);
        assert_eq!(3, top_mid.len());
        assert!(top_mid.contains_key(&NeighborDirection::Left));
        assert!(top_mid.contains_key(&NeighborDirection::Right));
        assert!(top_mid.contains_key(&NeighborDirection::Down));
    }

    #[test]
    fn rejects_neighbors_when_orientation_relation_invalid() {
        let spacing = 10.0;
        let corners = vec![
            make_corner(0.0, 0.0, FRAC_PI_4),
            make_corner(spacing, 0.0, FRAC_PI_4),
        ];

        let params = GridGraphParams {
            min_spacing_pix: 5.0,
            max_spacing_pix: 15.0,
            k_neighbors: 2,
            ..Default::default()
        };
        let graph = build_chessboard_grid_graph(&corners, &params, None);

        assert!(graph.neighbors[0].is_empty());
        assert!(graph.neighbors[1].is_empty());
    }

    #[test]
    fn rejects_neighbors_outside_distance_window() {
        let spacing = 30.0;
        let corners = vec![
            make_corner(0.0, 0.0, FRAC_PI_4),
            make_corner(spacing, 0.0, 3.0 * FRAC_PI_4),
        ];

        let params = GridGraphParams {
            min_spacing_pix: 5.0,
            max_spacing_pix: 15.0,
            k_neighbors: 2,
            ..Default::default()
        };
        let graph = build_chessboard_grid_graph(&corners, &params, None);

        assert!(graph.neighbors[0].is_empty());
        assert!(graph.neighbors[1].is_empty());
    }

    fn make_clustered_corner(x: f32, y: f32, axis0: f32, cluster: usize) -> Corner {
        Corner {
            position: Point2::new(x, y),
            orientation_cluster: Some(cluster),
            axes: [
                AxisEstimate {
                    angle: axis0,
                    sigma: 0.05,
                },
                AxisEstimate {
                    angle: axis0 + std::f32::consts::FRAC_PI_2,
                    sigma: 0.05,
                },
            ],
            strength: 1.0,
            ..Corner::default()
        }
    }

    #[test]
    fn rotated_grid_forms_single_component() {
        let spacing = 20.0;
        let angle = 40.0f32.to_radians();
        let cols = 4;
        let rows = 4;

        let ax = Vector2::new(angle.cos(), angle.sin());
        let ay = Vector2::new(-angle.sin(), angle.cos());

        // Under the axes-only contract, cluster centers ARE the grid
        // axes (not the diagonals). The helper populates each corner's
        // axes from `axis0` → [axis0, axis0+π/2], so passing `angle`
        // for even-cluster corners and `angle + π/2` for odd-cluster
        // corners produces the chessboard parity flip.
        let grid_axes = [angle, angle + FRAC_PI_2];

        let mut corners = Vec::new();
        for j in 0..rows {
            for i in 0..cols {
                let pos = ax * (i as f32 * spacing) + ay * (j as f32 * spacing);
                let cluster = (i + j) % 2;
                let axis0 = if cluster == 0 {
                    grid_axes[0]
                } else {
                    grid_axes[1]
                };
                corners.push(make_clustered_corner(
                    pos.x + 100.0,
                    pos.y + 100.0,
                    axis0,
                    cluster,
                ));
            }
        }

        let params = GridGraphParams {
            min_spacing_pix: spacing * 0.5,
            max_spacing_pix: spacing * 1.5,
            k_neighbors: 8,
            ..Default::default()
        };
        let graph = build_chessboard_grid_graph(&corners, &params, Some(grid_axes));

        let components = connected_components(&graph);
        assert_eq!(
            1,
            components.len(),
            "Rotated grid should form a single connected component, got {}",
            components.len()
        );
        assert_eq!(cols * rows, components[0].len());

        let coords = assign_grid_coordinates(&graph, &components[0]);
        assert_eq!(cols * rows, coords.len());
        let coord_set: std::collections::HashSet<(i32, i32)> =
            coords.iter().map(|&(_, g)| (g.i, g.j)).collect();
        assert_eq!(
            cols * rows,
            coord_set.len(),
            "All grid coords must be unique"
        );
    }

    #[test]
    fn direction_symmetry_on_rotated_grid() {
        let spacing = 20.0;
        let angle = 55.0f32.to_radians();
        let ax = Vector2::new(angle.cos(), angle.sin());
        let ay = Vector2::new(-angle.sin(), angle.cos());

        let grid_axes = [angle, angle + FRAC_PI_2];

        let mut corners = Vec::new();
        for j in 0..3 {
            for i in 0..3 {
                let pos = ax * (i as f32 * spacing) + ay * (j as f32 * spacing);
                let cluster = (i + j) % 2;
                let axis0 = if cluster == 0 {
                    grid_axes[0]
                } else {
                    grid_axes[1]
                };
                corners.push(make_clustered_corner(
                    pos.x + 50.0,
                    pos.y + 50.0,
                    axis0,
                    cluster,
                ));
            }
        }

        let params = GridGraphParams {
            min_spacing_pix: spacing * 0.5,
            max_spacing_pix: spacing * 1.5,
            k_neighbors: 8,
            ..Default::default()
        };
        let graph = build_chessboard_grid_graph(&corners, &params, Some(grid_axes));

        for (a, neighbors) in graph.neighbors.iter().enumerate() {
            for n in neighbors {
                let b = n.index;
                let b_neighbors = &graph.neighbors[b];
                let back = b_neighbors.iter().find(|nn| nn.index == a);
                assert!(
                    back.is_some(),
                    "Edge {a}->{b} exists but reverse {b}->{a} does not"
                );
                assert_eq!(
                    n.direction.opposite(),
                    back.unwrap().direction,
                    "Edge {a}->{b} is {:?} but {b}->{a} is {:?}, expected {:?}",
                    n.direction,
                    back.unwrap().direction,
                    n.direction.opposite(),
                );
            }
        }
    }

    #[test]
    fn grid_at_45_degrees_forms_single_component() {
        let spacing = 15.0;
        let angle = 45.0f32.to_radians();
        let ax = Vector2::new(angle.cos(), angle.sin());
        let ay = Vector2::new(-angle.sin(), angle.cos());

        let grid_axes = [angle, angle + FRAC_PI_2];

        let mut corners = Vec::new();
        for j in 0..5 {
            for i in 0..5 {
                let pos = ax * (i as f32 * spacing) + ay * (j as f32 * spacing);
                let cluster = (i + j) % 2;
                let axis0 = if cluster == 0 {
                    grid_axes[0]
                } else {
                    grid_axes[1]
                };
                corners.push(make_clustered_corner(
                    pos.x + 80.0,
                    pos.y + 80.0,
                    axis0,
                    cluster,
                ));
            }
        }

        let params = GridGraphParams {
            min_spacing_pix: spacing * 0.5,
            max_spacing_pix: spacing * 1.5,
            k_neighbors: 8,
            ..Default::default()
        };
        let graph = build_chessboard_grid_graph(&corners, &params, Some(grid_axes));

        let components = connected_components(&graph);
        assert_eq!(1, components.len());
        assert_eq!(25, components[0].len());
    }

    #[test]
    fn counter_records_distance_rejections_in_simple_validator() {
        // Two corners closer than `min_spacing_pix`: the KD-tree pre-filter
        // (bounded by `max_spacing_pix`) keeps the pair, so the validator's
        // distance window is what rejects it.
        let corners = vec![
            make_corner(0.0, 0.0, FRAC_PI_4),
            make_corner(3.0, 0.0, 3.0 * FRAC_PI_4),
        ];

        let params = GridGraphParams {
            min_spacing_pix: 5.0,
            max_spacing_pix: 50.0,
            k_neighbors: 2,
            ..Default::default()
        };

        let mut counter = RejectionCounter::default();
        let _graph =
            build_chessboard_grid_graph_instrumented(&corners, &params, None, Some(&mut counter));

        assert!(counter.total() > 0);
        assert!(counter.count_of(EdgeRejectReason::OutOfDistanceWindow) > 0);
    }

    #[test]
    fn counter_records_no_axis_match_in_simple_validator() {
        // Corners whose axes sit at 45° / 135° — neither aligns with a
        // horizontal edge within the default tolerance. Distance is fine,
        // so the axis-match check is what rejects. Simple validator path
        // (no clusters).
        let corners = vec![
            make_corner(0.0, 0.0, FRAC_PI_4),
            make_corner(10.0, 0.0, FRAC_PI_4),
        ];

        let params = GridGraphParams {
            min_spacing_pix: 5.0,
            max_spacing_pix: 15.0,
            k_neighbors: 2,
            ..Default::default()
        };

        let mut counter = RejectionCounter::default();
        let _graph =
            build_chessboard_grid_graph_instrumented(&corners, &params, None, Some(&mut counter));

        let no_match = counter.count_of(EdgeRejectReason::NoAxisMatchSource)
            + counter.count_of(EdgeRejectReason::NoAxisMatchCandidate);
        assert!(
            no_match > 0,
            "simple validator should reject with NoAxisMatch* reasons"
        );
        assert_eq!(counter.count_of(EdgeRejectReason::OutOfDistanceWindow), 0);
    }

    #[test]
    fn counter_is_noop_when_sink_missing() {
        // Same as the not-orthogonal case, but without a counter sink — the
        // graph should still build (same behavior as before this change).
        let corners = vec![
            make_corner(0.0, 0.0, FRAC_PI_4),
            make_corner(10.0, 0.0, FRAC_PI_4),
        ];
        let params = GridGraphParams {
            min_spacing_pix: 5.0,
            max_spacing_pix: 15.0,
            k_neighbors: 2,
            ..Default::default()
        };
        let graph = build_chessboard_grid_graph_instrumented(&corners, &params, None, None);
        assert!(graph.neighbors[0].is_empty());
    }

    #[test]
    fn keeps_best_candidate_per_direction() {
        let spacing = 10.0;
        let worse_spacing = 12.0;

        // Grid axes aligned with image axes (0, π/2). Alternate the axes
        // slot order on neighbors to mimic the chessboard parity flip.
        // The "worse" neighbor tilts its axes by 0.1 rad so its
        // alignment residual with the horizontal edge is larger.
        let corners = vec![
            make_corner(0.0, 0.0, 0.0),                   // center (idx 0)
            make_corner_swapped(spacing, 0.0, 0.0),       // better right (idx 1)
            make_corner_swapped(worse_spacing, 0.0, 0.1), // worse right (idx 2)
            make_corner_swapped(-spacing, 0.0, 0.0),      // left (idx 3)
        ];

        let params = GridGraphParams {
            min_spacing_pix: 5.0,
            max_spacing_pix: 15.0,
            k_neighbors: 4,
            ..Default::default()
        };
        let graph = build_chessboard_grid_graph(&corners, &params, None);

        let map = neighbor_map(&graph.neighbors[0]);
        assert_eq!(2, map.len()); // left + right only
        assert_eq!(1, map[&NeighborDirection::Right].index); // best right chosen
        assert_eq!(3, map[&NeighborDirection::Left].index);
    }
}
