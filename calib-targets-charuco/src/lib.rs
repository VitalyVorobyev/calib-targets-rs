//! ChArUco detector skeleton.
//!
//! Future work:
//! - Marker detection (ArUco-like).
//! - ID decoding.
//! - Board homography + interior corner interpolation.

mod detect_aruco;
mod mesh_warp;
mod rectified_view;
mod scan_decode_4x4;

pub use mesh_warp::{rectify_mesh_from_grid, MeshWarpError, RectifiedMeshView};
pub use rectified_view::rectify_from_chessboard_result;

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
