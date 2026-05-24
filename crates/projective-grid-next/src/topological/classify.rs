//! Axis-driven grid-edge classification plus local triangle diagonal inference
//! (replaces the SBF09 paper's color test).
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
//! Both endpoints of every edge are guaranteed to have at least one usable
//! axis: corners whose both axes are uninformative are filtered out by the
//! topological pre-filter, so `Spurious` here only flags genuine geometric
//! misalignment, not uncertainty rejection.
//!
//! ## Float discipline
//!
//! Every helper is `F: Float`-generic. The legacy crate hard-coded `f32` and
//! used `std::f32::consts::PI`; the port draws `PI` from `F::pi()` and
//! literal constants via a small private `lit` helper from
//! [`crate::float`].

use nalgebra::Point2;

use super::delaunay::Triangulation;
use crate::diagnostics::events::EdgeClass;
use crate::feature::AxisEstimate;
use crate::float::{lit, Float};

/// Per-corner result of matching the half-edge against the two axes.
#[derive(Clone, Copy, Debug, PartialEq)]
struct GridAxisMatch<F: Float> {
    slot: usize,
    distance_rad: F,
}

/// Joint per-half-edge result of matching against both endpoints' axes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GridEdgeMatch {
    start_slot: usize,
    end_slot: usize,
}

/// Per-half-edge diagnostic metric used by the event stream and any
/// downstream trace consumer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EdgeMetric<F: Float> {
    pub(crate) grid_distance_rad: Option<F>,
    pub(crate) grid_margin_rad: Option<F>,
}

/// Smallest unsigned angle between two undirected directions, in `[0, π/2]`.
///
/// Both `theta` and `alpha` are interpreted modulo π (axes are
/// undirected). The result is the geodesic distance on the half-circle.
#[inline]
fn axis_diff<F: Float>(theta: F, alpha: F) -> F {
    let pi = F::pi();
    let half_pi = pi / lit::<F>(2.0_f32);
    // Wrap to [0, π).
    let mut d = (theta - alpha) % pi;
    if d < F::zero() {
        d += pi;
    }
    if d > half_pi {
        d = pi - d;
    }
    d
}

/// Nearest usable grid axis to `theta` at this corner.
///
/// Both endpoints of every classified edge are guaranteed usable by the
/// upstream pre-filter — at least one axis at each endpoint is informative
/// (`sigma < π`). The per-axis sigma check below still skips an individual
/// axis whose uncertainty is too high while keeping the corner's other
/// (good) axis active.
fn nearest_axis_at_corner<F: Float>(
    theta: F,
    axes: &[AxisEstimate<F>; 2],
) -> Option<GridAxisMatch<F>> {
    let mut best: Option<GridAxisMatch<F>> = None;
    for (slot, a) in axes.iter().enumerate() {
        if !a.is_informative() {
            continue;
        }
        if !a.sigma.is_finite() {
            continue;
        }
        let d = axis_diff(theta, a.angle);
        if !d.is_finite() {
            continue;
        }
        match best {
            None => {
                best = Some(GridAxisMatch {
                    slot,
                    distance_rad: d,
                });
            }
            Some(m) if d < m.distance_rad => {
                best = Some(GridAxisMatch {
                    slot,
                    distance_rad: d,
                });
            }
            _ => {}
        }
    }
    best
}

/// Smallest angular distance from `theta` to a usable grid axis, in radians.
/// Falls back to `+inf` when no axis is informative (this should not occur
/// after the pre-filter, but the helper stays total to satisfy the
/// `EdgeMetric` contract).
fn grid_distance_at_corner<F: Float>(theta: F, axes: &[AxisEstimate<F>; 2]) -> F {
    nearest_axis_at_corner(theta, axes)
        .map_or(F::max_value().unwrap_or(F::pi()), |m| m.distance_rad)
}

fn grid_match_at_corner<F: Float>(
    theta: F,
    axes: &[AxisEstimate<F>; 2],
    align_tol_rad: F,
) -> Option<GridAxisMatch<F>> {
    let best = nearest_axis_at_corner(theta, axes)?;
    (best.distance_rad < align_tol_rad).then_some(best)
}

/// Per-half-edge diagnostic metric (largest endpoint angular distance to a
/// usable grid axis plus the resulting margin against the alignment
/// tolerance). Used by tests and any downstream trace consumer.
#[allow(dead_code)] // exercised by tests; promoted to internal API on demand
pub(crate) fn classify_edge_metric<F: Float>(
    positions: &[Point2<F>],
    axes: &[[AxisEstimate<F>; 2]],
    triangulation: &Triangulation,
    edge: usize,
    align_tol_rad: F,
) -> EdgeMetric<F> {
    let a = triangulation.triangles[edge];
    let b = triangulation.triangles[Triangulation::next_edge(edge)];
    let pa = positions[a];
    let pb = positions[b];
    let theta = (pb.y - pa.y).atan2(pb.x - pa.x);
    let a_grid = grid_distance_at_corner(theta, &axes[a]);
    let b_grid = grid_distance_at_corner(theta, &axes[b]);
    let grid_distance_rad = if a_grid > b_grid { a_grid } else { b_grid };
    EdgeMetric {
        grid_distance_rad: Some(grid_distance_rad),
        grid_margin_rad: Some(align_tol_rad - grid_distance_rad),
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
    kinds: &[EdgeClass],
    triangle: usize,
) -> Option<usize> {
    let base = 3 * triangle;
    let mut grid_edges = [usize::MAX; 2];
    let mut grid_count = 0usize;
    let mut non_grid_edge: Option<usize> = None;

    for k in 0..3 {
        let edge = base + k;
        match kinds[edge] {
            EdgeClass::Grid => {
                if grid_count >= grid_edges.len() {
                    return None;
                }
                grid_edges[grid_count] = edge;
                grid_count += 1;
            }
            EdgeClass::Spurious | EdgeClass::Unknown => {
                if non_grid_edge.is_some() {
                    return None;
                }
                non_grid_edge = Some(k);
            }
            EdgeClass::Diagonal => return None,
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
    kinds: &mut [EdgeClass],
) {
    for triangle in 0..triangulation.num_tri() {
        if let Some(k) = infer_triangle_diagonal(triangulation, grid_matches, kinds, triangle) {
            kinds[3 * triangle + k] = EdgeClass::Diagonal;
        }
    }
}

/// Classify every directed half-edge in the triangulation.
///
/// Length matches `triangulation.triangles.len()`. The bare classifier is
/// allocation-free aside from the output `Vec`; event emission is handled
/// in the orchestrator so this helper is free of `DiagnosticSink` plumbing.
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "debug",
        skip_all,
        fields(num_edges = triangulation.triangles.len()),
    )
)]
pub(crate) fn classify_all_edges<F: Float>(
    positions: &[Point2<F>],
    axes: &[[AxisEstimate<F>; 2]],
    triangulation: &Triangulation,
    align_tol_rad: F,
) -> Vec<EdgeClass> {
    let n = triangulation.triangles.len();
    let mut kinds = vec![EdgeClass::Spurious; n];
    let mut grid_matches = vec![None; n];
    for (e, kind) in kinds.iter_mut().enumerate().take(n) {
        let a = triangulation.triangles[e];
        let b = triangulation.triangles[Triangulation::next_edge(e)];
        let pa = positions[a];
        let pb = positions[b];
        let theta = (pb.y - pa.y).atan2(pb.x - pa.x);
        let at_a = grid_match_at_corner(theta, &axes[a], align_tol_rad);
        let at_b = grid_match_at_corner(theta, &axes[b], align_tol_rad);
        if let (Some(a_match), Some(b_match)) = (at_a, at_b) {
            grid_matches[e] = Some(GridEdgeMatch {
                start_slot: a_match.slot,
                end_slot: b_match.slot,
            });
            *kind = EdgeClass::Grid;
        }
    }
    promote_triangle_diagonals_from_grid_edges(triangulation, &grid_matches, &mut kinds);
    kinds
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::float::lit;

    fn axes<F: Float>(angle0: f32, angle1: f32) -> [AxisEstimate<F>; 2] {
        [
            AxisEstimate::new(lit::<F>(angle0), lit::<F>(0.05_f32)),
            AxisEstimate::new(lit::<F>(angle1), lit::<F>(0.05_f32)),
        ]
    }

    fn assert_axis_diff_is_symmetric_modulo_pi<F: Float>() {
        let pi = F::pi();
        let frac_pi_4 = pi / lit::<F>(4.0_f32);
        let eps = lit::<F>(1e-5_f32);
        assert!(crate::float::abs::<F>(axis_diff::<F>(F::zero(), pi)) < eps);
        let one_tenth = lit::<F>(0.1_f32);
        assert!(crate::float::abs::<F>(axis_diff::<F>(one_tenth, F::zero()) - one_tenth) < eps);
        assert!(
            crate::float::abs::<F>(axis_diff::<F>(pi - one_tenth, F::zero()) - one_tenth) < eps
        );
        assert!(crate::float::abs::<F>(axis_diff::<F>(frac_pi_4, F::zero()) - frac_pi_4) < eps);
    }

    fn assert_axis_aligned_edge_is_grid<F: Float>() {
        let pi = F::pi();
        let frac_pi_2 = pi / lit::<F>(2.0_f32);
        let tol = lit::<F>(15.0_f32.to_radians());
        let a = axes::<F>(0.0, frac_pi_2.to_subset_unchecked());
        // Edge angle = 0 → aligned with first axis at (almost) zero distance.
        let horizontal = grid_match_at_corner(F::zero(), &a, tol).unwrap();
        assert_eq!(horizontal.slot, 0);
        assert!(crate::float::abs::<F>(horizontal.distance_rad) < lit::<F>(1e-5_f32));
        // Edge angle = π/2 → aligned with second axis.
        let vertical = grid_match_at_corner(frac_pi_2, &a, tol).unwrap();
        assert_eq!(vertical.slot, 1);
        assert!(crate::float::abs::<F>(vertical.distance_rad) < lit::<F>(1e-5_f32));
    }

    fn assert_diagonal_angle_is_not_grid<F: Float>() {
        let pi = F::pi();
        let frac_pi_2 = pi / lit::<F>(2.0_f32);
        let frac_pi_4 = pi / lit::<F>(4.0_f32);
        let tol = lit::<F>(15.0_f32.to_radians());
        let a = axes::<F>(0.0, frac_pi_2.to_subset_unchecked());
        assert!(grid_match_at_corner(frac_pi_4, &a, tol).is_none());
        assert!(
            crate::float::abs::<F>(grid_distance_at_corner(frac_pi_4, &a) - frac_pi_4)
                < lit::<F>(1e-5_f32)
        );
    }

    fn assert_unaligned_edge_is_spurious<F: Float>() {
        let pi = F::pi();
        let frac_pi_2 = pi / lit::<F>(2.0_f32);
        let tol = lit::<F>(15.0_f32.to_radians());
        let a = axes::<F>(0.0, frac_pi_2.to_subset_unchecked());
        // 22° from horizontal axis: outside the 15° grid tolerance.
        let twenty_two = lit::<F>(22.0_f32.to_radians());
        assert!(grid_match_at_corner(twenty_two, &a, tol).is_none());
    }

    #[test]
    fn axis_diff_is_symmetric_modulo_pi_f32() {
        assert_axis_diff_is_symmetric_modulo_pi::<f32>();
    }
    #[test]
    fn axis_diff_is_symmetric_modulo_pi_f64() {
        assert_axis_diff_is_symmetric_modulo_pi::<f64>();
    }
    #[test]
    fn axis_aligned_edge_is_grid_f32() {
        assert_axis_aligned_edge_is_grid::<f32>();
    }
    #[test]
    fn axis_aligned_edge_is_grid_f64() {
        assert_axis_aligned_edge_is_grid::<f64>();
    }
    #[test]
    fn diagonal_angle_is_not_grid_f32() {
        assert_diagonal_angle_is_not_grid::<f32>();
    }
    #[test]
    fn diagonal_angle_is_not_grid_f64() {
        assert_diagonal_angle_is_not_grid::<f64>();
    }
    #[test]
    fn unaligned_edge_is_spurious_f32() {
        assert_unaligned_edge_is_spurious::<f32>();
    }
    #[test]
    fn unaligned_edge_is_spurious_f64() {
        assert_unaligned_edge_is_spurious::<f64>();
    }
}
