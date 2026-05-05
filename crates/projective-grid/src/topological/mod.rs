//! Topological grid construction (Shu/Brunton/Fiala 2009, axis-driven variant).
//!
//! Builds a labelled `(i, j)` grid from a cloud of 2D corners by:
//!
//! 1. Delaunay-triangulating the points.
//! 2. Classifying each Delaunay edge as a *grid edge*, *diagonal*, or
//!    *spurious* using the per-corner ChESS axes — no image color sampling
//!    is required.
//! 3. Merging triangle pairs whose shared edge is a diagonal into quads
//!    (one quad per chessboard cell).
//! 4. Pruning corners with quad-degree > 4 (illegal), then quads with two
//!    illegal corners (paper §4).
//! 5. Pruning quads whose opposing edges differ in length by more than
//!    `edge_ratio_max` (paper §4 geometric test).
//! 6. Flood-filling integer `(i, j)` labels through the quad mesh
//!    (paper §5 topological walking).
//! 7. Rebasing labels per component so the bounding box starts at `(0, 0)`.
//!
//! The pipeline produces one [`TopologicalComponent`] per connected
//! component of the surviving quad mesh. Component merging is handled by
//! [`crate::component_merge`] so the same logic is reusable from the
//! chessboard-v2 seed-and-grow pipeline.
//!
//! Why an axis-driven test rather than the paper's color test:
//!
//! - The crate stays standalone (no image dependency, see workspace
//!   architecture rules).
//! - At low view angles the global cell-size mode estimate becomes
//!   ambiguous, but ChESS axes (which encode local image-gradient
//!   direction at each corner) remain reliable.
//! - The test naturally rejects background corners whose axes do not
//!   align with the dominant grid directions.
//!
//! Pre-conditions on inputs:
//!
//! - `positions[k]` and `axes[k]` describe the same corner for every `k`.
//! - `axes[k][0]` and `axes[k][1]` follow the workspace convention:
//!   angles in radians, the two axes orthogonal up to ChESS noise, and
//!   `sigma = π` indicates "no information" (such corners are skipped).

use std::collections::HashMap;

use nalgebra::Point2;
use serde::{Deserialize, Serialize};

mod classify;
mod delaunay;
mod quads;
mod topo_filter;
mod trace;
mod walk;

#[cfg(test)]
mod tests;

pub use classify::EdgeKind;
pub use trace::{
    build_grid_topological_trace, TopologicalComponentTrace, TopologicalCornerTrace,
    TopologicalEdgeMetricTrace, TopologicalLabelTrace, TopologicalQuadTrace, TopologicalTrace,
    TopologicalTriangleTrace,
};

/// One local grid-axis direction at a corner with its 1σ angular uncertainty.
///
/// Mirror of `calib_targets_core::AxisEstimate` so that `projective-grid`
/// remains free of image / detector dependencies. The chessboard crate
/// converts `Corner.axes` into this type before calling.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AxisHint {
    /// Axis angle in radians.
    pub angle: f32,
    /// 1σ angular uncertainty in radians. `sigma >= max_sigma` is treated
    /// as "no information" and the corner is skipped.
    pub sigma: f32,
}

impl Default for AxisHint {
    fn default() -> Self {
        // No-information default — matches `AxisEstimate::default()`.
        Self {
            angle: 0.0,
            sigma: std::f32::consts::PI,
        }
    }
}

impl AxisHint {
    /// Construct an `AxisHint` from a bare angle, with no uncertainty
    /// information (`sigma = 0.0`).  Useful for callers that only have
    /// an angle (e.g. [`SeedQuadValidator::axes`] impls that do not track
    /// per-corner uncertainty).
    ///
    /// [`SeedQuadValidator::axes`]: crate::square::seed_finder::SeedQuadValidator::axes
    pub fn from_angle(angle: f32) -> Self {
        Self { angle, sigma: 0.0 }
    }
}

/// Two global grid-axis directions, in `[0, π)` with `theta0 < theta1`.
///
/// Mirrors `calib_targets_chessboard::ClusterCenters` so the chessboard
/// detector's `cluster_axes` output can flow into the topological
/// pre-Delaunay gate without `projective-grid` taking a chessboard-side
/// dependency. The two directions are interpreted modulo π (axes are
/// undirected). Construct via [`AxisClusterCenters::new`] which orders
/// the inputs and wraps them into `[0, π)`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AxisClusterCenters {
    pub theta0: f32,
    pub theta1: f32,
}

impl AxisClusterCenters {
    /// Wrap both inputs into `[0, π)` and order so `theta0 < theta1`.
    pub fn new(a: f32, b: f32) -> Self {
        let (mut t0, mut t1) = (
            crate::circular_stats::wrap_pi(a),
            crate::circular_stats::wrap_pi(b),
        );
        if t0 > t1 {
            std::mem::swap(&mut t0, &mut t1);
        }
        Self {
            theta0: t0,
            theta1: t1,
        }
    }
}

/// Tuning knobs for [`build_grid_topological`].
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TopologicalParams {
    /// Maximum angular distance, in radians, between an edge's direction
    /// and a corner's axis for the edge to be classified as a *grid edge*
    /// at that corner. Default: `15° = 0.262` — paired with the
    /// pre-Delaunay [`Self::axis_cluster_centers`] gate. The 22°/18°
    /// pre-cluster-gate values were a workaround for the missing global
    /// axis filter; with the gate active they're a precision risk.
    pub axis_align_tol_rad: f32,
    /// Maximum angular distance, in radians, between an edge's direction
    /// and `axis ± π/4` for the edge to be classified as a *diagonal* at
    /// that corner. Default: `15° = 0.262`. With
    /// [`Self::axis_align_tol_rad`] also at 15°, the sum is below π/4
    /// so a single edge can never satisfy both predicates — the
    /// classification is unambiguous by construction.
    pub diagonal_angle_tol_rad: f32,
    /// Maximum 1σ axis uncertainty (radians) for a corner to participate
    /// in classification. Corners whose both axes have `sigma >=
    /// max_axis_sigma_rad` are excluded. Default: `0.6` (≈ 34°).
    pub max_axis_sigma_rad: f32,
    /// Reject quads whose opposing edges differ in length by more than
    /// this factor (matches the paper's parallelogram test). Default: `10.0`.
    pub edge_ratio_max: f32,
    /// Discard connected quad-mesh components below this size. Default: `1`
    /// (keep all). Set higher to reject isolated noise quads.
    pub min_quads_per_component: usize,
    /// Optional global grid-direction centers. When `Some`, every corner
    /// must have at least one axis within
    /// [`Self::cluster_axis_tol_rad`] of either center to enter Delaunay
    /// (a precision filter borrowed from
    /// `calib_targets_chessboard::cluster_axes`). When `None`, the gate
    /// is skipped — preserving the legacy behaviour of this crate as a
    /// standalone primitive. The chessboard detector's topological
    /// dispatch path always supplies this from its own clustering.
    pub axis_cluster_centers: Option<AxisClusterCenters>,
    /// Per-axis admission tolerance against [`Self::axis_cluster_centers`],
    /// in radians. Only consulted when `axis_cluster_centers.is_some()`.
    /// Default: `16° = 0.279` — wider than the chessboard-v2
    /// `cluster_tol_deg` default of `12°` to compensate for the
    /// topological pipeline's lack of sigma-bonus / booster recovery for
    /// dropped corners. The right floor empirically sits near 16°: at
    /// 12° we lose a real corner on `02-topo-grid/GeminiChess2.png`
    /// and 4 frames on `130x130_puzzle`; tightening below this should
    /// be paired with a sigma-aware admission rule (Phase D).
    pub cluster_axis_tol_rad: f32,
    /// Lower bound on a quad's perimeter edge length, expressed as a
    /// fraction of the per-component median quad edge length. Quads
    /// with any edge shorter than `quad_edge_min_rel * component_median`
    /// are rejected as "below local cell scale". Default: `0.0`
    /// (disabled). Empirically the lower bound rejects too many
    /// legitimate small quads on heavily-distorted Gemini boards
    /// without compensating recall on 130x130_puzzle, so we lean on
    /// the upper bound only.
    pub quad_edge_min_rel: f32,
    /// Upper bound on a quad's perimeter edge length, expressed as a
    /// fraction of the per-component median quad edge length. Quads
    /// with any edge longer than `quad_edge_max_rel * component_median`
    /// are rejected as "above local cell scale" (typically a quad
    /// formed across a missing corner). Default: `1.8` — chosen above
    /// the natural perspective stretch on heavily-distorted boards
    /// like `02-topo-grid/GeminiChess2.png` while still excluding the
    /// double-cell hops that fragment 130x130_puzzle.
    /// Set to `f32::INFINITY` to disable.
    pub quad_edge_max_rel: f32,
}

impl Default for TopologicalParams {
    fn default() -> Self {
        Self {
            axis_align_tol_rad: 15.0_f32.to_radians(),
            diagonal_angle_tol_rad: 15.0_f32.to_radians(),
            max_axis_sigma_rad: 0.6,
            edge_ratio_max: 10.0,
            min_quads_per_component: 1,
            axis_cluster_centers: None,
            cluster_axis_tol_rad: 16.0_f32.to_radians(),
            quad_edge_min_rel: 0.0,
            quad_edge_max_rel: 1.8,
        }
    }
}

/// Per-component output of the topological pipeline.
#[derive(Clone, Debug, Default)]
pub struct TopologicalComponent {
    /// `(i, j) → corner_idx` mapping. Indices reference the original
    /// `positions` slice. The bounding box of the labelled set always
    /// starts at `(0, 0)` (workspace invariant).
    pub labelled: HashMap<(i32, i32), usize>,
}

/// Diagnostic counters from one [`build_grid_topological`] run.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TopologicalStats {
    /// Corners passed in.
    pub corners_in: usize,
    /// Corners that survived the axis-validity pre-filter.
    pub corners_used: usize,
    /// Triangles produced by Delaunay triangulation.
    pub triangles: usize,
    /// Half-edges classified as `Grid` (counted twice, once per direction).
    pub grid_edges: usize,
    /// Half-edges classified as `Diagonal`.
    pub diagonal_edges: usize,
    /// Half-edges classified as `Spurious`.
    pub spurious_edges: usize,
    /// Triangles with exactly one Diagonal edge and two Grid edges
    /// (i.e. eligible to merge into a quad if their buddy agrees).
    pub triangles_mergeable: usize,
    /// Triangles with all three edges classified as Grid (suggests
    /// the triangle spans more than one cell — the paper's failure
    /// mode at very low view angles).
    pub triangles_all_grid: usize,
    /// Triangles with multiple Diagonal edges (ambiguous).
    pub triangles_multi_diag: usize,
    /// Triangles with at least one Spurious edge.
    pub triangles_has_spurious: usize,
    /// Triangle pairs merged into quads.
    pub quads_merged: usize,
    /// Quads surviving topological + geometric filtering.
    pub quads_kept: usize,
    /// Connected quad-mesh components after walking.
    pub components: usize,
}

/// Top-level result.
#[derive(Clone, Debug, Default)]
pub struct TopologicalGrid {
    pub components: Vec<TopologicalComponent>,
    pub diagnostics: TopologicalStats,
}

/// Per-triangle edge-composition bucket used by diagnostics and tracing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TriangleClass {
    /// Exactly one diagonal edge and two grid edges.
    Mergeable,
    /// All three edges classified as grid.
    AllGrid,
    /// Two or three diagonal edges.
    MultiDiagonal,
    /// At least one spurious edge.
    HasSpurious,
}

/// Errors from [`build_grid_topological`].
#[derive(Clone, Copy, Debug, thiserror::Error)]
pub enum TopologicalError {
    /// The position and axes slices have mismatched length.
    #[error("positions and axes must be the same length (got {positions} and {axes})")]
    LengthMismatch { positions: usize, axes: usize },
    /// Fewer than three usable corners survived the pre-filter, which is
    /// the minimum for Delaunay triangulation.
    #[error("not enough usable corners ({usable}) for Delaunay triangulation")]
    NotEnoughCorners { usable: usize },
}

#[inline]
fn axis_passes_cluster(a: &AxisHint, centers: &AxisClusterCenters, tol: f32) -> bool {
    use crate::circular_stats::{angular_dist_pi, wrap_pi};
    if !a.sigma.is_finite() || a.sigma >= std::f32::consts::PI - f32::EPSILON {
        return false;
    }
    let angle = wrap_pi(a.angle);
    angular_dist_pi(angle, centers.theta0).min(angular_dist_pi(angle, centers.theta1)) < tol
}

#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "debug",
        skip_all,
        fields(num_corners = axes.len()),
    )
)]
fn usable_mask(axes: &[[AxisHint; 2]], params: &TopologicalParams) -> Vec<bool> {
    let centers = params.axis_cluster_centers.as_ref();
    let tol = params.cluster_axis_tol_rad;
    axes.iter()
        .map(|a| {
            let sigma_ok =
                a[0].sigma < params.max_axis_sigma_rad || a[1].sigma < params.max_axis_sigma_rad;
            if !sigma_ok {
                return false;
            }
            match centers {
                None => true,
                Some(c) => axis_passes_cluster(&a[0], c, tol) || axis_passes_cluster(&a[1], c, tol),
            }
        })
        .collect()
}

/// Triangulate only the usable corners and remap triangle vertex indices
/// back into the global `positions` index space.
///
/// The returned [`delaunay::Triangulation`] indexes into the original
/// `positions` slice (not the packed slice), so every downstream stage —
/// classification, quad merging, label flood-fill — keeps using global
/// indices and the rest of the pipeline is oblivious to the pre-filter.
///
/// Returns `(triangulation, packed_to_global)` where `packed_to_global[i]`
/// is the global index of the `i`-th packed corner. The map is returned
/// for callers that may want it (e.g. tracing); the production
/// [`build_grid_topological`] does not need it.
fn triangulate_usable(
    positions: &[Point2<f32>],
    usable: &[bool],
) -> (delaunay::Triangulation, Vec<usize>) {
    let mut packed_to_global: Vec<usize> = Vec::with_capacity(positions.len());
    let mut packed_positions: Vec<Point2<f32>> = Vec::with_capacity(positions.len());
    for (i, (&u, &p)) in usable.iter().zip(positions.iter()).enumerate() {
        if u {
            packed_to_global.push(i);
            packed_positions.push(p);
        }
    }
    let mut triangulation = delaunay::triangulate(&packed_positions);
    for v in triangulation.triangles.iter_mut() {
        *v = packed_to_global[*v];
    }
    (triangulation, packed_to_global)
}

pub(super) fn triangle_class(edge_kinds: &[EdgeKind], t: usize) -> TriangleClass {
    let mut g = 0;
    let mut d = 0;
    let mut sp = 0;
    for k in 0..3 {
        match edge_kinds[3 * t + k] {
            EdgeKind::Grid => g += 1,
            EdgeKind::Diagonal => d += 1,
            EdgeKind::Spurious => sp += 1,
        }
    }
    if sp > 0 {
        TriangleClass::HasSpurious
    } else if d == 1 && g == 2 {
        TriangleClass::Mergeable
    } else if d == 0 && g == 3 {
        TriangleClass::AllGrid
    } else {
        TriangleClass::MultiDiagonal
    }
}

pub(super) fn update_edge_stats(stats: &mut TopologicalStats, edge_kinds: &[EdgeKind]) {
    for &k in edge_kinds {
        match k {
            EdgeKind::Grid => stats.grid_edges += 1,
            EdgeKind::Diagonal => stats.diagonal_edges += 1,
            EdgeKind::Spurious => stats.spurious_edges += 1,
        }
    }
}

pub(super) fn update_triangle_stats(stats: &mut TopologicalStats, edge_kinds: &[EdgeKind]) {
    for t in 0..stats.triangles {
        match triangle_class(edge_kinds, t) {
            TriangleClass::Mergeable => stats.triangles_mergeable += 1,
            TriangleClass::AllGrid => stats.triangles_all_grid += 1,
            TriangleClass::MultiDiagonal => stats.triangles_multi_diag += 1,
            TriangleClass::HasSpurious => stats.triangles_has_spurious += 1,
        }
    }
}

/// Build labelled grid components from corners + per-corner axes.
///
/// Returns one [`TopologicalComponent`] per connected component of the
/// surviving quad mesh. Use [`crate::component_merge`] to attempt to
/// merge components into a single grid.
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "info",
        skip_all,
        fields(num_corners = positions.len()),
    )
)]
pub fn build_grid_topological(
    positions: &[Point2<f32>],
    axes: &[[AxisHint; 2]],
    params: &TopologicalParams,
) -> Result<TopologicalGrid, TopologicalError> {
    if positions.len() != axes.len() {
        return Err(TopologicalError::LengthMismatch {
            positions: positions.len(),
            axes: axes.len(),
        });
    }
    let mut stats = TopologicalStats {
        corners_in: positions.len(),
        ..Default::default()
    };

    // Pre-filter corners: at least one axis must have a usable sigma.
    // Triangulating over the usable subset (rather than over every input
    // corner) is a strict win — Delaunay is `O(n log n)`, so reducing `n`
    // saves work, and excluding noise-only corners up front avoids them
    // starving valid corners of cardinal Delaunay neighbours and producing
    // edges that would only ever classify as `Spurious` downstream.
    // Recovery / extension stages in the chessboard crate use ChESS-strong
    // corners independently and are unaffected by this filter.
    let usable_mask = usable_mask(axes, params);
    stats.corners_used = usable_mask.iter().filter(|&&b| b).count();
    if stats.corners_used < 3 {
        return Err(TopologicalError::NotEnoughCorners {
            usable: stats.corners_used,
        });
    }

    let (triangulation, _packed_to_global) = triangulate_usable(positions, &usable_mask);
    stats.triangles = triangulation.triangles.len() / 3;

    // Classify every half-edge.
    let edge_kinds = classify::classify_all_edges(positions, axes, &triangulation, params);
    update_edge_stats(&mut stats, &edge_kinds);

    // Per-triangle classification breakdown — tells us at a glance
    // whether the merge step is starving on noise (all-spurious),
    // saturated by perspective foreshortening (all-grid spans cells),
    // or jammed by ambiguity (≥ 2 diagonals).
    update_triangle_stats(&mut stats, &edge_kinds);

    // Merge triangle pairs sharing a diagonal whose other edges are grid.
    let raw_quads = quads::merge_triangle_pairs(&triangulation, &edge_kinds, positions);
    stats.quads_merged = raw_quads.len();

    // Topological + geometric filtering.
    let kept_quads = topo_filter::filter_quads(&raw_quads, positions, params);
    stats.quads_kept = kept_quads.len();

    // Flood-fill labels per connected component.
    let components = walk::label_components(&kept_quads, params.min_quads_per_component);
    stats.components = components.len();

    Ok(TopologicalGrid {
        components,
        diagnostics: stats,
    })
}
