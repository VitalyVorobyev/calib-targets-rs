use calib_targets_aruco::MarkerDetection;
use calib_targets_core::{GridAlignment, TargetDetection};
use serde::Serialize;

/// Output of a ChArUco detection run.
#[derive(Clone, Debug, Serialize)]
pub struct CharucoDetectionResult {
    pub detection: TargetDetection,
    /// Marker detections that are consistent with [`Self::alignment`] (inliers
    /// of the chosen hypothesis).
    pub markers: Vec<MarkerDetection>,
    /// Alignment from the rectified grid coordinate system into board coordinates.
    pub alignment: GridAlignment,
    /// Total number of markers decoded out of candidate cells, **before**
    /// alignment-based inlier filtering.
    ///
    /// `raw_marker_count - markers.len()` is the number of raw marker
    /// decodings rejected by the alignment stage.
    #[serde(default)]
    pub raw_marker_count: usize,
    /// Raw decodings whose id mapped to a valid board position that
    /// **disagreed** with the chosen [`Self::alignment`]. This is the
    /// self-consistency wrong-id count used by the internal charuco
    /// benchmark: it excludes pure dictionary-noise decodings whose id did
    /// not correspond to any marker on this board.
    #[serde(default)]
    pub raw_marker_wrong_id_count: usize,
}
