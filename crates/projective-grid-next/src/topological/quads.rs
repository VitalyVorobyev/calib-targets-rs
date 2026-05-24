//! Triangle-pair to quad merge: assemble candidate quads from classified
//! half-edges.
//!
//! For each Delaunay triangle, find its unique "diagonal" half-edge (the
//! edge that crosses a chessboard cell). The buddy of that half-edge in
//! the neighbour triangle is the same diagonal seen from the other side.
//! Merging the two triangles by removing the shared diagonal yields a
//! quadrilateral whose four perimeter edges are all *grid* edges — i.e.
//! one chessboard cell.
//!
//! Triangles that have zero or more than one diagonal edge are skipped:
//! they cannot be paired unambiguously. This is consistent with the
//! paper's topological-test-first principle (geometric tests come later).

use nalgebra::Point2;

use super::delaunay::Triangulation;
use crate::diagnostics::events::EdgeClass;
use crate::float::{lit, Float};
use crate::lattice::Coord;

/// Public view of a merged quad considered by the topological pipeline.
///
/// Passed to [`super::TopologicalContext::quad_label_ok`] so a pattern-aware
/// consumer can reject quads whose label-parity assignments are inconsistent
/// with the corner tags. Phase 3 ships the type and the hook plumbing; the
/// chessboard consumer wires the parity check in Phase 5.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct QuadView<'a, F: Float> {
    /// Indices into the input observation slice.
    pub vertices: [usize; 4],
    /// Positions of the four vertices, in TL/TR/BR/BL clockwise order.
    pub positions: [Point2<F>; 4],
    /// Proposed lattice labels for the four vertices, in the same order as
    /// `vertices`. Computed by the walker just before the policy hook is
    /// invoked.
    pub coords: [Coord; 4],
    /// Lifetime tie-in to the underlying observation slice; the view itself
    /// stores copies of the relevant data.
    pub(crate) _marker: std::marker::PhantomData<&'a ()>,
}

/// One merged quad.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Quad {
    /// The four corner indices in clockwise order around the quad's
    /// centroid (image y-down). The starting vertex is the geometrically
    /// top-left one (smallest `y`, ties broken by smallest `x`).
    pub vertices: [usize; 4],
}

impl Quad {
    /// Iterate the four perimeter edges as ordered `(u, v)` pairs (CW).
    pub(crate) fn perimeter_edges(&self) -> [(usize, usize); 4] {
        [
            (self.vertices[0], self.vertices[1]),
            (self.vertices[1], self.vertices[2]),
            (self.vertices[2], self.vertices[3]),
            (self.vertices[3], self.vertices[0]),
        ]
    }
}

/// Find the index `k ∈ {0, 1, 2}` of the unique diagonal half-edge in
/// triangle `t`, or `None` if the triangle has 0 or ≥ 2 diagonal edges,
/// or if any non-diagonal edge is not a grid edge.
fn unique_diagonal_edge(kinds: &[EdgeClass], t: usize) -> Option<usize> {
    let base = 3 * t;
    let mut diag_idx: Option<usize> = None;
    for k in 0..3 {
        let e = base + k;
        match kinds[e] {
            EdgeClass::Diagonal => {
                if diag_idx.is_some() {
                    return None; // > 1 diagonal — ambiguous.
                }
                diag_idx = Some(k);
            }
            EdgeClass::Grid => {} // Will be verified below.
            EdgeClass::Spurious | EdgeClass::Unknown => return None,
        }
    }
    let k = diag_idx?;
    // The other two edges must be Grid (they're the cell sides).
    for kk in 0..3 {
        if kk == k {
            continue;
        }
        if kinds[base + kk] != EdgeClass::Grid {
            return None;
        }
    }
    Some(k)
}

/// Build a quad from four distinct vertex indices, ordered CW around the
/// centroid starting from the geometrically top-left vertex.
fn build_quad<F: Float>(verts: [usize; 4], positions: &[Point2<F>]) -> Quad {
    let pts = verts.map(|v| positions[v]);
    let four = lit::<F>(4.0_f32);
    let cx = (pts[0].x + pts[1].x + pts[2].x + pts[3].x) / four;
    let cy = (pts[0].y + pts[1].y + pts[2].y + pts[3].y) / four;

    // Sort by angle from centroid. In image coords (y down), increasing
    // atan2(y - cy, x - cx) goes clockwise.
    let mut indexed: [(usize, F); 4] = [
        (verts[0], (pts[0].y - cy).atan2(pts[0].x - cx)),
        (verts[1], (pts[1].y - cy).atan2(pts[1].x - cx)),
        (verts[2], (pts[2].y - cy).atan2(pts[2].x - cx)),
        (verts[3], (pts[3].y - cy).atan2(pts[3].x - cx)),
    ];
    indexed.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    // Find geometrically top-left vertex among the 4: smallest y, ties
    // broken by smallest x. Rotate so it's at index 0.
    let mut tl_idx = 0usize;
    for k in 1..4 {
        let (vk, _) = indexed[k];
        let (v_tl, _) = indexed[tl_idx];
        let pk = positions[vk];
        let p_tl = positions[v_tl];
        if pk.y < p_tl.y || (pk.y == p_tl.y && pk.x < p_tl.x) {
            tl_idx = k;
        }
    }
    let mut out = [0usize; 4];
    for k in 0..4 {
        out[k] = indexed[(tl_idx + k) % 4].0;
    }
    Quad { vertices: out }
}

/// Walk the triangulation and emit one quad per matching pair of
/// triangles. Pairs are deduplicated (each diagonal edge is processed
/// from the side with the smaller triangle index).
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "debug",
        skip_all,
        fields(num_triangles = triangulation.num_tri()),
    )
)]
pub(crate) fn merge_triangle_pairs<F: Float>(
    triangulation: &Triangulation,
    kinds: &[EdgeClass],
    positions: &[Point2<F>],
) -> Vec<Quad> {
    let mut out = Vec::new();
    for t in 0..triangulation.num_tri() {
        let Some(k) = unique_diagonal_edge(kinds, t) else {
            continue;
        };
        let e = 3 * t + k;
        let e_buddy = triangulation.halfedges[e];
        if e_buddy == delaunator::EMPTY {
            continue; // Hull edge — no neighbour to pair with.
        }
        let t_other = Triangulation::tri_of(e_buddy);
        if t_other <= t {
            continue; // Already processed from the other side.
        }
        let Some(k_other) = unique_diagonal_edge(kinds, t_other) else {
            continue;
        };
        if 3 * t_other + k_other != e_buddy {
            // The other triangle's unique diagonal is not the buddy of
            // ours — pairing would be inconsistent.
            continue;
        }

        // Collect 4 distinct vertices: union of both triangles' vertex sets.
        let mut verts = [usize::MAX; 4];
        let mut count = 0;
        for &v in &triangulation.triangles[3 * t..3 * t + 3] {
            verts[count] = v;
            count += 1;
        }
        for &v in &triangulation.triangles[3 * t_other..3 * t_other + 3] {
            if !verts[..count].contains(&v) {
                if count >= 4 {
                    // Should never happen: triangle pair sharing one edge
                    // contributes exactly 4 distinct vertices.
                    break;
                }
                verts[count] = v;
                count += 1;
            }
        }
        if count != 4 {
            continue;
        }

        out.push(build_quad(verts, positions));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::float::lit;

    fn pt<F: Float>(x: f32, y: f32) -> Point2<F> {
        Point2::new(lit::<F>(x), lit::<F>(y))
    }

    fn assert_quad_ordering_is_cw_from_top_left<F: Float>() {
        // 4 corners of a unit square in image coords (y down).
        let positions = vec![
            pt::<F>(0.0, 0.0),
            pt::<F>(1.0, 0.0),
            pt::<F>(1.0, 1.0),
            pt::<F>(0.0, 1.0),
        ];
        // Pass them in scrambled order.
        let q = build_quad([2, 0, 3, 1], &positions);
        // Expect TL=(0,0), TR=(1,0), BR=(1,1), BL=(0,1) (CW in y-down).
        assert_eq!(q.vertices, [0, 1, 2, 3]);
    }

    #[test]
    fn quad_ordering_is_cw_from_top_left_f32() {
        assert_quad_ordering_is_cw_from_top_left::<f32>();
    }
    #[test]
    fn quad_ordering_is_cw_from_top_left_f64() {
        assert_quad_ordering_is_cw_from_top_left::<f64>();
    }
}
