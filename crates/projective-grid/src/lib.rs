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

pub mod direction;
pub mod graph;
pub mod grid_alignment;
pub mod grid_index;
pub mod grid_mesh;
pub mod grid_rectify;
pub mod grid_smoothness;
pub mod hex;
pub mod homography;
pub mod traverse;

pub use direction::{NeighborDirection, NodeNeighbor};
pub use graph::{GridGraph, GridGraphParams, NeighborCandidate, NeighborValidator};
pub use grid_alignment::{GridAlignment, GridTransform, GRID_TRANSFORMS_D4};
pub use grid_index::GridIndex;
pub use grid_mesh::GridHomographyMesh;
pub use grid_rectify::GridHomography;
pub use grid_smoothness::{find_inconsistent_corners, predict_grid_position};
pub use homography::{estimate_homography, homography_from_4pt, Homography};
pub use traverse::{assign_grid_coordinates, connected_components};
