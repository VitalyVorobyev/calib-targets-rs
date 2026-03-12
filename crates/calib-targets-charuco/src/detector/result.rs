use super::corner_validation::CornerValidationDiagnostics;
use calib_targets_aruco::MarkerDetection;
use calib_targets_chessboard::ChessboardDiagnostics;
use calib_targets_core::{GridAlignment, TargetDetection};
use serde::{Deserialize, Serialize};

/// Output of a ChArUco detection run.
#[derive(Clone, Debug, Serialize)]
pub struct CharucoDetectionResult {
    pub detection: TargetDetection,
    /// Raw marker detections in the rectified grid coordinate system.
    pub markers: Vec<MarkerDetection>,
    /// Alignment from the rectified grid coordinate system into board coordinates.
    pub alignment: GridAlignment,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CharucoStageTimings {
    pub chessboard_ms: f64,
    pub decode_ms: f64,
    pub alignment_ms: f64,
    pub map_validate_ms: f64,
    pub total_ms: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CharucoDiagnostics {
    pub chessboard: ChessboardDiagnostics,
    pub candidate_cell_count: usize,
    pub complete_candidate_cell_count: usize,
    pub inferred_candidate_cell_count: usize,
    pub decoded_marker_count: usize,
    pub aligned_marker_count: usize,
    pub alignment_inlier_count: usize,
    pub alignment_candidate_count: usize,
    pub alignment_corner_in_bounds_count: usize,
    pub alignment_corner_in_bounds_ratio: f32,
    pub alignment_runner_up_inlier_count: usize,
    pub alignment_runner_up_corner_in_bounds_ratio: f32,
    pub mapped_corner_count_before_validation: usize,
    pub corner_validation: Option<CornerValidationDiagnostics>,
    pub final_corner_count: usize,
    pub timings: CharucoStageTimings,
}

#[derive(Debug)]
pub struct CharucoDetectionRun {
    pub result: Result<CharucoDetectionResult, super::CharucoDetectError>,
    pub diagnostics: CharucoDiagnostics,
    pub markers: Vec<MarkerDetection>,
    pub alignment: Option<GridAlignment>,
}
