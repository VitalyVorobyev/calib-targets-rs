//! Hex-lattice family: roadmap stub.
//!
//! [`Hex`] implements [`Lattice`] with the axial `model_point`, the six axial
//! neighbour offsets, and the dihedral D6 symmetry group. The lattice geometry
//! is complete; what is *not* yet wired is hex grid **detection** — the
//! strategy skeletons are square-concrete today. Per `docs/DESIGN.md`
//! ("Extending to hex"), adding hex recovery is a fill-in-the-trait task:
//! implement the lattice-specific arms of each strategy against this trait
//! (hexagon assembly for the topological builder) while the shared back-half
//! already runs through [`Lattice::model_point`] and the symmetry group
//! unchanged.

use nalgebra::{Point2, Vector2};

use super::{CellTopology, Coord, GridTransform, Lattice, LatticeKind};

/// Zero-sized marker for the axial-coordinate hexagonal lattice.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Hex;

impl Hex {
    /// Construct the hex-lattice marker.
    pub const fn new() -> Self {
        Self
    }
}

impl Lattice for Hex {
    const KIND: LatticeKind = LatticeKind::Hex;

    fn model_point(self, coord: Coord) -> Point2<f32> {
        let q = coord.u as f32;
        let r = coord.v as f32;
        let half = 0.5_f32;
        let sqrt3_over_2 = 3.0_f32.sqrt() * half;
        Point2::new(q + half * r, sqrt3_over_2 * r)
    }

    fn neighbour_offsets(self) -> &'static [Coord] {
        &HEX_AXIAL_OFFSETS
    }

    fn symmetry_transforms(self) -> &'static [GridTransform] {
        &D6_TRANSFORMS
    }

    fn axis_family_count(self) -> usize {
        3
    }

    fn model_axis_directions(self) -> &'static [Vector2<f32>] {
        &HEX_AXIS_DIRECTIONS
    }

    fn cell_topology(self) -> CellTopology {
        CellTopology::TriangleIsCell
    }
}

/// Unit model-plane directions of the three hex axis families.
///
/// Derived from the axial neighbour offsets through [`Hex::model_point`] and
/// folded into the undirected upper half-plane: `(1,0)` maps to `(1,0)` at 0°,
/// `(0,1)` maps to `(cos60°, sin60°)` at 60°, and `(1,-1)` maps to a direction
/// at -60° ≡ 120°. The three families therefore sit at 0°, 60°, and 120°.
static HEX_AXIS_DIRECTIONS: [Vector2<f32>; 3] = [
    Vector2::new(1.0, 0.0),
    Vector2::new(0.5, 0.866_025_4),
    Vector2::new(-0.5, 0.866_025_4),
];

/// Six axial neighbour offsets on a hex grid.
pub const HEX_AXIAL_OFFSETS: [Coord; 6] = [
    Coord::new(1, 0),
    Coord::new(1, -1),
    Coord::new(0, -1),
    Coord::new(-1, 0),
    Coord::new(-1, 1),
    Coord::new(0, 1),
];

/// Dihedral group D6 acting on hex axial coordinates.
pub const D6_TRANSFORMS: [GridTransform; 12] = [
    GridTransform::new(LatticeKind::Hex, [[1, 0], [0, 1]], [0, 0]),
    GridTransform::new(LatticeKind::Hex, [[0, -1], [1, 1]], [0, 0]),
    GridTransform::new(LatticeKind::Hex, [[-1, -1], [1, 0]], [0, 0]),
    GridTransform::new(LatticeKind::Hex, [[-1, 0], [0, -1]], [0, 0]),
    GridTransform::new(LatticeKind::Hex, [[0, 1], [-1, -1]], [0, 0]),
    GridTransform::new(LatticeKind::Hex, [[1, 1], [-1, 0]], [0, 0]),
    GridTransform::new(LatticeKind::Hex, [[1, 1], [0, -1]], [0, 0]),
    GridTransform::new(LatticeKind::Hex, [[1, 0], [-1, -1]], [0, 0]),
    GridTransform::new(LatticeKind::Hex, [[0, -1], [-1, 0]], [0, 0]),
    GridTransform::new(LatticeKind::Hex, [[-1, -1], [0, 1]], [0, 0]),
    GridTransform::new(LatticeKind::Hex, [[-1, 0], [1, 1]], [0, 0]),
    GridTransform::new(LatticeKind::Hex, [[0, 1], [1, 0]], [0, 0]),
];
