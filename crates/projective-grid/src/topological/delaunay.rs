//! Thin wrapper around the [`delaunator`] crate.
//!
//! The wrapper converts `&[Point2<f32>]` into the f64 [`delaunator::Point`]
//! format and exposes the half-edge structure needed by the topological
//! pipeline. Conversion cost is negligible compared to triangulation.

use nalgebra::Point2;

/// Result of triangulating a corner cloud.
pub(crate) struct Triangulation {
    /// Flat list of triangle vertex indices: triangle `t` occupies
    /// `triangles[3t..3t+3]`. Length is always a multiple of 3.
    pub(crate) triangles: Vec<usize>,
    /// Half-edge buddies. `halfedges[e]` is the matching half-edge in
    /// the neighbour triangle, or `delaunator::EMPTY` (== `usize::MAX`)
    /// if `e` is on the convex hull.
    pub(crate) halfedges: Vec<usize>,
}

impl Triangulation {
    /// Number of triangles.
    #[inline]
    pub(crate) fn num_tri(&self) -> usize {
        self.triangles.len() / 3
    }

    /// Convenience: half-edges of triangle `t` are at offsets `3t..3t+3`.
    /// `next_edge(e)` walks to the next half-edge inside the same triangle.
    #[inline]
    pub(crate) fn next_edge(e: usize) -> usize {
        if e % 3 == 2 {
            e - 2
        } else {
            e + 1
        }
    }

    /// Triangle index containing half-edge `e`.
    #[inline]
    pub(crate) fn tri_of(e: usize) -> usize {
        e / 3
    }
}

pub(crate) fn triangulate(positions: &[Point2<f32>]) -> Triangulation {
    let pts: Vec<delaunator::Point> = positions
        .iter()
        .map(|p| delaunator::Point {
            x: p.x as f64,
            y: p.y as f64,
        })
        .collect();
    let t = delaunator::triangulate(&pts);
    Triangulation {
        triangles: t.triangles,
        halfedges: t.halfedges,
    }
}
