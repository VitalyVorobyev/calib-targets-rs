//! Plain chessboard detector built on top of `calib-targets-core`.
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
mod params;

pub use detector::ChessboardDetector;
pub use gridgraph::{GridGraph, GridGraphParams};
pub use params::ChessboardParams;
