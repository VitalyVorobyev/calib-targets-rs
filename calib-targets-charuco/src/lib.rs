//! ChArUco detector skeleton.
//!
//! Future work:
//! - Marker detection (ArUco-like).
//! - ID decoding.
//! - Board homography + interior corner interpolation.

use calib_targets_core::{LabeledCorner, TargetDetection, TargetKind};

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
