//! Chessboard seed finder.
//!
//! Find a `2×2` cell whose 4 corners satisfy the chessboard's per-edge
//! invariants — axis-slot swap, parity-required cluster labels, no 2×-
//! spacing midpoint violation — and estimate the cell size from that
//! seed. The cell-size output replaces the global cell-size estimator
//! (which empirically mispicks under bimodal distance distributions
//! produced by ChArUco markers).
//!
//! Layout and parity:
//! ```text
//!                     A.axes[0]
//!  A (0,0)  ───── AB ────── B (1,0)
//!   Canonical                Swapped
//!     │                         │
//!     │ A.axes[1]               │
//!     │                         │
//!  C (0,1)  ───── CD ────── D (1,1)
//!   Swapped                   Canonical
//! ```
//!
//! The pattern-agnostic geometry — KD-tree neighbour search, axis
//! classification of B vs C, parallelogram completion, edge-ratio
//! match — lives in `projective_grid::square::seed_finder`. This
//! module supplies the chessboard-specific `SeedQuadValidator` impl:
//! parity-aware A/BC partition, the axis-slot-swap edge gate, and
//! the chessboard's midpoint-violation test.

use crate::cluster::{angular_dist_pi, wrap_pi, ClusterCenters};
use crate::corner::{ClusterLabel, CornerAug, CornerStage};
use crate::params::DetectorParams;
use nalgebra::Point2;
use projective_grid::square::seed_finder::{find_quad, SeedQuadParams, SeedQuadValidator};

// `Seed` and `SeedOutput` live in `projective_grid::square::seed` so
// non-chessboard grid-detector pipelines can share the same 2×2 seed
// data carrier + `(seed, cell_size)` bundle. Chessboard re-exports
// them here under their historical names.
//
// The positional convention (A at (0, 0) / B at (1, 0) / C at (0, 1)
// / D at (1, 1)) matches [`projective_grid::square::grow::bfs_grow`]
// exactly — the seed comes out of this module and goes directly into
// grow with no index permutation. For chessboards, A and D are
// `Canonical`-cluster corners and B, C are `Swapped` under the seed's
// parity-fixing convention.
pub use projective_grid::square::seed::{Seed, SeedOutput};

/// Find a valid seed. Cell size comes OUT of the seed (no cell-size input).
///
/// Tries the primary tolerance first; on no seed, retries with every
/// tolerance widened by 1.5×.
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(level = "debug", skip_all, fields(num_corners = corners.len()))
)]
pub fn find_seed(
    corners: &[CornerAug],
    centers: ClusterCenters,
    params: &DetectorParams,
) -> Option<SeedOutput> {
    let _ = centers; // kept in the signature for caller stability; not used
    let validator = ChessboardSeedValidator::new(corners);
    find_with_slack(&validator, params, 1.0).or_else(|| find_with_slack(&validator, params, 1.5))
}

fn find_with_slack<V: SeedQuadValidator>(
    v: &V,
    params: &DetectorParams,
    slack: f32,
) -> Option<SeedOutput> {
    let pg_params = SeedQuadParams::new(
        params.seed_axis_tol_deg.to_radians() * slack,
        params.seed_edge_tol * slack,
        params.seed_close_tol * slack,
    );
    find_quad(v, &pg_params)
}

/// Chessboard plug-in for the generic
/// [`SeedQuadValidator`](projective_grid::square::seed_finder::SeedQuadValidator).
struct ChessboardSeedValidator<'a> {
    corners: &'a [CornerAug],
    /// Canonical-cluster indices, sorted by descending strength so the
    /// highest-quality A candidates are tried first.
    canonical: Vec<usize>,
    /// Swapped-cluster indices.
    swapped: Vec<usize>,
}

impl<'a> ChessboardSeedValidator<'a> {
    fn new(corners: &'a [CornerAug]) -> Self {
        let label_of = |i: usize| match corners[i].stage {
            CornerStage::Clustered { label } => Some(label),
            _ => None,
        };
        let mut canonical: Vec<usize> = (0..corners.len())
            .filter(|&i| label_of(i) == Some(ClusterLabel::Canonical))
            .collect();
        canonical.sort_by(|&i, &j| corners[j].strength.total_cmp(&corners[i].strength));
        let swapped: Vec<usize> = (0..corners.len())
            .filter(|&i| label_of(i) == Some(ClusterLabel::Swapped))
            .collect();
        Self {
            corners,
            canonical,
            swapped,
        }
    }
}

impl<'a> SeedQuadValidator for ChessboardSeedValidator<'a> {
    fn position(&self, idx: usize) -> Point2<f32> {
        self.corners[idx].position
    }

    fn axes(&self, idx: usize) -> [f32; 2] {
        let c = &self.corners[idx];
        [c.axes[0].angle, c.axes[1].angle]
    }

    fn a_candidates(&self) -> Vec<usize> {
        self.canonical.clone()
    }

    fn bc_candidates(&self) -> Vec<usize> {
        self.swapped.clone()
    }

    /// Axis-slot-swap invariant on the directed edge `from → to`: the
    /// chord direction must match one slot at `from` and the OTHER
    /// slot at `to`. Same check the BFS validator's `edge_ok` uses.
    fn edge_ok(&self, from: usize, to: usize, axis_tol_rad: f32) -> bool {
        let a = &self.corners[from];
        let b = &self.corners[to];
        let off = b.position - a.position;
        let ang = wrap_pi(off.y.atan2(off.x));
        let d_a0 = angular_dist_pi(ang, wrap_pi(a.axes[0].angle));
        let d_a1 = angular_dist_pi(ang, wrap_pi(a.axes[1].angle));
        let (slot_a, d_a) = if d_a0 <= d_a1 { (0, d_a0) } else { (1, d_a1) };
        if d_a > axis_tol_rad {
            return false;
        }
        let d_b0 = angular_dist_pi(ang, wrap_pi(b.axes[0].angle));
        let d_b1 = angular_dist_pi(ang, wrap_pi(b.axes[1].angle));
        let (slot_b, d_b) = if d_b0 <= d_b1 { (0, d_b0) } else { (1, d_b1) };
        if d_b > axis_tol_rad {
            return false;
        }
        slot_a != slot_b
    }

    /// 2×-spacing rejection: the seed quad is invalid when ANY real
    /// corner (clustered or not) sits near an edge midpoint or the
    /// parallelogram center, on top of the chessboard-specific
    /// signals (a `Swapped`-cluster corner at a midpoint, a
    /// `Canonical`-cluster corner at the center). The "any real
    /// corner" fallback catches sqrt(2)× (diagonal) and 2× cases
    /// where the intermediate corner failed Stage-3 clustering and
    /// isn't in `self.swapped` / `self.canonical`.
    fn has_midpoint_violation(
        &self,
        seed: projective_grid::square::seed::Seed,
        cell_size: f32,
    ) -> bool {
        let positions: Vec<Point2<f32>> = self.corners.iter().map(|c| c.position).collect();
        let all_idx: Vec<usize> = (0..self.corners.len()).collect();
        projective_grid::square::seed::seed_has_midpoint_violation(
            &positions,
            [seed.a, seed.b, seed.c, seed.d],
            cell_size,
            0.3,
            &self.swapped,
            &self.canonical,
            &all_idx,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::cluster_axes;
    use calib_targets_core::{AxisEstimate, Corner};
    use nalgebra::Point2;

    fn make_corner(
        idx: usize,
        x: f32,
        y: f32,
        axis_u: f32,
        axis_v: f32,
        label: ClusterLabel,
        strength: f32,
    ) -> CornerAug {
        let axes = match label {
            ClusterLabel::Canonical => [axis_u, axis_v],
            ClusterLabel::Swapped => [axis_v, axis_u],
        };
        let c = Corner {
            position: Point2::new(x, y),
            orientation_cluster: None,
            axes: [
                AxisEstimate {
                    angle: wrap_pi(axes[0]),
                    sigma: 0.01,
                },
                AxisEstimate {
                    angle: wrap_pi(axes[1]),
                    sigma: 0.01,
                },
            ],
            contrast: 10.0,
            fit_rms: 1.0,
            strength,
        };
        let mut aug = CornerAug::from_corner(idx, &c);
        aug.stage = CornerStage::Strong;
        aug
    }

    fn build_clean_grid(rows: i32, cols: i32, s: f32) -> Vec<CornerAug> {
        let axis_u = 0.0_f32;
        let axis_v = std::f32::consts::FRAC_PI_2;
        let mut out = Vec::new();
        let mut idx = 0;
        for j in 0..rows {
            for i in 0..cols {
                let x = i as f32 * s + 50.0;
                let y = j as f32 * s + 50.0;
                let label = if (i + j).rem_euclid(2) == 0 {
                    ClusterLabel::Canonical
                } else {
                    ClusterLabel::Swapped
                };
                out.push(make_corner(idx, x, y, axis_u, axis_v, label, 1.0));
                idx += 1;
            }
        }
        out
    }

    #[test]
    fn finds_seed_on_clean_5x5_grid() {
        let mut corners = build_clean_grid(5, 5, 20.0);
        let params = DetectorParams::default();
        let centers = cluster_axes(&mut corners, &params).expect("centers");
        let out = find_seed(&corners, centers, &params).expect("seed");
        assert!((out.cell_size - 20.0).abs() < 0.5);

        let label_of = |i: usize| match corners[i].stage {
            CornerStage::Clustered { label } => label,
            ref other => {
                unreachable!("seed corner {i} must be Clustered after find_seed, got {other:?}")
            }
        };
        assert_eq!(label_of(out.seed.a), ClusterLabel::Canonical);
        assert_eq!(label_of(out.seed.b), ClusterLabel::Swapped);
        assert_eq!(label_of(out.seed.c), ClusterLabel::Swapped);
        assert_eq!(label_of(out.seed.d), ClusterLabel::Canonical);
    }

    #[test]
    fn returns_none_on_isolated_cluster0_corner() {
        let mut corners = vec![make_corner(
            0,
            100.0,
            100.0,
            0.0,
            std::f32::consts::FRAC_PI_2,
            ClusterLabel::Canonical,
            1.0,
        )];
        let params = DetectorParams::default();
        let centers = ClusterCenters {
            theta0: 0.0,
            theta1: std::f32::consts::FRAC_PI_2,
        };
        corners[0].stage = CornerStage::Clustered {
            label: ClusterLabel::Canonical,
        };
        assert!(find_seed(&corners, centers, &params).is_none());
    }

    #[test]
    fn rotated_grid_seed() {
        let theta = 30.0_f32.to_radians();
        let axis_u = theta;
        let axis_v = theta + std::f32::consts::FRAC_PI_2;
        let s = 25.0;
        let mut corners = Vec::new();
        let mut idx = 0;
        for j in 0..5_i32 {
            for i in 0..5_i32 {
                let dx = i as f32 * s * axis_u.cos() + j as f32 * s * axis_v.cos();
                let dy = i as f32 * s * axis_u.sin() + j as f32 * s * axis_v.sin();
                let label = if (i + j).rem_euclid(2) == 0 {
                    ClusterLabel::Canonical
                } else {
                    ClusterLabel::Swapped
                };
                corners.push(make_corner(
                    idx,
                    100.0 + dx,
                    100.0 + dy,
                    axis_u,
                    axis_v,
                    label,
                    1.0,
                ));
                idx += 1;
            }
        }
        let params = DetectorParams::default();
        let centers = cluster_axes(&mut corners, &params).expect("centers");
        let out = find_seed(&corners, centers, &params).expect("seed");
        assert!((out.cell_size - s).abs() < 1.0);
    }

    #[test]
    fn handles_widely_varying_cell_size_among_clusters() {
        // Create a grid where TRUE cell is 60 — the new self-consistent
        // seed uses the cluster corners only to measure cell size.
        let s = 60.0_f32;
        let mut corners = build_clean_grid(4, 4, s);
        let params = DetectorParams::default();
        let centers = cluster_axes(&mut corners, &params).expect("centers");
        let out = find_seed(&corners, centers, &params).expect("seed");
        assert!(
            (out.cell_size - s).abs() < 1.0,
            "cell_size = {:.2} off from {s}",
            out.cell_size
        );
    }
}
