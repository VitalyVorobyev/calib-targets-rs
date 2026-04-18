//! Generic 2D projective grid graph construction, traversal, and homography tools.
//!
//! This crate provides reusable algorithms for building 4-connected grid graphs
//! from detected 2D corners, assigning grid coordinates via BFS traversal,
//! and computing projective mappings (homographies) for grid rectification.
//!
//! It is pattern-agnostic: the [`NeighborValidator`] trait lets callers plug in
//! pattern-specific logic (chessboard orientation checks, marker constraints, etc.)
//! while the graph construction, traversal, and geometry remain generic.
//!
//! # Hex Grid Support
//!
//! The [`hex`] module provides 6-connected hexagonal grid counterparts for
//! pointy-top axial coordinates `(q, r)`.
//!
//! # Built-in Validators
//!
//! The [`validators`] module provides ready-to-use implementations of
//! [`NeighborValidator`] and [`hex::HexNeighborValidator`] for common scenarios.

mod float_helpers;

pub mod direction;
pub mod global_step;
pub mod graph;
pub mod grid_alignment;
pub mod grid_index;
pub mod grid_mesh;
pub mod grid_rectify;
pub mod grid_smoothness;
pub mod hex;
pub mod homography;
pub mod local_step;
pub mod traverse;
pub mod validators;

/// Trait alias for floating-point types supported by this crate.
///
/// Both `f32` and `f64` satisfy this bound. All public generic types default
/// to `f32` for backward compatibility.
pub trait Float: nalgebra::RealField + Copy {}
impl<T: nalgebra::RealField + Copy> Float for T {}

pub use direction::{NeighborDirection, NodeNeighbor};
pub use global_step::{estimate_global_cell_size, GlobalStepEstimate, GlobalStepParams};
pub use graph::{GridGraph, GridGraphParams, NeighborCandidate, NeighborValidator};
pub use grid_alignment::{GridAlignment, GridTransform, GRID_TRANSFORMS_D4};
pub use grid_index::GridIndex;
pub use grid_mesh::GridHomographyMesh;
pub use grid_rectify::GridHomography;
pub use grid_smoothness::{
    find_inconsistent_corners, find_inconsistent_corners_step_aware, predict_grid_position,
};
pub use homography::{estimate_homography, homography_from_4pt, Homography};
pub use local_step::{estimate_local_steps, LocalStep, LocalStepParams, LocalStepPointData};
pub use traverse::{assign_grid_coordinates, connected_components};
