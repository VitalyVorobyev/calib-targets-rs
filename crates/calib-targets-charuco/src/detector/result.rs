use crate::alignment::CharucoAlignment;
use calib_targets_chessboard::RectifiedMeshView;
use calib_targets_core::TargetDetection;

/// Output of a ChArUco detection run.
#[derive(Clone, Debug)]
pub struct CharucoDetectionResult {
    pub detection: TargetDetection,
    pub chessboard: TargetDetection,
    pub chessboard_inliers: Vec<usize>,
    /// Raw marker detections in the rectified grid coordinate system.
    pub markers: Vec<calib_targets_aruco::MarkerDetection>,
    /// Marker square coordinates aligned to the board definition.
    pub marker_board_cells: Vec<[i32; 2]>,
    pub alignment: CharucoAlignment,
    /// Optional rectified mesh view (built only if requested).
    pub rectified: Option<RectifiedMeshView>,
}
