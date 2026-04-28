//! Axis-driven edge classification (replaces the paper's color test).
//!
//! For a Delaunay half-edge from corner `a` to corner `b`, the edge angle
//! `θ = atan2(b - a)` is compared to each corner's two axes (modulo π,
//! since axes are undirected). The minimum angular distance to either
//! axis at each endpoint determines the edge's classification at that
//! endpoint:
//!
//! - within `axis_align_tol_rad` of an axis → **Grid** (the edge runs
//!   along a chessboard cell side at this corner);
//! - within `diagonal_angle_tol_rad` of `axis ± π/4` → **Diagonal** (the
//!   edge crosses a chessboard cell at this corner);
//! - otherwise → **Spurious** (background or unaligned noise).
//!
//! The whole-edge classification is the conjunction of the per-endpoint
//! classifications: an edge is `Grid` iff both endpoints see it as
//! `Grid`, `Diagonal` iff both see it as `Diagonal`, otherwise
//! `Spurious`. This is the axis-only analogue of the paper's "shared
//! edge of a same-color triangle pair is the diagonal of a cell".

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

use nalgebra::Point2;

use super::delaunay::Triangulation;
use super::{AxisHint, TopologicalParams};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum EdgeKind {
    /// Edge runs along a grid line (cell edge in the chessboard pattern).
    Grid,
    /// Edge crosses a cell as its diagonal.
    Diagonal,
    /// Edge is unaligned with any grid direction (background, noise,
    /// occlusion).
    Spurious,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EdgeAt {
    Grid,
    Diagonal,
    Spurious,
}

/// Smallest unsigned angle between two undirected directions, in `[0, π/2]`.
///
/// Both `theta` and `alpha` are interpreted modulo π (axes are
/// undirected). The result is the geodesic distance on the half-circle.
#[inline]
fn axis_diff(theta: f32, alpha: f32) -> f32 {
    let mut d = (theta - alpha).rem_euclid(PI);
    if d > FRAC_PI_2 {
        d = PI - d;
    }
    d
}

fn classify_at_corner(theta: f32, axes: &[AxisHint; 2], params: &TopologicalParams) -> EdgeAt {
    // Pick the smaller axis-distance over the two axes; this is well-defined
    // even when one axis has sigma = π, because we only use angles, and the
    // pre-filter already excludes corners where both axes are unusable.
    let mut min_d = f32::INFINITY;
    for a in axes.iter() {
        if a.sigma >= params.max_axis_sigma_rad {
            continue;
        }
        let d = axis_diff(theta, a.angle);
        if d < min_d {
            min_d = d;
        }
    }
    if !min_d.is_finite() {
        return EdgeAt::Spurious;
    }
    if min_d < params.axis_align_tol_rad {
        return EdgeAt::Grid;
    }
    let dia = (min_d - FRAC_PI_4).abs();
    if dia < params.diagonal_angle_tol_rad {
        return EdgeAt::Diagonal;
    }
    EdgeAt::Spurious
}

/// Classify every directed half-edge in the triangulation.
///
/// Length matches `triangulation.triangles.len()`.
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "debug",
        skip_all,
        fields(num_edges = triangulation.triangles.len()),
    )
)]
pub(crate) fn classify_all_edges(
    positions: &[Point2<f32>],
    axes: &[[AxisHint; 2]],
    usable: &[bool],
    triangulation: &Triangulation,
    params: &TopologicalParams,
) -> Vec<EdgeKind> {
    let n = triangulation.triangles.len();
    let mut kinds = vec![EdgeKind::Spurious; n];
    for (e, kind) in kinds.iter_mut().enumerate().take(n) {
        let a = triangulation.triangles[e];
        let b = triangulation.triangles[Triangulation::next_edge(e)];
        if !usable[a] || !usable[b] {
            // Corner without axis info — drop the edge.
            continue;
        }
        let pa = positions[a];
        let pb = positions[b];
        let theta = (pb.y - pa.y).atan2(pb.x - pa.x);
        let at_a = classify_at_corner(theta, &axes[a], params);
        let at_b = classify_at_corner(theta, &axes[b], params);
        *kind = match (at_a, at_b) {
            (EdgeAt::Grid, EdgeAt::Grid) => EdgeKind::Grid,
            (EdgeAt::Diagonal, EdgeAt::Diagonal) => EdgeKind::Diagonal,
            _ => EdgeKind::Spurious,
        };
    }
    kinds
}

#[cfg(test)]
mod tests {
    use super::*;

    fn axes(angle0: f32, angle1: f32) -> [AxisHint; 2] {
        [
            AxisHint {
                angle: angle0,
                sigma: 0.05,
            },
            AxisHint {
                angle: angle1,
                sigma: 0.05,
            },
        ]
    }

    #[test]
    fn axis_diff_is_symmetric_modulo_pi() {
        assert!((axis_diff(0.0, PI) - 0.0).abs() < 1e-6);
        assert!((axis_diff(0.1, 0.0) - 0.1).abs() < 1e-6);
        assert!((axis_diff(PI - 0.1, 0.0) - 0.1).abs() < 1e-6);
        assert!((axis_diff(FRAC_PI_4, 0.0) - FRAC_PI_4).abs() < 1e-6);
    }

    #[test]
    fn axis_aligned_edge_is_grid() {
        let p = TopologicalParams::default();
        let a = axes(0.0, FRAC_PI_2);
        // Edge angle = 0 → aligned with first axis at (almost) zero distance.
        assert_eq!(classify_at_corner(0.0, &a, &p), EdgeAt::Grid);
        // Edge angle = π/2 → aligned with second axis.
        assert_eq!(classify_at_corner(FRAC_PI_2, &a, &p), EdgeAt::Grid);
    }

    #[test]
    fn diagonal_edge_is_diagonal() {
        let p = TopologicalParams::default();
        let a = axes(0.0, FRAC_PI_2);
        assert_eq!(classify_at_corner(FRAC_PI_4, &a, &p), EdgeAt::Diagonal);
        assert_eq!(classify_at_corner(-FRAC_PI_4, &a, &p), EdgeAt::Diagonal);
    }

    #[test]
    fn unaligned_edge_is_spurious() {
        let p = TopologicalParams::default();
        let a = axes(0.0, FRAC_PI_2);
        // 22° from horizontal axis: outside the 15° grid tolerance and
        // far from the 45° diagonal (|22-45| = 23° > 15° tol).
        assert_eq!(
            classify_at_corner(22.0_f32.to_radians(), &a, &p),
            EdgeAt::Spurious
        );
    }
}
