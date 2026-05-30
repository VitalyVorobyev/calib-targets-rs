//! Delaunator wrapper used by the axis-driven topological pipeline.
//!
//! Thin wrapper around the [`delaunator`] crate. The wrapper converts a slice
//! of `nalgebra::Point2<F>` (for `F: Float`) into `delaunator`'s `f64`
//! `Point` format and exposes the half-edge structure needed by the
//! topological pipeline. The conversion cost is negligible compared to
//! triangulation, and the `f64` internal type buys numerical robustness for
//! near-degenerate inputs.
//!
//! All items here are crate-internal; the `topological` module's
//! [`super::detect_square_oriented2_topological`] orchestrator owns the
//! public surface.

use nalgebra::Point2;

use crate::float::Float;

/// Result of triangulating a feature cloud.
pub(super) struct Triangulation {
    /// Flat list of triangle vertex indices: triangle `t` occupies
    /// `triangles[3t..3t+3]`. Length is always a multiple of 3.
    pub(super) triangles: Vec<usize>,
    /// Half-edge buddies. `halfedges[e]` is the matching half-edge in
    /// the neighbour triangle, or [`delaunator::EMPTY`] (== `usize::MAX`)
    /// if `e` is on the convex hull.
    pub(super) halfedges: Vec<usize>,
}

impl Triangulation {
    /// Number of triangles.
    #[inline]
    pub(super) fn num_tri(&self) -> usize {
        self.triangles.len() / 3
    }

    /// Convenience: half-edges of triangle `t` are at offsets `3t..3t+3`.
    /// `next_edge(e)` walks to the next half-edge inside the same triangle.
    #[inline]
    pub(super) fn next_edge(e: usize) -> usize {
        if e % 3 == 2 {
            e - 2
        } else {
            e + 1
        }
    }

    /// Triangle index containing half-edge `e`.
    #[inline]
    pub(super) fn tri_of(e: usize) -> usize {
        e / 3
    }
}

/// Convert `Point2<F>` to `delaunator::Point` via the
/// `nalgebra::convert_unchecked` route. Every `F: Float` is a superset of
/// `f64` so the conversion is total and infallible. For the two concrete
/// instantiations the workspace cares about (`f32` and `f64`) it compiles
/// to a single `as f64` cast.
#[inline]
fn point_to_f64<F: Float>(p: Point2<F>) -> delaunator::Point {
    delaunator::Point {
        x: nalgebra::convert_unchecked::<F, f64>(p.x),
        y: nalgebra::convert_unchecked::<F, f64>(p.y),
    }
}

#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        name = "delaunay_triangulate",
        level = "debug",
        skip_all,
        fields(num_points = positions.len()),
    )
)]
pub(super) fn triangulate<F: Float>(positions: &[Point2<F>]) -> Triangulation {
    let pts: Vec<delaunator::Point> = positions.iter().copied().map(point_to_f64::<F>).collect();
    let t = delaunator::triangulate(&pts);
    Triangulation {
        triangles: t.triangles,
        halfedges: t.halfedges,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::float::lit;

    fn pt<F: Float>(x: f32, y: f32) -> Point2<F> {
        Point2::new(lit::<F>(x), lit::<F>(y))
    }

    fn assert_triangulates_square<F: Float>() {
        let positions = vec![
            pt::<F>(0.0, 0.0),
            pt::<F>(1.0, 0.0),
            pt::<F>(1.0, 1.0),
            pt::<F>(0.0, 1.0),
        ];
        let t = triangulate(&positions);
        assert_eq!(t.num_tri(), 2);
        for (e, &buddy) in t.halfedges.iter().enumerate() {
            if buddy != delaunator::EMPTY {
                assert_eq!(t.halfedges[buddy], e);
            }
        }
    }

    fn assert_next_edge_walks_triangle<F: Float>() {
        let _ = F::pi();
        assert_eq!(Triangulation::next_edge(0), 1);
        assert_eq!(Triangulation::next_edge(1), 2);
        assert_eq!(Triangulation::next_edge(2), 0);
        assert_eq!(Triangulation::next_edge(3), 4);
        assert_eq!(Triangulation::next_edge(5), 3);
        assert_eq!(Triangulation::tri_of(2), 0);
        assert_eq!(Triangulation::tri_of(5), 1);
    }

    #[test]
    fn triangulates_square_f32() {
        assert_triangulates_square::<f32>();
    }
    #[test]
    fn triangulates_square_f64() {
        assert_triangulates_square::<f64>();
    }
    #[test]
    fn next_edge_walks_triangle_f32() {
        assert_next_edge_walks_triangle::<f32>();
    }
    #[test]
    fn next_edge_walks_triangle_f64() {
        assert_next_edge_walks_triangle::<f64>();
    }
}
