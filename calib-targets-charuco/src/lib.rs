//! ChArUco-related utilities.
//!
//! Current focus:
//! - Rectification helpers for detected chessboard grids:
//!   - global homography: [`rectify_from_chessboard_result`]
//!   - mesh warp (piecewise homographies): [`rectify_mesh_from_grid`]
//!
//! Marker dictionaries and decoding live in the separate `calib-targets-aruco` crate.
//! This crate provides a grid-first ChArUco detector that:
//! - detects a chessboard grid from ChESS corners (`calib-targets-chessboard`),
//! - rectifies via mesh warp,
//! - decodes embedded markers on the rectified grid (`calib-targets-aruco`),
//! - aligns the detected grid to a known board definition and assigns corner IDs.

mod detector;
mod mesh_warp;
mod rectified_view;

pub use detector::{
    CharucoAlignment, CharucoBoard, CharucoBoardError, CharucoBoardSpec, CharucoDetectError,
    CharucoDetectionResult, CharucoDetector, CharucoDetectorParams, GridTransform, MarkerLayout,
};
pub use mesh_warp::{rectify_mesh_from_grid, MeshWarpError, RectifiedMeshView};
pub use rectified_view::{rectify_from_chessboard_result, RectifiedBoardView, RectifyError};
