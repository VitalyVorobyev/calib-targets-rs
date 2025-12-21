use calib_targets_core::TargetDetection;

/// Output of a ChArUco detection run.
#[derive(Clone, Debug)]
pub struct CharucoDetectionResult {
    pub detection: TargetDetection,
    /// Raw marker detections in the rectified grid coordinate system.
    pub markers: Vec<calib_targets_aruco::MarkerDetection>,
}
