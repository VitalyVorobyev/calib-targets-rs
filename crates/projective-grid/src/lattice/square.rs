//! Square-lattice family: the only family with a complete detection pipeline
//! today.
//!
//! [`Square`] implements [`Lattice`] with a Cartesian `model_point`, the four
//! cardinal neighbour offsets, and the dihedral D4 symmetry group acting on
//! `(i, j)` coordinates.

use nalgebra::{Point2, Vector2};

use super::{CellTopology, Coord, GridTransform, Lattice, LatticeKind};

/// Zero-sized marker for the orthogonal square lattice.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Square;

impl Square {
    /// Construct the square-lattice marker.
    pub const fn new() -> Self {
        Self
    }
}

impl Lattice for Square {
    const KIND: LatticeKind = LatticeKind::Square;

    fn model_point(self, coord: Coord) -> Point2<f32> {
        Point2::new(coord.u as f32, coord.v as f32)
    }

    fn neighbour_offsets(self) -> &'static [Coord] {
        &SQUARE_CARDINAL_OFFSETS
    }

    fn symmetry_transforms(self) -> &'static [GridTransform] {
        &D4_TRANSFORMS
    }

    fn axis_family_count(self) -> usize {
        2
    }

    fn model_axis_directions(self) -> &'static [Vector2<f32>] {
        &SQUARE_AXIS_DIRECTIONS
    }

    fn cell_topology(self) -> CellTopology {
        CellTopology::TrianglePairToQuad
    }
}

/// Unit model-plane directions of the two square axis families: `+u` and `+v`.
static SQUARE_AXIS_DIRECTIONS: [Vector2<f32>; 2] = [Vector2::new(1.0, 0.0), Vector2::new(0.0, 1.0)];

/// Four cardinal neighbour offsets on a square grid.
pub const SQUARE_CARDINAL_OFFSETS: [Coord; 4] = [
    Coord::new(1, 0),
    Coord::new(0, 1),
    Coord::new(-1, 0),
    Coord::new(0, -1),
];

/// Dihedral group D4 acting on square lattice coordinates.
pub const D4_TRANSFORMS: [GridTransform; 8] = [
    GridTransform::new(LatticeKind::Square, [[1, 0], [0, 1]], [0, 0]),
    GridTransform::new(LatticeKind::Square, [[0, -1], [1, 0]], [0, 0]),
    GridTransform::new(LatticeKind::Square, [[-1, 0], [0, -1]], [0, 0]),
    GridTransform::new(LatticeKind::Square, [[0, 1], [-1, 0]], [0, 0]),
    GridTransform::new(LatticeKind::Square, [[-1, 0], [0, 1]], [0, 0]),
    GridTransform::new(LatticeKind::Square, [[1, 0], [0, -1]], [0, 0]),
    GridTransform::new(LatticeKind::Square, [[0, 1], [1, 0]], [0, 0]),
    GridTransform::new(LatticeKind::Square, [[0, -1], [-1, 0]], [0, 0]),
];
