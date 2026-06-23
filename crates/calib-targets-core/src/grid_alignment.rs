//! Integer square-lattice alignment helpers.
//!
//! The canonical integer grid-coordinate type is [`projective_grid::Coord`]
//! (`{ u, v }`), re-exported from this crate's facade. This module owns the
//! square-lattice *alignment* algebra (`GridTransform`, `GridAlignment`) that
//! maps board-model coordinates, expressed as [`Coord`]s, through a unimodular
//! linear part plus an integer translation. The axis convention is fixed:
//! `u` is the grid's first axis (right), `v` is the second axis (down).

use projective_grid::lattice::LatticeKind;
use projective_grid::Coord;
use projective_grid::GridTransform as NextGridTransform;
use serde::{Deserialize, Serialize};

/// Integer 2D grid transform (a 2×2 matrix) for aligning detected grids to a
/// board model.
///
/// Represents a linear transform on integer coordinates:
/// `(u', v') = (a*u + b*v, c*u + d*v)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridTransform {
    /// Row-0, column-0 entry — the `u`-contribution to the new `u`.
    pub a: i32,
    /// Row-0, column-1 entry — the `v`-contribution to the new `u`.
    pub b: i32,
    /// Row-1, column-0 entry — the `u`-contribution to the new `v`.
    pub c: i32,
    /// Row-1, column-1 entry — the `v`-contribution to the new `v`.
    pub d: i32,
}

impl GridTransform {
    /// The identity transform — leaves `(u, v)` unchanged.
    pub const IDENTITY: GridTransform = GridTransform {
        a: 1,
        b: 0,
        c: 0,
        d: 1,
    };

    /// Apply the transform to `(u, v)`, returning the result as a [`Coord`].
    #[inline]
    pub fn apply(&self, u: i32, v: i32) -> Coord {
        Coord::new(self.a * u + self.b * v, self.c * u + self.d * v)
    }

    /// Invert the transform if it is unimodular (det = ±1).
    pub fn inverse(&self) -> Option<GridTransform> {
        let det = self.a * self.d - self.b * self.c;
        if det != 1 && det != -1 {
            return None;
        }
        Some(GridTransform {
            a: self.d / det,
            b: -self.b / det,
            c: -self.c / det,
            d: self.a / det,
        })
    }
}

/// The four corners of a unit grid cell at `gc`, in canonical rectified
/// space, in clockwise **TL, TR, BR, BL** order (the workspace-wide
/// quad/homography order).
///
/// `gc` selects the cell whose top-left corner sits at
/// `(gc.u * px_per_cell, gc.v * px_per_cell)`; the cell is `px_per_cell`
/// pixels on a side. Pass `Coord::new(0, 0)` for the origin cell.
///
/// Shared by the ArUco rectified-cell scan and the ChArUco board-match
/// sampler so the cell-corner order is defined in exactly one place.
#[inline]
pub fn cell_rect_corners_at(gc: Coord, px_per_cell: f32) -> [nalgebra::Point2<f32>; 4] {
    let x0 = gc.u as f32 * px_per_cell;
    let y0 = gc.v as f32 * px_per_cell;
    let s = px_per_cell;
    [
        nalgebra::Point2::new(x0, y0),
        nalgebra::Point2::new(x0 + s, y0),
        nalgebra::Point2::new(x0 + s, y0 + s),
        nalgebra::Point2::new(x0, y0 + s),
    ]
}

/// A grid alignment: `dst = transform(src) + translation`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridAlignment {
    /// Linear part — applied to the source coordinates before translation.
    pub transform: GridTransform,
    /// Integer `[Δu, Δv]` offset added after `transform`.
    pub translation: [i32; 2],
}

impl GridAlignment {
    /// The identity alignment — identity transform with zero translation.
    pub const IDENTITY: GridAlignment = GridAlignment {
        transform: GridTransform::IDENTITY,
        translation: [0, 0],
    };

    /// Map grid coordinates `(u, v)` using this alignment, returning a
    /// [`Coord`].
    #[inline]
    pub fn map(&self, u: i32, v: i32) -> Coord {
        let g = self.transform.apply(u, v);
        Coord::new(g.u + self.translation[0], g.v + self.translation[1])
    }

    /// Invert the alignment if its linear part is unimodular (det = ±1).
    pub fn inverse(&self) -> Option<GridAlignment> {
        let inv = self.transform.inverse()?;
        let [tx, ty] = self.translation;
        let g = inv.apply(-tx, -ty);
        Some(GridAlignment {
            transform: inv,
            translation: [g.u, g.v],
        })
    }
}

/// The 8 dihedral transforms `D4` on the integer grid.
///
/// The order intentionally matches the historical `projective-grid` table:
/// index `1` is `(u, v) -> (v, -u)`.
pub const GRID_TRANSFORMS_D4: [GridTransform; 8] = [
    GridTransform {
        a: 1,
        b: 0,
        c: 0,
        d: 1,
    },
    GridTransform {
        a: 0,
        b: 1,
        c: -1,
        d: 0,
    },
    GridTransform {
        a: -1,
        b: 0,
        c: 0,
        d: -1,
    },
    GridTransform {
        a: 0,
        b: -1,
        c: 1,
        d: 0,
    },
    GridTransform {
        a: -1,
        b: 0,
        c: 0,
        d: 1,
    },
    GridTransform {
        a: 1,
        b: 0,
        c: 0,
        d: -1,
    },
    GridTransform {
        a: 0,
        b: 1,
        c: 1,
        d: 0,
    },
    GridTransform {
        a: 0,
        b: -1,
        c: -1,
        d: 0,
    },
];

/// Convert the legacy `GridTransform` into a tagged square-lattice transform
/// for [`projective_grid`] consumers.
#[inline]
pub fn grid_transform_to_next(t: GridTransform) -> NextGridTransform {
    NextGridTransform::new(LatticeKind::Square, [[t.a, t.b], [t.c, t.d]], [0, 0])
}

/// Project a [`NextGridTransform`] back to the legacy 2×2 shape, dropping
/// the lattice tag and any non-zero offset.
#[inline]
pub fn grid_transform_from_next(t: NextGridTransform) -> GridTransform {
    let m = t.matrix;
    GridTransform {
        a: m[0][0],
        b: m[0][1],
        c: m[1][0],
        d: m[1][1],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn d4_index_1_is_v_negative_u() {
        let t = GRID_TRANSFORMS_D4[1];
        assert_eq!(t.apply(1, 0), Coord::new(0, -1));
        assert_eq!(t.apply(0, 1), Coord::new(1, 0));
    }

    #[test]
    fn transform_identity_mapping_and_inverse() {
        let identity = GridTransform::IDENTITY;
        assert_eq!(identity.apply(7, -3), Coord::new(7, -3));
        assert_eq!(identity.inverse(), Some(identity));

        for t in GRID_TRANSFORMS_D4 {
            let inv = t.inverse().expect("D4 transform is unimodular");
            let p = Coord::new(4, -9);
            let q = t.apply(p.u, p.v);
            assert_eq!(inv.apply(q.u, q.v), p);
        }
    }

    #[test]
    fn alignment_mapping_and_inverse() {
        let align = GridAlignment {
            transform: GRID_TRANSFORMS_D4[1],
            translation: [3, -4],
        };
        let p = Coord::new(2, 5);
        let q = align.map(p.u, p.v);
        assert_eq!(q, Coord::new(8, -6));
        let inv = align.inverse().expect("D4 alignment is invertible");
        assert_eq!(inv.map(q.u, q.v), p);
    }

    #[test]
    fn transform_round_trips_through_next() {
        for (idx, t) in GRID_TRANSFORMS_D4.iter().enumerate() {
            let next = grid_transform_to_next(*t);
            assert_eq!(next.source_kind, LatticeKind::Square);
            assert_eq!(next.offset, [0, 0]);
            assert_eq!(grid_transform_from_next(next), *t, "D4[{idx}] round-trip");
        }
    }
}
