//! Plain chessboard detector built on top of `calib-targets-core`.
//!
//! ## Quickstart
//!
//! ```
//! use calib_targets_chessboard::{ChessboardDetector, ChessboardParams, GridGraphParams};
//! use calib_targets_core::Corner;
//!
//! let params = ChessboardParams::default();
//! let detector = ChessboardDetector::new(params).with_grid_search(GridGraphParams::default());
//!
//! let corners: Vec<Corner> = Vec::new();
//! let result = detector.detect_from_corners(&corners);
//! println!("detected: {}", result.is_some());
//! ```
//!
//! New algorithm (graph-based, perspective-aware):
//! 1. Filter strong ChESS corners.
//! 2. Estimate two approximate global grid axes (u, v) from ChESS orientations.
//! 3. Estimate a base spacing from nearest-neighbor distances.
//! 4. For each corner, find up to 4 neighbors (right/left/up/down) based on:
//!    - distance ~ base spacing,
//!    - direction roughly along ±u or ±v (orientation-based),
//! 5. Build an undirected 4-connected grid graph from these neighbor relations.
//! 6. BFS each connected component, assign integer coordinates (i, j).
//! 7. For each component, compute width×height and completeness.
//! 8. Keep the best component that matches expected_rows/cols (up to swap) and completeness threshold.

mod detector;
mod geom;
mod gridgraph;
mod mesh_warp;
mod params;
mod rectified_view;

pub use detector::{
    ChessboardDebug, ChessboardDetectionResult, ChessboardDetector, GridGraphDebug,
    GridGraphNeighborDebug, GridGraphNodeDebug,
};
pub use mesh_warp::{rectify_mesh_from_grid, MeshWarpError, RectifiedMeshView};
pub use params::{ChessboardParams, GridGraphParams};
pub use rectified_view::{rectify_from_chessboard_result, RectifiedBoardView, RectifyError};
