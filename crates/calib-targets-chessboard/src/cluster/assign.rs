//! Per-corner label assignment, tolerance computation, and centre refit.
//!
//! Once the two grid-direction centres `(Θ₀, Θ₁)` are known (see
//! [`super::cluster_axes`]), every corner is labelled by matching its two
//! axes against those centres. This module owns that pure assignment math
//! plus the post-grow centre refit that recomputes the centres from the
//! labelled set's axes alone.

use std::f32::consts::PI;

use crate::circular_stats::{angular_dist_pi, wrap_pi};
use crate::corner::{ClusterLabel, CornerAug};
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

/// Per-corner cluster admission threshold in radians.
///
/// `min(cluster_tol_rad + cluster_sigma_k * max(σ_a0, σ_a1),
///      cluster_tol_rad + max_sigma_bonus_rad)` — sigma bonus is
/// capped so a single noisy corner cannot blow open the gate. Sigmas
/// at the no-info sentinel (≈ π) are clamped to a finite ceiling.
///
/// The cap exists because admitting too many borderline corners
/// destabilises the seed finder: with more clustered candidates the
/// first-rank seed quad can land on a sub-grid spaced 1.4× cell
/// (sqrt(2)×, the diagonal step), which then grows a sparse,
/// inconsistent (i, j) frame. Confirmed empirically on
/// `puzzleboard_reference/example2.png` — uncapped bonus turned a
/// 134-label / 32-pixel-cell detection into a 127-label /
/// 45-pixel-cell detection with `SHIFT-INCONSISTENT` errors.
#[inline]
pub(crate) fn effective_tol_rad(corner: &CornerAug, base_tol_rad: f32, sigma_k: f32) -> f32 {
    if sigma_k <= 0.0 {
        return base_tol_rad;
    }
    // Hard cap on sigma input. The no-info sentinel is π, and any
    // sigma above ~10° on a real corner is already noise-dominated.
    let sigma_cap = 10.0_f32.to_radians();
    let s0 = corner.axes[0].sigma.clamp(0.0, sigma_cap);
    let s1 = corner.axes[1].sigma.clamp(0.0, sigma_cap);
    let bonus = sigma_k * s0.max(s1);
    // Hard cap on the bonus itself: never exceed +3° over the base
    // tolerance. This keeps the effective gate within `[base_tol,
    // base_tol + 3°]` regardless of sigma.
    let max_bonus = 3.0_f32.to_radians();
    base_tol_rad + bonus.min(max_bonus)
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
/// For each labelled corner, pick the slot assignment (Canonical /
/// Swapped) that minimises the cost under `old_centers` — same
/// tie-break rule as [`assign_corner`] — to determine which of its two
/// axes belongs to slot 0 vs slot 1. Accumulate `(cos 2θ, sin 2θ)`
/// per slot (undirected circular mean — mandated by the workspace
/// "axes-only" contract; see CLAUDE.md "Corner orientation contract"),
/// halve the atan2, wrap to `[0, π)`, and order so `θ0 < θ1`.
///
/// Returns `None` if `labelled_indices.len() < min_samples` (the
/// caller should keep the original centres).
pub fn refit_centers_from_labelled(
    corners: &[CornerAug],
    labelled_indices: &[usize],
    old_centers: ClusterCenters,
    min_samples: usize,
) -> Option<ClusterCenters> {
    if labelled_indices.len() < min_samples {
        return None;
    }
    let mut s0_re = 0.0_f32;
    let mut s0_im = 0.0_f32;
    let mut s1_re = 0.0_f32;
    let mut s1_im = 0.0_f32;
    for &idx in labelled_indices {
        let c = &corners[idx];
        let a0 = wrap_pi(c.axes[0].angle);
        let a1 = wrap_pi(c.axes[1].angle);
        let d_a0_t0 = angular_dist_pi(a0, old_centers.theta0);
        let d_a0_t1 = angular_dist_pi(a0, old_centers.theta1);
        let d_a1_t0 = angular_dist_pi(a1, old_centers.theta0);
        let d_a1_t1 = angular_dist_pi(a1, old_centers.theta1);
        let canon_cost = d_a0_t0 + d_a1_t1;
        let swap_cost = d_a0_t1 + d_a1_t0;
        let (a_to_t0, a_to_t1) = if canon_cost <= swap_cost {
            (a0, a1)
        } else {
            (a1, a0)
        };
        s0_re += (2.0 * a_to_t0).cos();
        s0_im += (2.0 * a_to_t0).sin();
        s1_re += (2.0 * a_to_t1).cos();
        s1_im += (2.0 * a_to_t1).sin();
    }
    let mut t0 = 0.5 * s0_im.atan2(s0_re);
    let mut t1 = 0.5 * s1_im.atan2(s1_re);
    while t0 < 0.0 {
        t0 += PI;
    }
    while t0 >= PI {
        t0 -= PI;
    }
    while t1 < 0.0 {
        t1 += PI;
    }
    while t1 >= PI {
        t1 -= PI;
    }
    if t0 > t1 {
        std::mem::swap(&mut t0, &mut t1);
    }
    Some(ClusterCenters {
        theta0: t0,
        theta1: t1,
    })
}

/// Pure assignment of one corner to a label given known centers —
/// exposed for tests and for the Stage-3 re-check in boosters.
pub fn assign_corner(corner: &CornerAug, centers: ClusterCenters, tol_rad: f32) -> AxisCluster {
    let a0 = wrap_pi(corner.axes[0].angle);
    let a1 = wrap_pi(corner.axes[1].angle);

    let d_a0_t0 = angular_dist_pi(a0, centers.theta0);
    let d_a0_t1 = angular_dist_pi(a0, centers.theta1);
    let d_a1_t0 = angular_dist_pi(a1, centers.theta0);
    let d_a1_t1 = angular_dist_pi(a1, centers.theta1);

    // Canonical: axes[0] → Θ₀, axes[1] → Θ₁. Cost = d(0,0)+d(1,1).
    let canon_cost = d_a0_t0 + d_a1_t1;
    let canon_max = d_a0_t0.max(d_a1_t1);
    // Swapped: axes[0] → Θ₁, axes[1] → Θ₀.
    let swap_cost = d_a0_t1 + d_a1_t0;
    let swap_max = d_a0_t1.max(d_a1_t0);

    let (label, max_d) = if canon_cost <= swap_cost {
        (ClusterLabel::Canonical, canon_max)
    } else {
        (ClusterLabel::Swapped, swap_max)
    };

    if max_d <= tol_rad {
        AxisCluster::Labeled {
            label,
            max_d_rad: max_d,
        }
    } else {
        AxisCluster::Unclustered { max_d_rad: max_d }
    }
}
