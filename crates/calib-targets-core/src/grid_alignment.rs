//! Grid-alignment compatibility shim.
//!
//! Downstream crates (chessboard, charuco, puzzleboard, marker, …) construct
//! [`GridCoords`], [`GridTransform`], and [`GridAlignment`] with struct-literal
//! syntax, index [`GRID_TRANSFORMS_D4`] by position (`[1]` is the legacy
//! "(j, −i)" rotation, NOT the new crate's 90° CCW entry), and freely cross
//! these values between this crate and the legacy `projective-grid` crate's
//! free functions (e.g. `square_predict_grid_position`).
//!
//! To keep that contract during the `projective-grid → projective-grid-next`
//! migration window (Phase 6a–6e), this module re-exports the legacy types
//! directly so type identity is preserved across crate boundaries. It also
//! adds conversion impls between the legacy types and the new crate's
//! [`projective_grid_next::Coord`] / [`projective_grid_next::GridTransform`]
//! so internal bridges that have already migrated can interoperate.
//!
//! Phase 8 deletes the legacy `projective-grid` crate and the re-exports
//! switch to point at the renamed-back `projective-grid` (formerly
//! `-next`); this module is the single edit site for that flip.

use projective_grid_next::lattice::LatticeKind;
use projective_grid_next::GridTransform as NextGridTransform;

pub use projective_grid::{GridAlignment, GridCoords, GridTransform, GRID_TRANSFORMS_D4};

// ---- Conversions to / from projective-grid-next ----
//
// `projective_grid_next::Coord` is `(i32, i32)` (a type alias), so
// `GridCoords` already converts to / from it via the legacy crate's
// `From<(i32, i32)> for GridCoords` / `From<GridCoords> for (i32, i32)`
// impls — no shim impls needed there.
//
// The 2×2 + offset conversion for `GridTransform` / `GridAlignment` IS new
// (the legacy crate had no awareness of the new crate). The legacy
// `GridTransform { a, b, c, d }` maps to a square-lattice matrix with zero
// offset; `GridAlignment` carries the offset on its `translation` slot.

/// Convert the legacy `GridTransform` into a tagged square-lattice transform
/// for [`projective_grid_next`] consumers.
///
/// Implemented as a free function (not an `impl From`) because both
/// [`GridTransform`] and [`NextGridTransform`] live in foreign crates from
/// this module's POV — the orphan rules forbid `impl From<Foreign> for
/// Foreign`.
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

/// Convert the legacy `GridAlignment` into a single tagged square-lattice
/// transform (the new crate folds linear + translation into one struct).
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_d4_index_1_is_j_negative_i() {
        // Sanity check: the legacy D4 table indexes the (j, −i) rotation at
        // [1]. The new crate's `D4_TRANSFORMS[1]` puts (−j, i) there
        // instead, so re-exporting from the legacy crate is essential for
        // downstream consumers (decoder, charuco, marker) that index by
        // integer constant.
        let t = GRID_TRANSFORMS_D4[1];
        assert_eq!(t.apply(1, 0), GridCoords { i: 0, j: -1 });
        assert_eq!(t.apply(0, 1), GridCoords { i: 1, j: 0 });
    }

    #[test]
    fn legacy_transform_round_trips_through_next() {
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
}
