//! Hexagonal grid support for pointy-top axial coordinates.
//!
//! The geometry-only sibling of [`square`](crate::square): D6 alignment
//! transforms, per-cell mesh and global rectification, and midpoint
//! prediction / outlier detection. There is no seed-and-grow path yet —
//! that pipeline is square-only at present.
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`alignment`] | D6 rotations / reflections on axial `(q, r)` |
//! | [`mesh`] | Per-triangle homography mesh over a hex grid |
//! | [`rectify`] | Global homography from hex grid corner positions |
//! | [`smoothness`] | Midpoint prediction and outlier detection |
//!
//! Top-level types are re-exported at this module's root.
//!
//! # Coordinate Convention
//!
//! Axial coordinates `(q, r)` are stored in [`GridCoords`](crate::GridCoords)
//! where `i = q` and `j = r`. Pointy-top orientation: `q` increases eastward,
//! `r` increases south-eastward.

/// D6 rotations and reflections on integer axial `(q, r)` grid coordinates.
pub mod alignment;
pub mod mesh;
pub mod rectify;
pub mod smoothness;

pub use crate::affine::AffineTransform2D;
pub use alignment::GRID_TRANSFORMS_D6;
pub use mesh::{HexGridHomographyMesh, HexGridMeshError};
pub use rectify::{HexGridHomography, HexGridRectifyError};
pub use smoothness::{hex_find_inconsistent_corners, hex_predict_grid_position};
