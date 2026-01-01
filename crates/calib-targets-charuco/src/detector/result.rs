use calib_targets_aruco::MarkerDetection;
use calib_targets_core::{GridAlignment, TargetDetection};
use serde::Serialize;

/// Output of a ChArUco detection run.
#[derive(Clone, Debug, Serialize)]
pub struct CharucoDetectionResult {
    pub detection: TargetDetection,
    /// Raw marker detections in the rectified grid coordinate system.
    pub markers: Vec<MarkerDetection>,
    /// Alignment from the rectified grid coordinate system into board coordinates.
    pub alignment: GridAlignment,
}
