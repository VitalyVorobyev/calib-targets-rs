//! ChArUco-related utilities.
//!
//! Current focus:
//! - Rectification helpers for detected chessboard grids:
//!   - global homography: [`rectify_from_chessboard_result`]
//!   - mesh warp (piecewise homographies): [`rectify_mesh_from_grid`]
//!
//! Marker dictionaries and decoding live in the separate `calib-targets-aruco` crate.
//! A full ChArUco board solver (marker→board pose, corner IDs/interpolation) is not implemented yet.

mod mesh_warp;
mod rectified_view;
// NOTE: marker dictionaries + decoding live in the `calib-targets-aruco` crate.

pub use mesh_warp::{rectify_mesh_from_grid, MeshWarpError, RectifiedMeshView};
pub use rectified_view::{rectify_from_chessboard_result, RectifiedBoardView, RectifyError};

use calib_targets_core::TargetDetection;

#[derive(Clone, Debug)]
pub struct CharucoParams {
    // put dictionary ref, board layout, etc., here
}

pub struct CharucoDetector {
    pub params: CharucoParams,
}

impl CharucoDetector {
    pub fn new(params: CharucoParams) -> Self {
        Self { params }
    }

    /// Placeholder: later this will take an image and/or corners + marker quads.
    pub fn detect(&self) -> Vec<TargetDetection> {
        Vec::new()
    }
}
