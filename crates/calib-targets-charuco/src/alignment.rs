//! Marker-to-board alignment result.
//!
//! The board-level matcher in [`crate::CharucoDetector`] produces a
//! [`CharucoAlignment`] mapping detected grid `(i, j)` corners to absolute
//! board coordinates; the corner-mapping stage consumes it to assign each
//! chessboard inner corner its OpenCV-compatible ChArUco ID.

use calib_targets_core::{Coord, GridAlignment};
use serde::{Deserialize, Serialize};

/// Alignment result between detected markers and a board specification.
#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct CharucoAlignment {
    /// Grid alignment mapping detected `(i, j)` corners to board coordinates.
    pub alignment: GridAlignment,
    /// Indices of the marker detections that agreed with the chosen
    /// alignment (the inlier set).
    pub marker_inliers: Vec<usize>,
}

impl CharucoAlignment {
    /// Map grid coordinates `(i, j)` into board coordinates.
    #[inline]
    pub fn map(&self, i: i32, j: i32) -> Coord {
        self.alignment.map(i, j)
    }
}
