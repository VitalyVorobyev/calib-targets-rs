//! Integer grid coordinates and square-lattice alignment helpers.
//!
//! This crate owns the shared `(i, j)` vocabulary used by target detectors.
//! Legacy `projective-grid` algorithms are still used by the chessboard
//! implementation during the migration, but crossing that boundary must happen
//! through explicit private adapters in the consuming crate.

use projective_grid::lattice::LatticeKind;
use projective_grid::Coord as NextCoord;
use projective_grid::GridTransform as NextGridTransform;
use serde::{Deserialize, Serialize};

/// Integer grid coordinates `(i, j)` identifying a corner intersection in a
/// 2D square grid.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct GridCoords {
    /// Column index — increases along the grid's first axis (`i` right).
    pub i: i32,
    /// Row index — increases along the grid's second axis (`j` down).
    pub j: i32,
}

impl From<(i32, i32)> for GridCoords {
    #[inline]
    fn from((i, j): (i32, i32)) -> Self {
        Self { i, j }
    }
}

impl From<GridCoords> for (i32, i32) {
    #[inline]
    fn from(g: GridCoords) -> Self {
        (g.i, g.j)
    }
}

/// Integer 2D grid transform (a 2×2 matrix) for aligning detected grids to a
/// board model.
///
/// Represents a linear transform on integer coordinates:
/// `(i', j') = (a*i + b*j, c*i + d*j)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridTransform {
    /// Row-0, column-0 entry — the `i`-contribution to the new `i`.
    pub a: i32,
    /// Row-0, column-1 entry — the `j`-contribution to the new `i`.
    pub b: i32,
    /// Row-1, column-0 entry — the `i`-contribution to the new `j`.
    pub c: i32,
    /// Row-1, column-1 entry — the `j`-contribution to the new `j`.
    pub d: i32,
}

impl GridTransform {
    /// The identity transform — leaves `(i, j)` unchanged.
    pub const IDENTITY: GridTransform = GridTransform {
        a: 1,
        b: 0,
        c: 0,
        d: 1,
    };

    /// Apply the transform to `(i, j)`, returning the result as
    /// [`GridCoords`].
    #[inline]
    pub fn apply(&self, i: i32, j: i32) -> GridCoords {
        GridCoords {
            i: self.a * i + self.b * j,
            j: self.c * i + self.d * j,
        }
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
/// `(gc.i * px_per_cell, gc.j * px_per_cell)`; the cell is `px_per_cell`
/// pixels on a side. Pass `GridCoords { i: 0, j: 0 }` for the origin cell.
///
/// Shared by the ArUco rectified-cell scan and the ChArUco board-match
/// sampler so the cell-corner order is defined in exactly one place.
#[inline]
pub fn cell_rect_corners_at(gc: GridCoords, px_per_cell: f32) -> [nalgebra::Point2<f32>; 4] {
    let x0 = gc.i as f32 * px_per_cell;
    let y0 = gc.j as f32 * px_per_cell;
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
    /// Integer `[Δi, Δj]` offset added after `transform`.
    pub translation: [i32; 2],
}

impl GridAlignment {
    /// The identity alignment — identity transform with zero translation.
    pub const IDENTITY: GridAlignment = GridAlignment {
        transform: GridTransform::IDENTITY,
        translation: [0, 0],
    };

    /// Map grid coordinates `(i, j)` using this alignment, returning
    /// [`GridCoords`].
    #[inline]
    pub fn map(&self, i: i32, j: i32) -> GridCoords {
        let g = self.transform.apply(i, j);
        GridCoords {
            i: g.i + self.translation[0],
            j: g.j + self.translation[1],
        }
    }

    /// Invert the alignment if its linear part is unimodular (det = ±1).
    pub fn inverse(&self) -> Option<GridAlignment> {
        let inv = self.transform.inverse()?;
        let [tx, ty] = self.translation;
        let g = inv.apply(-tx, -ty);
        Some(GridAlignment {
            transform: inv,
            translation: [g.i, g.j],
        })
    }
}

/// The 8 dihedral transforms `D4` on the integer grid.
///
/// The order intentionally matches the historical `projective-grid` table:
/// index `1` is `(i, j) -> (j, -i)`.
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
///
/// Callers that need offset preservation should convert through
/// [`grid_alignment_from_next`] instead.
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

/// Convert a [`GridAlignment`] into a single tagged square-lattice transform
/// (the new crate folds linear + translation into one struct).
#[inline]
pub fn grid_alignment_to_next(a: GridAlignment) -> NextGridTransform {
    NextGridTransform::new(
        LatticeKind::Square,
        [
            [a.transform.a, a.transform.b],
            [a.transform.c, a.transform.d],
        ],
        a.translation,
    )
}

/// Project a [`NextGridTransform`] back to the legacy two-part shape
/// (`{ transform, translation }`).
#[inline]
pub fn grid_alignment_from_next(t: NextGridTransform) -> GridAlignment {
    let m = t.matrix;
    GridAlignment {
        transform: GridTransform {
            a: m[0][0],
            b: m[0][1],
            c: m[1][0],
            d: m[1][1],
        },
        translation: t.offset,
    }
}

/// Convert [`GridCoords`] into the lattice-agnostic
/// [`projective_grid::Coord`]. The mapping is `i → u`, `j → v`.
#[inline]
pub fn grid_coords_to_next(c: GridCoords) -> NextCoord {
    NextCoord::new(c.i, c.j)
}

/// Project a [`projective_grid::Coord`] back into [`GridCoords`]
/// (`u → i`, `v → j`).
#[inline]
pub fn grid_coords_from_next(c: NextCoord) -> GridCoords {
    GridCoords { i: c.u, j: c.v }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn d4_index_1_is_j_negative_i() {
        let t = GRID_TRANSFORMS_D4[1];
        assert_eq!(t.apply(1, 0), GridCoords { i: 0, j: -1 });
        assert_eq!(t.apply(0, 1), GridCoords { i: 1, j: 0 });
    }

    #[test]
    fn transform_identity_mapping_and_inverse() {
        let identity = GridTransform::IDENTITY;
        assert_eq!(identity.apply(7, -3), GridCoords { i: 7, j: -3 });
        assert_eq!(identity.inverse(), Some(identity));

        for t in GRID_TRANSFORMS_D4 {
            let inv = t.inverse().expect("D4 transform is unimodular");
            let p = GridCoords { i: 4, j: -9 };
            let q = t.apply(p.i, p.j);
            assert_eq!(inv.apply(q.i, q.j), p);
        }
    }

    #[test]
    fn alignment_mapping_and_inverse() {
        let align = GridAlignment {
            transform: GRID_TRANSFORMS_D4[1],
            translation: [3, -4],
        };
        let p = GridCoords { i: 2, j: 5 };
        let q = align.map(p.i, p.j);
        assert_eq!(q, GridCoords { i: 8, j: -6 });
        let inv = align.inverse().expect("D4 alignment is invertible");
        assert_eq!(inv.map(q.i, q.j), p);
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

    #[test]
    fn legacy_alignment_round_trips_through_next() {
        let align = GridAlignment {
            transform: GRID_TRANSFORMS_D4[6],
            translation: [-1, 2],
        };
        let next = grid_alignment_to_next(align);
        assert_eq!(grid_alignment_from_next(next), align);
    }

    #[test]
    fn grid_coords_bridges_pin_field_mapping() {
        // The mapping is load-bearing for every consumer that converts
        // labelled-grid data into the next crate: legacy `i` is square's
        // first axis ("right"), which is `Coord::u`; legacy `j` is the
        // second axis ("down"), which is `Coord::v`.
        let legacy = GridCoords { i: 3, j: -5 };
        let next = grid_coords_to_next(legacy);
        assert_eq!(next.u, 3);
        assert_eq!(next.v, -5);
        assert_eq!(grid_coords_from_next(next), legacy);

        // Asymmetric values catch any future i/j swap.
        for (i, j) in [(0, 0), (1, 0), (0, 1), (-7, 11), (i32::MAX, i32::MIN)] {
            let g = GridCoords { i, j };
            assert_eq!(grid_coords_from_next(grid_coords_to_next(g)), g);
        }
    }
}
