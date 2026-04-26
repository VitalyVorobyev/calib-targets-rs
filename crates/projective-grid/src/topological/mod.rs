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
mod walk;

#[cfg(test)]
mod tests;

pub use classify::EdgeKind;

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

/// Tuning knobs for [`build_grid_topological`].
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TopologicalParams {
    /// Maximum angular distance, in radians, between an edge's direction
    /// and a corner's axis for the edge to be classified as a *grid edge*
    /// at that corner. Default: `15° = 0.262`.
    pub axis_align_tol_rad: f32,
    /// Maximum angular distance, in radians, between an edge's direction
    /// and `axis ± π/4` for the edge to be classified as a *diagonal* at
    /// that corner. Default: `15° = 0.262`.
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
}

impl Default for TopologicalParams {
    fn default() -> Self {
        Self {
            axis_align_tol_rad: 0.262, // 15°
            diagonal_angle_tol_rad: 0.262,
            max_axis_sigma_rad: 0.6,
            edge_ratio_max: 10.0,
            min_quads_per_component: 1,
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
#[derive(Clone, Copy, Debug, Default)]
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

/// Build labelled grid components from corners + per-corner axes.
///
/// Returns one [`TopologicalComponent`] per connected component of the
/// surviving quad mesh. Use [`crate::component_merge`] to attempt to
/// merge components into a single grid.
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
    let usable_mask: Vec<bool> = axes
        .iter()
        .map(|a| a[0].sigma < params.max_axis_sigma_rad || a[1].sigma < params.max_axis_sigma_rad)
        .collect();
    stats.corners_used = usable_mask.iter().filter(|&&b| b).count();
    if stats.corners_used < 3 {
        return Err(TopologicalError::NotEnoughCorners {
            usable: stats.corners_used,
        });
    }

    // Delaunay over ALL positions (cheaper than rebuilding indices).
    // Spurious corners simply produce spurious edges and are dropped later.
    let triangulation = delaunay::triangulate(positions);
    stats.triangles = triangulation.triangles.len() / 3;

    // Classify every half-edge.
    let edge_kinds =
        classify::classify_all_edges(positions, axes, &usable_mask, &triangulation, params);
    for &k in &edge_kinds {
        match k {
            EdgeKind::Grid => stats.grid_edges += 1,
            EdgeKind::Diagonal => stats.diagonal_edges += 1,
            EdgeKind::Spurious => stats.spurious_edges += 1,
        }
    }

    // Per-triangle classification breakdown — tells us at a glance
    // whether the merge step is starving on noise (all-spurious),
    // saturated by perspective foreshortening (all-grid spans cells),
    // or jammed by ambiguity (≥ 2 diagonals).
    for t in 0..stats.triangles {
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
            stats.triangles_has_spurious += 1;
        } else if d == 1 && g == 2 {
            stats.triangles_mergeable += 1;
        } else if d == 0 && g == 3 {
            stats.triangles_all_grid += 1;
        } else if d >= 2 {
            stats.triangles_multi_diag += 1;
        }
    }

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
