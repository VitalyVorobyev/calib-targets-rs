//! Square (4-connected) grid support.
//!
//! Mirrors the [`hex`](crate::hex) module for square grids. Modules parallel
//! the hex submodules:
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`direction`] | 4 cardinal directions + neighbor delta helpers |
//! | [`alignment`] | D4 rotations and reflections on `(i, j)` |
//! | [`index`] | `(i, j)` cell identifier shared with the hex side (axial `(q, r)`) |
//! | [`mesh`] | Per-cell homography mesh over a regular grid |
//! | [`rectify`] | Global homography from grid corner positions |
//! | [`smoothness`] | Midpoint prediction and outlier detection |
//! | [`validators`] | Ready-to-use [`NeighborValidator`](crate::NeighborValidator) implementations |
//!
//! Top-level types are re-exported at the crate root for back-compat.

pub mod alignment;
pub mod direction;
pub mod index;
pub mod mesh;
pub mod rectify;
pub mod smoothness;
pub mod validators;

pub use alignment::{GridAlignment, GridTransform, GRID_TRANSFORMS_D4};
pub use direction::{NeighborDirection, NodeNeighbor};
pub use index::GridIndex;
pub use mesh::GridHomographyMesh;
pub use rectify::GridHomography;
pub use smoothness::{
    find_inconsistent_corners, find_inconsistent_corners_step_aware, predict_grid_position,
};
pub use validators::{SpatialSquareValidator, XJunctionValidator};
