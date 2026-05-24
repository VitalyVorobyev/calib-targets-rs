//! Lattice taxonomy: [`LatticeKind`], [`Coord`], [`GridTransform`], and the
//! D4 / D6 symmetry tables.
//!
//! ## Coord
//!
//! The integer label of a feature on a lattice. Always `(i32, i32)`,
//! regardless of lattice family — hex axial coords are also a pair of
//! integers, so a shared representation is safe. The *interpretation* differs
//! by [`LatticeKind`]:
//!
//! * `Square` — `(i, j)` are row/column indices.
//! * `Hex` — `(q, r)` are axial coordinates (Amit Patel convention).
//!
//! ## Lattice strategy
//!
//! [`LatticeKind`] is an enum at the public surface; algorithm internals never
//! branch on it. They take `cardinal_offsets: &[Coord]` and
//! `symmetry: &[GridTransform]` as slices. Public entry points are concrete
//! per lattice (`detect_square_grid`, `detect_hex_grid`). The runtime check
//! on [`GridTransform::source_kind`] catches accidental D4-on-hex (or vice
//! versa) at the merge / consistency boundary instead of silently producing
//! garbage labels.

pub mod hex;
pub mod square;

/// Integer label of a feature on a lattice.
///
/// `(i, j)` for square lattices; `(q, r)` axial for hex lattices. The shared
/// representation lets algorithm internals stay lattice-agnostic; the
/// interpretation lives in [`LatticeKind`].
pub type Coord = (i32, i32);

/// Tagged lattice family.
///
/// Public-surface enum for tasks and merge / consistency boundaries.
/// `#[non_exhaustive]` so future lattice families (e.g. triangular) can be
/// added without breaking match exhaustiveness in downstream code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LatticeKind {
    /// Square lattice. D4 symmetry, four cardinal neighbour offsets.
    Square,
    /// Hex lattice. D6 symmetry, six cardinal neighbour offsets in axial
    /// coordinates.
    Hex,
}

/// A symmetry transform on lattice coordinates: `out = matrix * coord + offset`.
///
/// `source_kind` tags which lattice family this transform belongs to so the
/// merger and consistency checker can fail fast at runtime when a caller
/// mixes D4 and D6 transforms. This is the "compile-time prevention of
/// D4-on-hex bugs" trade-off captured cheaply at runtime; compose / apply
/// are pure integer arithmetic with no Float involvement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct GridTransform {
    /// Lattice family this transform belongs to. Mixing transforms across
    /// kinds is a programmer error and is caught by [`Self::then`] / the
    /// merge runtime check.
    pub source_kind: LatticeKind,
    /// Row-major linear part: `out_i = matrix[0][0]*i + matrix[0][1]*j`,
    /// `out_j = matrix[1][0]*i + matrix[1][1]*j`.
    pub matrix: [[i32; 2]; 2],
    /// Integer offset added after the linear part.
    pub offset: [i32; 2],
}

impl GridTransform {
    /// Construct a transform from raw components. Same-kind validation is the
    /// caller's responsibility (the D4 / D6 tables in [`square`] / [`hex`]
    /// are the typical sources; ad-hoc transforms are rare).
    pub const fn new(source_kind: LatticeKind, matrix: [[i32; 2]; 2], offset: [i32; 2]) -> Self {
        Self {
            source_kind,
            matrix,
            offset,
        }
    }

    /// Apply the transform to a coordinate: `out = matrix * coord + offset`.
    #[inline]
    pub fn apply(&self, coord: Coord) -> Coord {
        let (i, j) = coord;
        let ni = self.matrix[0][0] * i + self.matrix[0][1] * j + self.offset[0];
        let nj = self.matrix[1][0] * i + self.matrix[1][1] * j + self.offset[1];
        (ni, nj)
    }

    /// Compose two transforms: `self.then(other)` is the map
    /// `coord ↦ other(self(coord))`. Returns `None` when the two transforms
    /// belong to different lattice kinds.
    pub fn then(&self, other: &Self) -> Option<Self> {
        if self.source_kind != other.source_kind {
            return None;
        }
        let a = &other.matrix;
        let b = &self.matrix;
        // (A ∘ B)(x) = A(B x + b_off) + a_off = (A B) x + (A b_off + a_off).
        let matrix = [
            [
                a[0][0] * b[0][0] + a[0][1] * b[1][0],
                a[0][0] * b[0][1] + a[0][1] * b[1][1],
            ],
            [
                a[1][0] * b[0][0] + a[1][1] * b[1][0],
                a[1][0] * b[0][1] + a[1][1] * b[1][1],
            ],
        ];
        let offset = [
            a[0][0] * self.offset[0] + a[0][1] * self.offset[1] + other.offset[0],
            a[1][0] * self.offset[0] + a[1][1] * self.offset[1] + other.offset[1],
        ];
        Some(Self {
            source_kind: self.source_kind,
            matrix,
            offset,
        })
    }

    /// Determinant of the linear part. `±1` for proper unimodular transforms
    /// (the entire D4 / D6 tables are unimodular).
    #[inline]
    pub fn determinant(&self) -> i32 {
        self.matrix[0][0] * self.matrix[1][1] - self.matrix[0][1] * self.matrix[1][0]
    }
}

pub use hex::{D6_TRANSFORMS, HEX_AXIAL_OFFSETS};
pub use square::{D4_TRANSFORMS, SQUARE_CARDINAL_OFFSETS};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_identity_is_noop() {
        let id = GridTransform::new(LatticeKind::Square, [[1, 0], [0, 1]], [0, 0]);
        assert_eq!(id.apply((3, -2)), (3, -2));
    }

    #[test]
    fn apply_rot90_square() {
        // 90° CCW: (i, j) → (-j, i)
        let rot = GridTransform::new(LatticeKind::Square, [[0, -1], [1, 0]], [0, 0]);
        assert_eq!(rot.apply((1, 0)), (0, 1));
        assert_eq!(rot.apply((0, 1)), (-1, 0));
    }

    #[test]
    fn then_compose_square_chain() {
        // Two 90° rotations equals 180°.
        let rot90 = GridTransform::new(LatticeKind::Square, [[0, -1], [1, 0]], [0, 0]);
        let composed = rot90.then(&rot90).expect("same kind composes");
        assert_eq!(composed.apply((1, 0)), (-1, 0));
        assert_eq!(composed.apply((0, 1)), (0, -1));
    }

    #[test]
    fn then_compose_with_offset() {
        // shift-then-rot90: first add (1, 0), then rotate 90° CCW.
        let shift = GridTransform::new(LatticeKind::Square, [[1, 0], [0, 1]], [1, 0]);
        let rot = GridTransform::new(LatticeKind::Square, [[0, -1], [1, 0]], [0, 0]);
        let chained = shift.then(&rot).unwrap();
        // shift((0,0)) = (1, 0); rot((1,0)) = (0, 1)
        assert_eq!(chained.apply((0, 0)), (0, 1));
        // shift((2,3)) = (3, 3); rot((3,3)) = (-3, 3)
        assert_eq!(chained.apply((2, 3)), (-3, 3));
    }

    #[test]
    fn then_rejects_mixed_kinds() {
        let sq = GridTransform::new(LatticeKind::Square, [[1, 0], [0, 1]], [0, 0]);
        let hx = GridTransform::new(LatticeKind::Hex, [[1, 0], [0, 1]], [0, 0]);
        assert!(sq.then(&hx).is_none());
        assert!(hx.then(&sq).is_none());
    }

    #[test]
    fn d4_table_is_complete_and_distinct() {
        use std::collections::HashSet;
        let set: HashSet<_> = D4_TRANSFORMS.iter().map(|t| (t.matrix, t.offset)).collect();
        assert_eq!(set.len(), 8);
        for t in &D4_TRANSFORMS {
            assert_eq!(t.source_kind, LatticeKind::Square);
            let d = t.determinant();
            assert!(d == 1 || d == -1, "non-unimodular D4 transform {t:?}");
        }
    }

    #[test]
    fn d6_table_is_complete_and_distinct() {
        use std::collections::HashSet;
        let set: HashSet<_> = D6_TRANSFORMS.iter().map(|t| (t.matrix, t.offset)).collect();
        assert_eq!(set.len(), 12);
        for t in &D6_TRANSFORMS {
            assert_eq!(t.source_kind, LatticeKind::Hex);
            let d = t.determinant();
            assert!(d == 1 || d == -1, "non-unimodular D6 transform {t:?}");
        }
    }

    #[test]
    fn d4_closed_under_composition() {
        use std::collections::HashSet;
        let set: HashSet<_> = D4_TRANSFORMS.iter().map(|t| (t.matrix, t.offset)).collect();
        for a in &D4_TRANSFORMS {
            for b in &D4_TRANSFORMS {
                let c = a.then(b).unwrap();
                assert!(
                    set.contains(&(c.matrix, c.offset)),
                    "D4 composition {a:?} ∘ {b:?} = {c:?} escaped table"
                );
            }
        }
    }

    #[test]
    fn d6_closed_under_composition() {
        use std::collections::HashSet;
        let set: HashSet<_> = D6_TRANSFORMS.iter().map(|t| (t.matrix, t.offset)).collect();
        for a in &D6_TRANSFORMS {
            for b in &D6_TRANSFORMS {
                let c = a.then(b).unwrap();
                assert!(
                    set.contains(&(c.matrix, c.offset)),
                    "D6 composition {a:?} ∘ {b:?} = {c:?} escaped table"
                );
            }
        }
    }

    #[test]
    fn cardinal_offsets_count_matches_lattice() {
        assert_eq!(SQUARE_CARDINAL_OFFSETS.len(), 4);
        assert_eq!(HEX_AXIAL_OFFSETS.len(), 6);
    }
}
