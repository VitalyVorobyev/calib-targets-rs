//! Hexagonal grid support for pointy-top axial coordinates.
//!
//! Geometry-only primitives: D6 alignment transforms, per-cell mesh and
//! global rectification, midpoint prediction / outlier detection. The
//! seed-and-grow path is square-only at present.
//!
//! # Coordinate Convention
//!
//! Axial coordinates `(q, r)` are stored in [`GridCoords`](crate::GridCoords)
//! where `i = q` and `j = r`. Pointy-top orientation: `q` increases eastward,
//! `r` increases south-eastward.

pub mod alignment;
pub mod mesh;
pub mod rectify;
pub mod smoothness;

pub use crate::affine::AffineTransform2D;
pub use alignment::GRID_TRANSFORMS_D6;
pub use mesh::{HexGridHomographyMesh, HexMeshError};
pub use rectify::{HexGridHomography, HexRectifyError};
pub use smoothness::{hex_find_inconsistent_corners, hex_predict_grid_position};
