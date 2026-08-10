//! Integer square-lattice alignment helpers.
//!
//! The canonical integer grid-coordinate type is [`projective_grid::Coord`]
//! (`{ u, v }`), re-exported from this crate's facade. The canonical affine
//! grid transform is owned by `projective-grid` as well; this module retains
//! only square-target conveniences and rectified-cell geometry.

pub use projective_grid::expert::lattice::GridTransform;
use projective_grid::Coord;

/// Semantic alias for a transform that maps detected grid coordinates into a
/// board/model coordinate frame.
///
/// An alignment is not a second representation: its integer translation is
/// stored directly in the canonical [`GridTransform`].
pub type GridAlignment = GridTransform;

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

/// The 8 dihedral transforms `D4` on the integer grid.
///
/// The order intentionally matches the historical workspace table:
/// index `1` is `(u, v) -> (v, -u)`.
pub const GRID_TRANSFORMS_D4: [GridTransform; 8] = [
    projective_grid::expert::lattice::D4_TRANSFORMS[0],
    projective_grid::expert::lattice::D4_TRANSFORMS[3],
    projective_grid::expert::lattice::D4_TRANSFORMS[2],
    projective_grid::expert::lattice::D4_TRANSFORMS[1],
    projective_grid::expert::lattice::D4_TRANSFORMS[4],
    projective_grid::expert::lattice::D4_TRANSFORMS[5],
    projective_grid::expert::lattice::D4_TRANSFORMS[6],
    projective_grid::expert::lattice::D4_TRANSFORMS[7],
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn d4_index_1_is_v_negative_u() {
        let t = GRID_TRANSFORMS_D4[1];
        assert_eq!(t.apply(Coord::new(1, 0)), Coord::new(0, -1));
        assert_eq!(t.apply(Coord::new(0, 1)), Coord::new(1, 0));
    }

    #[test]
    fn transform_identity_mapping_and_inverse() {
        let identity = GridTransform::IDENTITY;
        assert_eq!(identity.apply(Coord::new(7, -3)), Coord::new(7, -3));
        assert_eq!(identity.inverse(), Some(identity));

        for t in GRID_TRANSFORMS_D4 {
            let inv = t.inverse().expect("D4 transform is unimodular");
            let p = Coord::new(4, -9);
            let q = t.apply(p);
            assert_eq!(inv.apply(q), p);
        }
    }

    #[test]
    fn alignment_mapping_and_inverse() {
        let align = GRID_TRANSFORMS_D4[1].with_translation([3, -4]);
        let p = Coord::new(2, 5);
        let q = align.apply(p);
        assert_eq!(q, Coord::new(8, -6));
        let inv = align.inverse().expect("D4 alignment is invertible");
        assert_eq!(inv.apply(q), p);
    }
}
