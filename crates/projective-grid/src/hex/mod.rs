//! Hexagonal grid support for pointy-top axial coordinates.
//!
//! This module provides hex-grid counterparts of the square-grid types
//! in the parent crate: 6-connected graph construction, BFS traversal,
//! smoothness analysis, D6 alignment transforms, and rectification.
//!
//! # Coordinate Convention
//!
//! Axial coordinates `(q, r)` are stored in [`GridIndex`](crate::GridIndex)
//! where `i = q` and `j = r`. Pointy-top orientation: `q` increases eastward,
//! `r` increases south-eastward.

pub mod alignment;
pub mod direction;
pub mod graph;
pub mod mesh;
pub mod rectify;
pub mod smoothness;
pub mod traverse;

pub use alignment::GRID_TRANSFORMS_D6;
pub use direction::{HexDirection, HexNodeNeighbor};
pub use graph::{HexGridGraph, HexNeighborValidator};
pub use mesh::{AffineTransform2D, HexGridHomographyMesh, HexMeshError};
pub use rectify::{HexGridHomography, HexRectifyError};
pub use smoothness::{hex_find_inconsistent_corners, hex_predict_grid_position};
pub use traverse::{hex_assign_grid_coordinates, hex_connected_components};
