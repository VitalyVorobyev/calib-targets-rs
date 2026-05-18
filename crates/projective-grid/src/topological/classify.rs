//! Axis-driven grid-edge classification plus local triangle diagonal inference
//! (replaces the paper's color test).
//!
//! For a Delaunay half-edge from corner `a` to corner `b`, the edge angle
//! `θ = atan2(b - a)` is compared to each corner's two axes (modulo π,
//! since axes are undirected). If both endpoints see the edge within
//! `axis_align_tol_rad` of one usable axis, the edge is a **Grid** edge.
//!
//! Diagonals are not classified by a fixed `axis ± π/4` angle. Under a
//! projective warp, a projected cell diagonal is induced by the local
//! projected grid-step vectors, not by the angle bisector in image space.
//! After the Grid/Spurious pass, each Delaunay triangle is inspected: if
//! exactly two of its edges are Grid edges and those two edges meet at a
//! vertex using different local axis slots, the remaining edge is promoted
//! to **Diagonal** for that triangle.
//!
//! Both endpoints of every edge are guaranteed to have at least one
//! usable axis: high-`sigma` corners are filtered out at triangulation
//! time (see [`super::triangulate_usable`]), so `Spurious` here only
//! flags genuine geometric misalignment, not uncertainty rejection.

use std::f32::consts::{FRAC_PI_2, PI};

use nalgebra::Point2;
use serde::{Deserialize, Serialize};

use super::delaunay::Triangulation;
use super::{AxisEstimate, TopologicalParams};

/// Classification of a Delaunay edge against the recovered grid directions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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

#[derive(Clone, Copy, Debug, PartialEq)]
struct GridAxisMatch {
    slot: usize,
    distance_rad: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GridEdgeMatch {
    start_slot: usize,
    end_slot: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EdgeMetric {
    pub(crate) grid_distance_rad: Option<f32>,
    pub(crate) grid_margin_rad: Option<f32>,
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

/// Nearest usable grid axis to `theta` at this corner.
///
/// Both endpoints of every classified edge are guaranteed usable by the
/// upstream pre-filter (see [`super::triangulate_usable`]) — at least one
/// axis at each endpoint has `sigma < max_axis_sigma_rad`. The per-axis
/// `sigma` check below still skips an individual axis whose uncertainty
/// is too high while keeping the corner's other (good) axis active.
fn nearest_axis_at_corner(
    theta: f32,
    axes: &[AxisEstimate; 2],
    params: &TopologicalParams,
) -> Option<GridAxisMatch> {
    let mut best: Option<GridAxisMatch> = None;
    for (slot, a) in axes.iter().enumerate() {
        if !a.sigma.is_finite() || a.sigma >= params.max_axis_sigma_rad {
            continue;
        }
        let d = axis_diff(theta, a.angle);
        if !d.is_finite() {
            continue;
        }
        if best.is_none_or(|m| d < m.distance_rad) {
            best = Some(GridAxisMatch {
                slot,
                distance_rad: d,
            });
        }
    }
    best
}

/// Smallest angular distance from `theta` to a usable grid axis, in radians.
fn grid_distance_at_corner(
    theta: f32,
    axes: &[AxisEstimate; 2],
    params: &TopologicalParams,
) -> f32 {
    let best = nearest_axis_at_corner(theta, axes, params);
    debug_assert!(
        best.is_some(),
        "topological pre-filter must guarantee at least one usable axis per endpoint"
    );
    best.map_or(f32::INFINITY, |m| m.distance_rad)
}

fn grid_match_at_corner(
    theta: f32,
    axes: &[AxisEstimate; 2],
    params: &TopologicalParams,
) -> Option<GridAxisMatch> {
    let best = nearest_axis_at_corner(theta, axes, params)?;
    (best.distance_rad < params.axis_align_tol_rad).then_some(best)
}

pub(crate) fn classify_edge_metric(
    positions: &[Point2<f32>],
    axes: &[[AxisEstimate; 2]],
    triangulation: &Triangulation,
    edge: usize,
    params: &TopologicalParams,
) -> EdgeMetric {
    let a = triangulation.triangles[edge];
    let b = triangulation.triangles[Triangulation::next_edge(edge)];
    let pa = positions[a];
    let pb = positions[b];
    let theta = (pb.y - pa.y).atan2(pb.x - pa.x);
    let a_grid = grid_distance_at_corner(theta, &axes[a], params);
    let b_grid = grid_distance_at_corner(theta, &axes[b], params);
    let grid_distance_rad = a_grid.max(b_grid);
    EdgeMetric {
        grid_distance_rad: Some(grid_distance_rad),
        grid_margin_rad: Some(params.axis_align_tol_rad - grid_distance_rad),
    }
}

fn edge_vertices(triangulation: &Triangulation, edge: usize) -> (usize, usize) {
    (
        triangulation.triangles[edge],
        triangulation.triangles[Triangulation::next_edge(edge)],
    )
}

fn grid_axis_slot_at_vertex(
    triangulation: &Triangulation,
    grid_matches: &[Option<GridEdgeMatch>],
    edge: usize,
    vertex: usize,
) -> Option<usize> {
    let grid = grid_matches[edge]?;
    let (start, end) = edge_vertices(triangulation, edge);
    if vertex == start {
        Some(grid.start_slot)
    } else if vertex == end {
        Some(grid.end_slot)
    } else {
        None
    }
}

fn shared_vertex_of_edges(
    triangulation: &Triangulation,
    edge_a: usize,
    edge_b: usize,
) -> Option<usize> {
    let (a0, a1) = edge_vertices(triangulation, edge_a);
    let (b0, b1) = edge_vertices(triangulation, edge_b);
    if a0 == b0 || a0 == b1 {
        Some(a0)
    } else if a1 == b0 || a1 == b1 {
        Some(a1)
    } else {
        None
    }
}

fn infer_triangle_diagonal(
    triangulation: &Triangulation,
    grid_matches: &[Option<GridEdgeMatch>],
    kinds: &[EdgeKind],
    triangle: usize,
) -> Option<usize> {
    let base = 3 * triangle;
    let mut grid_edges = [usize::MAX; 2];
    let mut grid_count = 0usize;
    let mut non_grid_edge: Option<usize> = None;

    for k in 0..3 {
        let edge = base + k;
        match kinds[edge] {
            EdgeKind::Grid => {
                if grid_count >= grid_edges.len() {
                    return None;
                }
                grid_edges[grid_count] = edge;
                grid_count += 1;
            }
            EdgeKind::Spurious => {
                if non_grid_edge.is_some() {
                    return None;
                }
                non_grid_edge = Some(k);
            }
            EdgeKind::Diagonal => return None,
        }
    }
    if grid_count != 2 {
        return None;
    }

    let shared = shared_vertex_of_edges(triangulation, grid_edges[0], grid_edges[1])?;
    let slot0 = grid_axis_slot_at_vertex(triangulation, grid_matches, grid_edges[0], shared)?;
    let slot1 = grid_axis_slot_at_vertex(triangulation, grid_matches, grid_edges[1], shared)?;
    (slot0 != slot1).then_some(non_grid_edge?)
}

fn promote_triangle_diagonals_from_grid_edges(
    triangulation: &Triangulation,
    grid_matches: &[Option<GridEdgeMatch>],
    kinds: &mut [EdgeKind],
) {
    for triangle in 0..triangulation.num_tri() {
        if let Some(k) = infer_triangle_diagonal(triangulation, grid_matches, kinds, triangle) {
            kinds[3 * triangle + k] = EdgeKind::Diagonal;
        }
    }
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
    axes: &[[AxisEstimate; 2]],
    triangulation: &Triangulation,
    params: &TopologicalParams,
) -> Vec<EdgeKind> {
    let n = triangulation.triangles.len();
    let mut kinds = vec![EdgeKind::Spurious; n];
    let mut grid_matches = vec![None; n];
    for (e, kind) in kinds.iter_mut().enumerate().take(n) {
        let a = triangulation.triangles[e];
        let b = triangulation.triangles[Triangulation::next_edge(e)];
        let pa = positions[a];
        let pb = positions[b];
        let theta = (pb.y - pa.y).atan2(pb.x - pa.x);
        let at_a = grid_match_at_corner(theta, &axes[a], params);
        let at_b = grid_match_at_corner(theta, &axes[b], params);
        if let (Some(a_match), Some(b_match)) = (at_a, at_b) {
            grid_matches[e] = Some(GridEdgeMatch {
                start_slot: a_match.slot,
                end_slot: b_match.slot,
            });
            *kind = EdgeKind::Grid;
        }
    }
    promote_triangle_diagonals_from_grid_edges(triangulation, &grid_matches, &mut kinds);
    kinds
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_4;

    fn axes(angle0: f32, angle1: f32) -> [AxisEstimate; 2] {
        [
            AxisEstimate {
                angle: angle0,
                sigma: 0.05,
            },
            AxisEstimate {
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
        let horizontal = grid_match_at_corner(0.0, &a, &p).unwrap();
        assert_eq!(horizontal.slot, 0);
        assert!(horizontal.distance_rad < 1e-6);
        // Edge angle = π/2 → aligned with second axis.
        let vertical = grid_match_at_corner(FRAC_PI_2, &a, &p).unwrap();
        assert_eq!(vertical.slot, 1);
        assert!(vertical.distance_rad < 1e-6);
    }

    #[test]
    fn diagonal_angle_edge_is_not_a_grid_match() {
        let p = TopologicalParams::default();
        let a = axes(0.0, FRAC_PI_2);
        // An edge at 45° to both axes is outside the grid-angle gate.
        assert!(grid_match_at_corner(FRAC_PI_4, &a, &p).is_none());
        assert!((grid_distance_at_corner(FRAC_PI_4, &a, &p) - FRAC_PI_4).abs() < 1e-6);
    }

    #[test]
    fn unaligned_edge_is_spurious() {
        let p = TopologicalParams::default();
        let a = axes(0.0, FRAC_PI_2);
        // 22° from horizontal axis: outside the 15° grid tolerance.
        assert!(grid_match_at_corner(22.0_f32.to_radians(), &a, &p).is_none());
    }
}
