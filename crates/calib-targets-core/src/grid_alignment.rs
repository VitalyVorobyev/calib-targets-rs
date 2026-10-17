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

/// The corner alignment implied by a *cell* alignment on a square grid.
///
/// A cell is named by its top-left corner (as in [`cell_rect_corners_at`]),
/// so an alignment found by matching cells — `cell_alignment.apply(gc)` is
/// the destination cell that cell `gc` lands on — is not itself the alignment
/// of the corners. A rotation or reflection carries the four corners of cell
/// `gc` onto the four corners of the destination cell, but the source
/// top-left generally lands on a different one of them: under `M`, the
/// destination top-left is `M·gc + t + o`, where `t` is the corner
/// translation and `o` is the componentwise minimum of `M` over the unit
/// square's corners. The two translations therefore differ by `o`, which is
/// zero for the identity and nonzero for every other `D4` element (for
/// example `(0, -1)` for `GRID_TRANSFORMS_D4[1]`, `(-1, -1)` for `[2]`,
/// `(-1, 0)` for `[3]`).
///
/// Returns the alignment with the same linear part that maps every corner of
/// a source cell onto the corresponding corner of its destination cell.
pub fn cell_to_corner_alignment(cell_alignment: GridAlignment) -> GridAlignment {
    let [[a, b], [c, d]] = cell_alignment.matrix();
    let offset = [a.min(0) + b.min(0), c.min(0) + d.min(0)];
    let [tx, ty] = cell_alignment.translation();
    cell_alignment.with_translation([tx - offset[0], ty - offset[1]])
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

/// The 4 rotations `C4` on the integer grid: the orientation-preserving
/// (`determinant == +1`) subgroup of [`GRID_TRANSFORMS_D4`].
///
/// These are exactly the first four entries of [`GRID_TRANSFORMS_D4`], in the
/// same order, so a transform *index* means the same thing under either table
/// and the two can be swapped without renumbering anything.
///
/// # Physical meaning
///
/// A camera imaging the printed side of an opaque planar target can only
/// observe the board under one of these four transforms: a rigid pose plus a
/// perspective projection preserves handedness, so the grid-to-board
/// relabelling of a front view is always a rotation. Producing one of the four
/// reflections in [`GRID_TRANSFORMS_D4`] requires an optical path that flips
/// handedness — a mirror, a beam splitter — or an image that was mirrored
/// before detection. Detectors that search only this subgroup are therefore
/// both faster and strictly less prone to symmetry aliasing, because the four
/// physically unreachable hypotheses can no longer compete with the true one.
///
/// This is only sound for grids whose axes have already been pinned to the
/// image axes (`projective-grid` canonicalises every labelled grid so `+u`
/// points `+x` and `+v` points `+y` in pixels); without that step the observed
/// labelling can be an arbitrary element of `D4`.
pub const GRID_TRANSFORMS_C4: [GridTransform; 4] = [
    projective_grid::expert::lattice::D4_TRANSFORMS[0],
    projective_grid::expert::lattice::D4_TRANSFORMS[3],
    projective_grid::expert::lattice::D4_TRANSFORMS[2],
    projective_grid::expert::lattice::D4_TRANSFORMS[1],
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

    /// The structural fact that lets a caller treat `&GRID_TRANSFORMS_D4[..4]`
    /// and [`GRID_TRANSFORMS_C4`] as the same table: same entries, same order,
    /// so transform indices are interchangeable between the two searches.
    #[test]
    fn c4_is_the_first_four_of_d4() {
        assert_eq!(GRID_TRANSFORMS_C4.len(), 4);
        for (idx, (c4, d4)) in GRID_TRANSFORMS_C4
            .iter()
            .zip(GRID_TRANSFORMS_D4.iter())
            .enumerate()
        {
            assert_eq!(c4, d4, "C4[{idx}] must be D4[{idx}]");
        }
    }

    /// C4 is the orientation-preserving half and the D4 tail is the
    /// orientation-reversing half — the property that makes the split
    /// "rotations vs reflections" rather than an arbitrary partition.
    #[test]
    fn c4_entries_are_rotations_and_d4_tail_are_reflections() {
        for (idx, t) in GRID_TRANSFORMS_C4.iter().enumerate() {
            assert_eq!(t.determinant(), 1, "C4[{idx}] must preserve orientation");
        }
        for (offset, t) in GRID_TRANSFORMS_D4[4..].iter().enumerate() {
            let idx = offset + 4;
            assert_eq!(t.determinant(), -1, "D4[{idx}] must reverse orientation");
        }
    }

    /// C4 is a genuine subgroup, not just a det-filtered selection: composing
    /// any two of its elements lands back inside it. Composition is checked
    /// through [`GridTransform::apply`] on the two basis vectors, which pins
    /// the linear part exactly (all four have zero translation).
    #[test]
    fn c4_is_closed_under_composition() {
        let basis = [Coord::new(1, 0), Coord::new(0, 1)];
        for (i, a) in GRID_TRANSFORMS_C4.iter().enumerate() {
            for (j, b) in GRID_TRANSFORMS_C4.iter().enumerate() {
                let composed = basis.map(|p| a.apply(b.apply(p)));
                assert!(
                    GRID_TRANSFORMS_C4
                        .iter()
                        .any(|c| basis.map(|p| c.apply(p)) == composed),
                    "C4[{i}] ∘ C4[{j}] left the subgroup"
                );
            }
        }
    }

    /// For every `D4` element and translation, the corner alignment carries
    /// the four corners of each cell exactly onto the four corners of the
    /// cell the cell alignment names.
    #[test]
    fn cell_to_corner_alignment_maps_cell_corners_onto_destination_cell() {
        let unit = [(0, 0), (1, 0), (1, 1), (0, 1)];
        for (idx, t) in GRID_TRANSFORMS_D4.iter().enumerate() {
            for translation in [[0, 0], [3, -4], [-2, 5]] {
                let cell_align = t.with_translation(translation);
                let corner_align = cell_to_corner_alignment(cell_align);
                assert_eq!(corner_align.matrix(), cell_align.matrix());
                for gc in [Coord::new(0, 0), Coord::new(2, 1), Coord::new(-3, 4)] {
                    let dst = cell_align.apply(gc);
                    let mut want: Vec<_> = unit
                        .iter()
                        .map(|&(du, dv)| (dst.u + du, dst.v + dv))
                        .collect();
                    let mut got: Vec<_> = unit
                        .iter()
                        .map(|&(du, dv)| {
                            let p = corner_align.apply(Coord::new(gc.u + du, gc.v + dv));
                            (p.u, p.v)
                        })
                        .collect();
                    want.sort_unstable();
                    got.sort_unstable();
                    assert_eq!(got, want, "D4[{idx}], t={translation:?}, gc={gc:?}");
                }
            }
        }
    }

    /// The corner translation of each rotation, pinned so a change to the
    /// `D4` table order shows up here.
    #[test]
    fn cell_to_corner_alignment_offsets_of_the_rotations() {
        let shifted = |idx: usize| {
            cell_to_corner_alignment(GRID_TRANSFORMS_D4[idx].with_translation([3, -4]))
                .translation()
        };
        assert_eq!(shifted(0), [3, -4]);
        assert_eq!(shifted(1), [3, -3]);
        assert_eq!(shifted(2), [4, -3]);
        assert_eq!(shifted(3), [4, -4]);
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
