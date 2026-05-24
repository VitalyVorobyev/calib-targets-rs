//! Square lattice: D4 symmetry group + four cardinal neighbour offsets.
//!
//! The D4 transforms are ported verbatim from the legacy
//! `projective_grid::square::alignment::GRID_TRANSFORMS_D4` table — same eight
//! elements (4 rotations + 4 reflections), all about the origin, all
//! unimodular. The new representation wraps each transform with
//! [`LatticeKind::Square`] so the merger and consistency checker can fail
//! fast on cross-lattice misuse.

use super::{Coord, GridTransform, LatticeKind};

/// Four cardinal neighbour offsets on a square grid.
///
/// Order is `(+i, 0)`, `(0, +j)`, `(-i, 0)`, `(0, -j)` — the BFS prediction
/// step in the seed-and-grow engine iterates this slice when looking for the
/// next candidate.
pub const SQUARE_CARDINAL_OFFSETS: [Coord; 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

/// The eight transforms in the dihedral group D4 acting on integer square
/// lattice coordinates.
///
/// Indices 0..4 are pure rotations (0°, 90°, 180°, 270° CCW about the origin);
/// indices 4..8 are reflections (about the `j`-axis, the `i`-axis, the main
/// diagonal, the anti-diagonal). Order matches the legacy
/// `GRID_TRANSFORMS_D4` table for migration parity.
pub const D4_TRANSFORMS: [GridTransform; 8] = [
    // identity / 0°
    GridTransform::new(LatticeKind::Square, [[1, 0], [0, 1]], [0, 0]),
    // 90° CCW: (i, j) ↦ (-j, i)
    GridTransform::new(LatticeKind::Square, [[0, -1], [1, 0]], [0, 0]),
    // 180°: (i, j) ↦ (-i, -j)
    GridTransform::new(LatticeKind::Square, [[-1, 0], [0, -1]], [0, 0]),
    // 270° CCW: (i, j) ↦ (j, -i)
    GridTransform::new(LatticeKind::Square, [[0, 1], [-1, 0]], [0, 0]),
    // reflect across j-axis: (i, j) ↦ (-i, j)
    GridTransform::new(LatticeKind::Square, [[-1, 0], [0, 1]], [0, 0]),
    // reflect across i-axis: (i, j) ↦ (i, -j)
    GridTransform::new(LatticeKind::Square, [[1, 0], [0, -1]], [0, 0]),
    // reflect across main diagonal: (i, j) ↦ (j, i)
    GridTransform::new(LatticeKind::Square, [[0, 1], [1, 0]], [0, 0]),
    // reflect across anti-diagonal: (i, j) ↦ (-j, -i)
    GridTransform::new(LatticeKind::Square, [[0, -1], [-1, 0]], [0, 0]),
];
