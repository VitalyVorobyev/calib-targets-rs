//! Per-corner label assignment and centre refit — chessboard glue over
//! [`projective_grid::cluster`].
//!
//! Once the two grid-direction centres `(Θ₀, Θ₁)` are known (see
//! [`super::cluster_axes`]), every corner is labelled by matching its two
//! axes against those centres. The pure assignment math (canonical /
//! swapped cost, tolerance gate) and the undirected-circular-mean centre
//! refit live in [`projective_grid::cluster`]; this module maps the
//! generic outputs onto the chessboard [`ClusterLabel`] / [`AxisCluster`]
//! vocabulary the pipeline consumes.

use crate::corner::{ClusterLabel, CornerAug};
use projective_grid::cluster::{self as pg, assign_axes, AxisAssignment, AxisObservation};
use serde::Serialize;

use super::ClusterCenters;

/// Per-corner assignment produced by [`super::cluster_axes`].
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum AxisCluster {
    /// Axes matched both centers within `cluster_tol_deg`, with the
    /// given slot assignment.
    Labeled {
        label: ClusterLabel,
        /// Worst per-axis distance to its matched center (radians).
        max_d_rad: f32,
    },
    /// The best assignment still left one axis further than
    /// `cluster_tol_deg` from its matched center.
    Unclustered { max_d_rad: f32 },
}

/// Map a generic [`AxisAssignment`] onto the chessboard [`AxisCluster`].
pub(super) fn map_assignment(assign: AxisAssignment) -> AxisCluster {
    match assign {
        AxisAssignment::Canonical { max_d_rad } => AxisCluster::Labeled {
            label: ClusterLabel::Canonical,
            max_d_rad,
        },
        AxisAssignment::Swapped { max_d_rad } => AxisCluster::Labeled {
            label: ClusterLabel::Swapped,
            max_d_rad,
        },
        AxisAssignment::None { max_d_rad } => AxisCluster::Unclustered { max_d_rad },
        _ => unreachable!("AxisAssignment is exhaustively handled"),
    }
}

#[inline]
fn corner_axes(corner: &CornerAug) -> [AxisObservation; 2] {
    [
        AxisObservation::new(corner.axes[0].angle, corner.axes[0].sigma),
        AxisObservation::new(corner.axes[1].angle, corner.axes[1].sigma),
    ]
}

/// Per-corner cluster admission threshold in radians.
///
/// `min(cluster_tol_rad + cluster_sigma_k * max(σ_a0, σ_a1),
///      cluster_tol_rad + max_sigma_bonus_rad)` — sigma bonus is
/// capped so a single noisy corner cannot blow open the gate. Sigmas
/// at the no-info sentinel (≈ π) are clamped to a finite ceiling.
///
/// Delegates to [`projective_grid::cluster::effective_tol_rad`]; the cap
/// rationale (uncapped bonus destabilises the seed finder on
/// `puzzleboard_reference/example2.png`) lives there.
#[inline]
pub(crate) fn effective_tol_rad(corner: &CornerAug, base_tol_rad: f32, sigma_k: f32) -> f32 {
    pg::effective_tol_rad(&corner_axes(corner), base_tol_rad, sigma_k)
}

/// Refit cluster centres from the labelled set's axes only.
///
/// Stage-3 clustering on the full ChESS corner set produces centres
/// biased by marker-internal corners whose local axes don't agree
/// with the global chessboard grid (see CLAUDE.md "Evidence-driven
/// detector debugging" for the small3.png case study). After Stage 5
/// BFS, the labelled set is guaranteed to consist of true chessboard
/// intersections; their axes give an unbiased estimate of the grid
/// directions.
///
/// Delegates to [`projective_grid::cluster::refit_centers`] over the
/// labelled corners' axes. Returns `None` if
/// `labelled_indices.len() < min_samples`.
pub fn refit_centers_from_labelled(
    corners: &[CornerAug],
    labelled_indices: &[usize],
    old_centers: ClusterCenters,
    min_samples: usize,
) -> Option<ClusterCenters> {
    let axes: Vec<[AxisObservation; 2]> = labelled_indices
        .iter()
        .map(|&idx| corner_axes(&corners[idx]))
        .collect();
    pg::refit_centers(&axes, old_centers, min_samples)
}

/// Pure assignment of one corner to a label given known centers —
/// exposed for tests and for the Stage-3 re-check in boosters.
pub fn assign_corner(corner: &CornerAug, centers: ClusterCenters, tol_rad: f32) -> AxisCluster {
    map_assignment(assign_axes(&corner_axes(corner), centers, tol_rad))
}
