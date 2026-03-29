//! Dihedral group D6 transforms for hexagonal grids in axial coordinates.
//!
//! The 12 symmetries of a regular hexagon: 6 rotations and 6 reflections.
//! All transforms are expressed as 2x2 integer matrices acting on axial `(q, r)`.
//!
//! Reuses [`GridTransform`] — same struct, different values.

use crate::grid_alignment::GridTransform;

/// The 12 dihedral transforms D6 on the hexagonal integer grid (axial coordinates).
///
/// Rotation generator (60° CW): `(q, r) → (-r, q + r)` = `[[0, -1], [1, 1]]`.
/// Reflection generator: `(q, r) → (q + r, -r)` = `[[1, 1], [0, -1]]`.
///
/// # Order
///
/// Indices 0..6 are rotations (0°, 60°, 120°, 180°, 240°, 300°).
/// Indices 6..12 are reflections (reflection composed with each rotation).
pub const GRID_TRANSFORMS_D6: [GridTransform; 12] = [
    // --- Rotations ---
    // 0°: identity
    GridTransform {
        a: 1,
        b: 0,
        c: 0,
        d: 1,
    },
    // 60°: (q, r) → (-r, q+r)
    GridTransform {
        a: 0,
        b: -1,
        c: 1,
        d: 1,
    },
    // 120°: (q, r) → (-q-r, q)
    GridTransform {
        a: -1,
        b: -1,
        c: 1,
        d: 0,
    },
    // 180°: (q, r) → (-q, -r)
    GridTransform {
        a: -1,
        b: 0,
        c: 0,
        d: -1,
    },
    // 240°: (q, r) → (r, -q-r)
    GridTransform {
        a: 0,
        b: 1,
        c: -1,
        d: -1,
    },
    // 300°: (q, r) → (q+r, -q)
    GridTransform {
        a: 1,
        b: 1,
        c: -1,
        d: 0,
    },
    // --- Reflections (reflection generator composed with rotations) ---
    // ref ∘ rot(0°): (q, r) → (q+r, -r)
    GridTransform {
        a: 1,
        b: 1,
        c: 0,
        d: -1,
    },
    // ref ∘ rot(60°): (q, r) → (-r, -q) ... apply ref to (-r, q+r) = (-r + q+r, -(q+r)) = (q, -q-r)
    GridTransform {
        a: 1,
        b: 0,
        c: -1,
        d: -1,
    },
    // ref ∘ rot(120°): apply ref to (-q-r, q) = (-q-r+q, -q) = (-r, -q)
    GridTransform {
        a: 0,
        b: -1,
        c: -1,
        d: 0,
    },
    // ref ∘ rot(180°): apply ref to (-q, -r) = (-q-r, r)
    GridTransform {
        a: -1,
        b: -1,
        c: 0,
        d: 1,
    },
    // ref ∘ rot(240°): apply ref to (r, -q-r) = (r-q-r, q+r) = (-q, q+r)
    GridTransform {
        a: -1,
        b: 0,
        c: 1,
        d: 1,
    },
    // ref ∘ rot(300°): apply ref to (q+r, -q) = (q+r-q, q) = (r, q)
    GridTransform {
        a: 0,
        b: 1,
        c: 1,
        d: 0,
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn compose(a: &GridTransform, b: &GridTransform) -> GridTransform {
        GridTransform {
            a: a.a * b.a + a.b * b.c,
            b: a.a * b.b + a.b * b.d,
            c: a.c * b.a + a.d * b.c,
            d: a.c * b.b + a.d * b.d,
        }
    }

    fn det(t: &GridTransform) -> i32 {
        t.a * t.d - t.b * t.c
    }

    fn as_tuple(t: &GridTransform) -> (i32, i32, i32, i32) {
        (t.a, t.b, t.c, t.d)
    }

    #[test]
    fn all_twelve_distinct() {
        let set: HashSet<_> = GRID_TRANSFORMS_D6.iter().map(as_tuple).collect();
        assert_eq!(set.len(), 12);
    }

    #[test]
    fn all_unimodular() {
        for t in &GRID_TRANSFORMS_D6 {
            let d = det(t);
            assert!(d == 1 || d == -1, "det = {d} for {t:?}");
        }
    }

    #[test]
    fn rotations_det_plus_one() {
        for t in &GRID_TRANSFORMS_D6[0..6] {
            assert_eq!(det(t), 1, "rotation {t:?} should have det +1");
        }
    }

    #[test]
    fn reflections_det_minus_one() {
        for t in &GRID_TRANSFORMS_D6[6..12] {
            assert_eq!(det(t), -1, "reflection {t:?} should have det -1");
        }
    }

    #[test]
    fn rotation_order_six() {
        let rot60 = &GRID_TRANSFORMS_D6[1];
        let identity = &GRID_TRANSFORMS_D6[0];

        let mut acc = *identity;
        for k in 1..=6 {
            acc = compose(&acc, rot60);
            if k < 6 {
                assert_ne!(
                    as_tuple(&acc),
                    as_tuple(identity),
                    "rot60^{k} should not be identity"
                );
            }
        }
        assert_eq!(
            as_tuple(&acc),
            as_tuple(identity),
            "rot60^6 must be identity"
        );
    }

    #[test]
    fn reflections_are_involutions() {
        for (i, t) in GRID_TRANSFORMS_D6[6..12].iter().enumerate() {
            let t_sq = compose(t, t);
            assert_eq!(
                as_tuple(&t_sq),
                as_tuple(&GRID_TRANSFORMS_D6[0]),
                "reflection[{i}]^2 must be identity"
            );
        }
    }

    #[test]
    fn closure_under_composition() {
        let set: HashSet<_> = GRID_TRANSFORMS_D6.iter().map(as_tuple).collect();
        for a in &GRID_TRANSFORMS_D6 {
            for b in &GRID_TRANSFORMS_D6 {
                let c = compose(a, b);
                assert!(
                    set.contains(&as_tuple(&c)),
                    "product of {a:?} and {b:?} = {c:?} not in D6"
                );
            }
        }
    }

    #[test]
    fn rotations_match_successive_composition() {
        let rot60 = &GRID_TRANSFORMS_D6[1];
        let identity = &GRID_TRANSFORMS_D6[0];
        let mut acc = *identity;
        for (k, expected) in GRID_TRANSFORMS_D6.iter().enumerate().take(6) {
            assert_eq!(
                as_tuple(&acc),
                as_tuple(expected),
                "rot60^{k} mismatch"
            );
            acc = compose(&acc, rot60);
        }
    }
}
