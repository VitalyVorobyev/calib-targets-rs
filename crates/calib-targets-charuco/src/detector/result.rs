use calib_targets_aruco::MarkerDetection;
use calib_targets_core::{GridAlignment, TargetDetection};
use serde::Serialize;

/// Output of a ChArUco detection run.
///
/// `#[non_exhaustive]`: construct with [`CharucoDetectionResult::new`].
#[non_exhaustive]
#[derive(Clone, Debug, Serialize)]
pub struct CharucoDetectionResult {
    /// The labelled ChArUco corner detection.
    pub detection: TargetDetection,
    /// Marker detections that are consistent with [`Self::alignment`] (inliers
    /// of the chosen hypothesis).
    pub markers: Vec<MarkerDetection>,
    /// Alignment from the rectified grid coordinate system into board coordinates.
    pub alignment: GridAlignment,
}

impl CharucoDetectionResult {
    /// Create a result from its detection, inlier markers, and alignment.
    pub fn new(
        detection: TargetDetection,
        markers: Vec<MarkerDetection>,
        alignment: GridAlignment,
    ) -> Self {
        Self {
            detection,
            markers,
            alignment,
        }
    }
}
