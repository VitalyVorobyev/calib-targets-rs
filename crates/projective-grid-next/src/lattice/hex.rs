//! Hex lattice: D6 symmetry group + six cardinal neighbour offsets in axial
//! coordinates.
//!
//! The D6 transforms are ported verbatim from the legacy
//! `projective_grid::hex::alignment::GRID_TRANSFORMS_D6` table — twelve
//! elements (6 rotations + 6 reflections) acting on `(q, r)` axial
//! coordinates. The new representation wraps each with [`LatticeKind::Hex`]
//! so the merger and consistency checker fail fast on cross-lattice misuse.
//!
//! Rotation generator (60° CW): `(q, r) ↦ (-r, q + r)` ≡ `[[0, -1], [1, 1]]`.
//! Reflection generator: `(q, r) ↦ (q + r, -r)` ≡ `[[1, 1], [0, -1]]`.

use super::{Coord, GridTransform, LatticeKind};

/// Six cardinal neighbour offsets on a hex grid in axial coordinates.
///
/// Order is `+q`, `+q -r`, `-r`, `-q`, `-q +r`, `+r`, completing the ring of
/// neighbours around `(0, 0)` once counter-clockwise.
pub const HEX_AXIAL_OFFSETS: [Coord; 6] = [(1, 0), (1, -1), (0, -1), (-1, 0), (-1, 1), (0, 1)];

/// The twelve transforms in the dihedral group D6 acting on integer hex
/// (axial) coordinates.
///
/// Indices 0..6 are rotations (0°, 60°, 120°, 180°, 240°, 300°);
/// indices 6..12 are reflections (the reflection generator composed with
/// each rotation). Order matches the legacy `GRID_TRANSFORMS_D6` table for
/// migration parity.
pub const D6_TRANSFORMS: [GridTransform; 12] = [
    // --- Rotations ---
    // 0°: identity
    GridTransform::new(LatticeKind::Hex, [[1, 0], [0, 1]], [0, 0]),
    // 60°: (q, r) ↦ (-r, q+r)
    GridTransform::new(LatticeKind::Hex, [[0, -1], [1, 1]], [0, 0]),
    // 120°: (q, r) ↦ (-q-r, q)
    GridTransform::new(LatticeKind::Hex, [[-1, -1], [1, 0]], [0, 0]),
    // 180°: (q, r) ↦ (-q, -r)
    GridTransform::new(LatticeKind::Hex, [[-1, 0], [0, -1]], [0, 0]),
    // 240°: (q, r) ↦ (r, -q-r)
    GridTransform::new(LatticeKind::Hex, [[0, 1], [-1, -1]], [0, 0]),
    // 300°: (q, r) ↦ (q+r, -q)
    GridTransform::new(LatticeKind::Hex, [[1, 1], [-1, 0]], [0, 0]),
    // --- Reflections (reflection generator composed with rotations) ---
    // ref ∘ rot(0°):   (q, r) ↦ (q+r, -r)
    GridTransform::new(LatticeKind::Hex, [[1, 1], [0, -1]], [0, 0]),
    // ref ∘ rot(60°):  (q, r) ↦ (q, -q-r)
    GridTransform::new(LatticeKind::Hex, [[1, 0], [-1, -1]], [0, 0]),
    // ref ∘ rot(120°): (q, r) ↦ (-r, -q)
    GridTransform::new(LatticeKind::Hex, [[0, -1], [-1, 0]], [0, 0]),
    // ref ∘ rot(180°): (q, r) ↦ (-q-r, r)
    GridTransform::new(LatticeKind::Hex, [[-1, -1], [0, 1]], [0, 0]),
    // ref ∘ rot(240°): (q, r) ↦ (-q, q+r)
    GridTransform::new(LatticeKind::Hex, [[-1, 0], [1, 1]], [0, 0]),
    // ref ∘ rot(300°): (q, r) ↦ (r, q)
    GridTransform::new(LatticeKind::Hex, [[0, 1], [1, 0]], [0, 0]),
];
