//! Marker-to-board alignment result.
//!
//! The board-level matcher in [`crate::CharucoDetector`] produces a
//! [`CharucoAlignment`] mapping detected grid `(i, j)` corners to absolute
//! board coordinates; the corner-mapping stage consumes it to assign each
//! chessboard inner corner its OpenCV-compatible ChArUco ID.

use calib_targets_core::{GridAlignment, GridCoords};
use serde::{Deserialize, Serialize};

/// Alignment result between detected markers and a board specification.
#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CharucoAlignment {
    /// Grid alignment mapping detected `(i, j)` corners to board coordinates.
    pub alignment: GridAlignment,
    /// Indices of the marker detections that agreed with the chosen
    /// alignment (the inlier set).
    pub marker_inliers: Vec<usize>,
}

impl CharucoAlignment {
    /// Build an alignment result from a grid alignment and its inlier marker
    /// indices.
    pub fn new(alignment: GridAlignment, marker_inliers: Vec<usize>) -> Self {
        Self {
            alignment,
            marker_inliers,
        }
    }

    /// Map grid coordinates `(i, j)` into board coordinates.
    #[inline]
    pub fn map(&self, i: i32, j: i32) -> GridCoords {
        self.alignment.map(i, j)
    }
}
