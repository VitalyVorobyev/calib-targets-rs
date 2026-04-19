//! Generic 2D projective grid graph construction, traversal, and homography tools.
//!
//! This crate provides reusable algorithms for building 4- or 6-connected grid
//! graphs from detected 2D points, assigning grid coordinates via BFS traversal,
//! and computing projective mappings (homographies) for grid rectification.
//!
//! It is pattern-agnostic: the [`NeighborValidator`] trait lets callers plug in
//! pattern-specific logic (chessboard orientation checks, marker constraints,
//! etc.) while the graph construction, traversal, and geometry remain generic.
//!
//! # Module layout
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`square`] | Square (4-connected) grid: direction, alignment, index, mesh, rectify, smoothness, validators |
//! | [`hex`] | Hex (6-connected) grid: mirrored submodules for axial `(q, r)` coordinates |
//! | [`graph`] | Generic KD-tree-based graph builder + [`NeighborValidator`] trait |
//! | [`graph_cleanup`] | Graph-level cleanup passes (symmetry, straightness, crossing pruning) |
//! | [`global_step`] / [`local_step`] | Cell-size / local-step estimation on point clouds |
//! | [`traverse`] | BFS / DFS on grid graphs (connected components, `(i, j)` assignment) |
//! | [`homography`] | Projective geometry (4-point homography, DLT) |
//!
//! Top-level types from [`square`] are re-exported here at the crate root so
//! existing consumers (e.g., `projective_grid::GridIndex`) continue to work
//! unchanged.

mod float_helpers;

pub mod global_step;
pub mod graph;
pub mod graph_cleanup;
pub mod hex;
pub mod homography;
pub mod local_step;
pub mod square;
pub mod traverse;

/// Re-export of the square-grid submodule at the legacy `validators` path.
///
/// Kept for back-compat with doc-comment examples that wrote
/// `projective_grid::validators::XJunctionValidator`.
pub mod validators {
    pub use crate::square::validators::{SpatialSquareValidator, XJunctionValidator};
}

/// Trait alias for floating-point types supported by this crate.
///
/// Both `f32` and `f64` satisfy this bound. All public generic types default
/// to `f32` for backward compatibility.
pub trait Float: nalgebra::RealField + Copy {}
impl<T: nalgebra::RealField + Copy> Float for T {}

// --- Generic building blocks (no square / hex assumption) --------------------
pub use global_step::{estimate_global_cell_size, GlobalStepEstimate, GlobalStepParams};
pub use graph::{GridGraph, GridGraphParams, NeighborCandidate, NeighborValidator};
pub use graph_cleanup::{
    enforce_symmetry, prune_by_edge_straightness, prune_crossing_edges, prune_isolated_pairs,
    segments_properly_cross,
};
pub use homography::{estimate_homography, homography_from_4pt, Homography};
pub use local_step::{estimate_local_steps, LocalStep, LocalStepParams, LocalStepPointData};
pub use traverse::{assign_grid_coordinates, connected_components};

// --- Square-grid surface re-exported at the crate root (back-compat) --------
pub use square::alignment::{GridAlignment, GridTransform, GRID_TRANSFORMS_D4};
pub use square::direction::{NeighborDirection, NodeNeighbor};
pub use square::index::GridIndex;
pub use square::mesh::GridHomographyMesh;
pub use square::rectify::GridHomography;
pub use square::smoothness::{
    find_inconsistent_corners, find_inconsistent_corners_step_aware, predict_grid_position,
};
pub use square::validators::{SpatialSquareValidator, XJunctionValidator};

// `SpatialHexValidator` stays namespaced under `hex::validators` and
// `hex::SpatialHexValidator` — it's hex-specific and does not need a
// crate-root re-export.
