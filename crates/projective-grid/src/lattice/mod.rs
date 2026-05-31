//! Lattice-family axis: the parameter that the strategies and the shared
//! back-half are written against, rather than a copy per family.
//!
//! This module hosts the family-agnostic coordinate types ([`Coord`],
//! [`GridDimensions`], [`GridTransform`]), the [`LatticeKind`] selector, and
//! the [`Lattice`] trait that captures the per-family geometry a recovery
//! pipeline needs: how a lattice coordinate maps into the model plane, the
//! cardinal neighbour offsets, and the coordinate symmetry group.
//!
//! Today only [`Square`] is implemented; [`Hex`] is a
//! roadmap stub (see `docs/DESIGN.md` "Extending to hex"). Both the strategies
//! and `shared::fit` reach the geometry through [`LatticeKind`] /
//! [`Lattice::model_point`], so adding hex detection is a fill-in-the-trait
//! task rather than a new folder tree.

use nalgebra::Point2;

pub mod hex;
pub mod square;

pub use hex::Hex;
pub use square::Square;

/// Integer coordinate on a lattice.
///
/// For square grids this is `(u, v) = (i, j)`. For hex grids this is axial
/// `(u, v) = (q, r)`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub struct Coord {
    /// First lattice coordinate: square `i`, or hex axial `q`.
    pub u: i32,
    /// Second lattice coordinate: square `j`, or hex axial `r`.
    pub v: i32,
}

impl Coord {
    /// Construct a coordinate from two integer components.
    pub const fn new(u: i32, v: i32) -> Self {
        Self { u, v }
    }
}

/// Optional known grid dimensions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct GridDimensions {
    /// Number of cells or feature positions along the first lattice axis.
    pub width: usize,
    /// Number of cells or feature positions along the second lattice axis.
    pub height: usize,
}

impl GridDimensions {
    /// Construct known grid dimensions.
    pub const fn new(width: usize, height: usize) -> Self {
        Self { width, height }
    }
}

/// Supported lattice families.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LatticeKind {
    /// Orthogonal square lattice.
    Square,
    /// Axial-coordinate hexagonal lattice.
    Hex,
}

impl LatticeKind {
    /// Map an integer lattice coordinate into the model plane.
    ///
    /// Square coordinates map to `(u, v)`. Hex axial coordinates map to
    /// `(q + 0.5*r, sqrt(3)/2*r)`, using unit nearest-neighbour spacing in the
    /// model plane.
    ///
    /// This dispatches to the [`Lattice::model_point`] of the family impl, so
    /// callers holding only a [`LatticeKind`] (e.g. `shared::fit`,
    /// [`crate::check`]) need not name the concrete family type.
    pub fn model_point(self, coord: Coord) -> Point2<f32> {
        match self {
            Self::Square => Square.model_point(coord),
            Self::Hex => Hex.model_point(coord),
        }
    }

    /// Cardinal neighbour offsets for this family (4 for square, 6 for hex).
    pub fn neighbour_offsets(self) -> &'static [Coord] {
        match self {
            Self::Square => Square.neighbour_offsets(),
            Self::Hex => Hex.neighbour_offsets(),
        }
    }

    /// The coordinate symmetry group for this family (D4 for square,
    /// D6 for hex).
    pub fn symmetry_transforms(self) -> &'static [GridTransform] {
        match self {
            Self::Square => Square.symmetry_transforms(),
            Self::Hex => Hex.symmetry_transforms(),
        }
    }
}

/// Per-family lattice geometry.
///
/// A [`Lattice`] impl supplies the geometry a recovery pipeline needs without
/// hard-coding the family: how a coordinate maps into the model plane, the
/// cardinal neighbour offsets used to walk the graph, and the coordinate
/// symmetry group used by component merge. The shared back-half and (in the
/// hex roadmap) the strategy skeletons are written against this trait so a new
/// family is added by implementing the trait, not by copying machinery.
///
/// Implementations are zero-sized markers ([`Square`], [`Hex`]); the
/// [`LatticeKind`] enum is the runtime selector that dispatches to them.
pub trait Lattice: Copy {
    /// The [`LatticeKind`] this impl corresponds to.
    const KIND: LatticeKind;

    /// Map an integer lattice coordinate into the model plane (unit
    /// nearest-neighbour spacing).
    fn model_point(self, coord: Coord) -> Point2<f32>;

    /// Cardinal neighbour offsets used to walk between adjacent lattice
    /// coordinates.
    fn neighbour_offsets(self) -> &'static [Coord];

    /// The coordinate symmetry group (dihedral) for this family.
    fn symmetry_transforms(self) -> &'static [GridTransform];
}

/// A lattice-coordinate symmetry transform: `out = matrix * coord + offset`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct GridTransform {
    /// Lattice family this transform belongs to.
    pub source_kind: LatticeKind,
    /// Row-major 2x2 integer linear part.
    pub matrix: [[i32; 2]; 2],
    /// Integer offset applied after the linear part.
    pub offset: [i32; 2],
}

impl GridTransform {
    /// Construct a lattice transform from raw components.
    pub const fn new(source_kind: LatticeKind, matrix: [[i32; 2]; 2], offset: [i32; 2]) -> Self {
        Self {
            source_kind,
            matrix,
            offset,
        }
    }

    /// Apply this transform to a coordinate.
    pub fn apply(self, coord: Coord) -> Coord {
        Coord {
            u: self.matrix[0][0] * coord.u + self.matrix[0][1] * coord.v + self.offset[0],
            v: self.matrix[1][0] * coord.u + self.matrix[1][1] * coord.v + self.offset[1],
        }
    }

    /// Determinant of the linear part.
    pub const fn determinant(self) -> i32 {
        self.matrix[0][0] * self.matrix[1][1] - self.matrix[0][1] * self.matrix[1][0]
    }
}

/// Four cardinal neighbour offsets on a square grid.
pub const SQUARE_CARDINAL_OFFSETS: [Coord; 4] = square::SQUARE_CARDINAL_OFFSETS;

/// Six axial neighbour offsets on a hex grid.
pub const HEX_AXIAL_OFFSETS: [Coord; 6] = hex::HEX_AXIAL_OFFSETS;

/// Dihedral group D4 acting on square lattice coordinates.
pub const D4_TRANSFORMS: [GridTransform; 8] = square::D4_TRANSFORMS;

/// Dihedral group D6 acting on hex axial coordinates.
pub const D6_TRANSFORMS: [GridTransform; 12] = hex::D6_TRANSFORMS;

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn square_model_mapping_is_cartesian() {
        let p = LatticeKind::Square.model_point(Coord::new(2, -3));
        assert_eq!(p, Point2::new(2.0, -3.0));
    }

    #[test]
    fn hex_model_mapping_is_axial() {
        let p = LatticeKind::Hex.model_point(Coord::new(1, 2));
        assert!((p.x - 2.0).abs() < 1e-6);
        assert!((p.y - 3.0_f32.sqrt()).abs() < 1e-6);
    }

    #[test]
    fn kind_dispatch_matches_trait_impls() {
        let c = Coord::new(3, -1);
        assert_eq!(LatticeKind::Square.model_point(c), Square.model_point(c));
        assert_eq!(LatticeKind::Hex.model_point(c), Hex.model_point(c));
        assert_eq!(
            LatticeKind::Square.neighbour_offsets(),
            Square.neighbour_offsets()
        );
        assert_eq!(
            LatticeKind::Square.symmetry_transforms().len(),
            D4_TRANSFORMS.len()
        );
        assert_eq!(
            LatticeKind::Hex.symmetry_transforms().len(),
            D6_TRANSFORMS.len()
        );
    }

    #[test]
    fn d4_table_is_complete() {
        let set: HashSet<_> = D4_TRANSFORMS.iter().map(|t| t.matrix).collect();
        assert_eq!(set.len(), 8);
        assert!(D4_TRANSFORMS
            .iter()
            .all(|t| t.source_kind == LatticeKind::Square && t.determinant().abs() == 1));
    }

    #[test]
    fn d6_table_is_complete() {
        let set: HashSet<_> = D6_TRANSFORMS.iter().map(|t| t.matrix).collect();
        assert_eq!(set.len(), 12);
        assert!(D6_TRANSFORMS
            .iter()
            .all(|t| t.source_kind == LatticeKind::Hex && t.determinant().abs() == 1));
    }
}
