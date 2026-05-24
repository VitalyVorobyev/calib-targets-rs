//! Axis-driven grid-edge classification plus local triangle diagonal
//! inference (replaces SBF09's image-color sampling test).
//!
//! For a Delaunay half-edge from corner `a` to corner `b`, the edge angle
//! `θ = atan2(b - a)` is compared to each corner's two axes (modulo π,
//! since axes are undirected). If both endpoints see the edge within
//! `axis_align_tol_rad` of one informative axis, the edge is a **Grid**
//! edge.
//!
//! Diagonals are not classified by a fixed `axis ± π/4` angle. Under a
//! projective warp, a projected cell diagonal is induced by the local
//! projected grid-step vectors, not by the angle bisector in image space.
//! After the Grid/Spurious pass, each Delaunay triangle is inspected: if
//! exactly two of its edges are Grid edges and those two edges meet at a
//! vertex using different local axis slots, the remaining edge is
//! promoted to **Diagonal** for that triangle.
//!
//! The pre-filter in [`super::usable_mask`] guarantees both endpoints of
//! every classified edge have at least one informative axis; `Spurious`
//! here only flags genuine geometric misalignment, not uncertainty
//! rejection.

use nalgebra::Point2;

use super::axis::AxisCache;
use super::delaunay::Triangulation;
use crate::float::{lit, Float};

/// Per-edge classification result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EdgeClass {
    /// Both endpoints see the edge as aligned with one of their axes.
    Grid,
    /// The edge crosses a chessboard cell from one corner to the
    /// opposite corner — promoted by the per-triangle inference pass.
    Diagonal,
    /// Neither endpoint accepts the edge as a grid-axis match.
    Spurious,
}

/// Per-corner result of matching the half-edge against the two axes.
#[derive(Clone, Copy, Debug)]
struct GridAxisMatch<F: Float> {
    slot: usize,
    distance_rad: F,
}

/// Joint per-half-edge result of matching against both endpoints' axes.
#[derive(Clone, Copy, Debug)]
struct GridEdgeMatch {
    start_slot: usize,
    end_slot: usize,
}

/// Smallest unsigned angle between two undirected directions, in `[0, π/2]`.
///
/// Both `theta` and `alpha` are interpreted modulo π (axes are
/// undirected). The result is the geodesic distance on the half-circle.
#[inline]
fn axis_diff<F: Float>(theta: F, alpha: F) -> F {
    let pi = F::pi();
    let half_pi = pi / lit::<F>(2.0_f32);
    let mut d = (theta - alpha) % pi;
    if d < F::zero() {
        d += pi;
    }
    if d > half_pi {
        d = pi - d;
    }
    d
}

/// Nearest informative grid axis to `theta` at this corner.
fn nearest_axis_at_corner<F: Float>(theta: F, cache: &AxisCache<F>) -> Option<GridAxisMatch<F>> {
    let mut best: Option<GridAxisMatch<F>> = None;
    for slot in 0..2 {
        if !cache.informative[slot] {
            continue;
        }
        let angle = cache.angle_rad[slot];
        let d = axis_diff(theta, angle);
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

fn grid_match_at_corner<F: Float>(
    theta: F,
    cache: &AxisCache<F>,
    align_tol_rad: F,
) -> Option<GridAxisMatch<F>> {
    let best = nearest_axis_at_corner(theta, cache)?;
    (best.distance_rad < align_tol_rad).then_some(best)
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
            EdgeClass::Spurious => {
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
/// `axes_cache[global_idx]` carries the precomputed per-axis informative
/// flag for the feature. The pre-filter in [`super::usable_mask`]
/// guarantees at least one informative axis at every endpoint of every
/// triangulated edge.
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "debug",
        skip_all,
        fields(num_edges = triangulation.triangles.len()),
    )
)]
pub(super) fn classify_all_edges<F: Float>(
    positions: &[Point2<F>],
    axes_cache: &[AxisCache<F>],
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
        let at_a = grid_match_at_corner(theta, &axes_cache[a], align_tol_rad);
        let at_b = grid_match_at_corner(theta, &axes_cache[b], align_tol_rad);
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

    fn cache<F: Float>(angle0: f32, angle1: f32) -> AxisCache<F> {
        AxisCache {
            angle_rad: [lit::<F>(angle0), lit::<F>(angle1)],
            informative: [true, true],
        }
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
        let cache = cache::<F>(0.0, frac_pi_2.to_subset_unchecked());
        let horizontal = grid_match_at_corner(F::zero(), &cache, tol).unwrap();
        assert_eq!(horizontal.slot, 0);
        assert!(crate::float::abs::<F>(horizontal.distance_rad) < lit::<F>(1e-5_f32));
        let vertical = grid_match_at_corner(frac_pi_2, &cache, tol).unwrap();
        assert_eq!(vertical.slot, 1);
        assert!(crate::float::abs::<F>(vertical.distance_rad) < lit::<F>(1e-5_f32));
    }

    fn assert_diagonal_angle_is_not_grid<F: Float>() {
        let pi = F::pi();
        let frac_pi_2 = pi / lit::<F>(2.0_f32);
        let frac_pi_4 = pi / lit::<F>(4.0_f32);
        let tol = lit::<F>(15.0_f32.to_radians());
        let cache = cache::<F>(0.0, frac_pi_2.to_subset_unchecked());
        assert!(grid_match_at_corner(frac_pi_4, &cache, tol).is_none());
    }

    fn assert_unaligned_edge_is_spurious<F: Float>() {
        let pi = F::pi();
        let frac_pi_2 = pi / lit::<F>(2.0_f32);
        let tol = lit::<F>(15.0_f32.to_radians());
        let cache = cache::<F>(0.0, frac_pi_2.to_subset_unchecked());
        let twenty_two = lit::<F>(22.0_f32.to_radians());
        assert!(grid_match_at_corner(twenty_two, &cache, tol).is_none());
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
