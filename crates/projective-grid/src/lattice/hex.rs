//! Hex-lattice family: roadmap stub.
//!
//! [`Hex`] implements [`Lattice`] with the axial `model_point`, the six axial
//! neighbour offsets, and the dihedral D6 symmetry group. The lattice geometry
//! is complete; what is *not* yet wired is hex grid **detection** — the
//! strategy skeletons are square-concrete today. Per `docs/DESIGN.md`
//! ("Extending to hex"), adding hex recovery is a fill-in-the-trait task:
//! implement the lattice-specific arms of each strategy against this trait
//! (seed/cell shape for seed-and-grow, hexagon assembly for topological) while
//! the shared back-half already runs through [`Lattice::model_point`] and the
//! symmetry group unchanged.

use nalgebra::Point2;

use super::{Coord, GridTransform, Lattice, LatticeKind};

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
}

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
