use super::corner_validation::CornerValidationDiagnostics;
use super::{CharucoDetectError, CharucoDetectionResult};
use crate::alignment::CharucoAlignment;
use calib_targets_aruco::MarkerDetection;
use calib_targets_chessboard::ChessboardDetectionResult;
use std::cmp::Ordering;

#[derive(Debug)]
pub(super) struct CandidateEvaluation {
    pub chessboard: ChessboardDetectionResult,
    pub candidate_cell_count: usize,
    pub complete_candidate_cell_count: usize,
    pub inferred_candidate_cell_count: usize,
    pub decoded_marker_count: usize,
    pub aligned_marker_count: usize,
    pub alignment_candidate_count: usize,
    pub alignment_corner_in_bounds_count: usize,
    pub alignment_corner_in_bounds_ratio: f32,
    pub alignment_runner_up_inlier_count: usize,
    pub alignment_runner_up_corner_in_bounds_ratio: f32,
    pub markers: Vec<MarkerDetection>,
    pub alignment: Option<CharucoAlignment>,
    pub mapped_corner_count_before_validation: usize,
    pub final_corner_count: usize,
    pub corner_validation: Option<CornerValidationDiagnostics>,
    pub result: Option<CharucoDetectionResult>,
    pub decode_ms: f64,
    pub alignment_ms: f64,
    pub map_validate_ms: f64,
    pub failure: Option<CandidateFailure>,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum CandidateFailure {
    NoMarkers,
    AlignmentFailed { inliers: usize },
}

pub(super) fn select_best_evaluation(
    evaluations: &[CandidateEvaluation],
) -> Option<&CandidateEvaluation> {
    evaluations.iter().max_by(|a, b| compare_evaluations(a, b))
}

pub(super) fn compare_evaluations(a: &CandidateEvaluation, b: &CandidateEvaluation) -> Ordering {
    let a_success = a.failure.is_none();
    let b_success = b.failure.is_none();
    a_success
        .cmp(&b_success)
        .then_with(|| a.final_corner_count.cmp(&b.final_corner_count))
        .then_with(|| {
            a.alignment
                .as_ref()
                .map(|alignment| alignment.marker_inliers.len())
                .unwrap_or(0)
                .cmp(
                    &b.alignment
                        .as_ref()
                        .map(|alignment| alignment.marker_inliers.len())
                        .unwrap_or(0),
                )
        })
        .then_with(|| a.markers.len().cmp(&b.markers.len()))
        .then_with(|| {
            a.chessboard
                .detection
                .corners
                .len()
                .cmp(&b.chessboard.detection.corners.len())
        })
}

pub(super) fn candidate_failure_to_error(failure: CandidateFailure) -> CharucoDetectError {
    match failure {
        CandidateFailure::NoMarkers => CharucoDetectError::NoMarkers,
        CandidateFailure::AlignmentFailed { inliers } => {
            CharucoDetectError::AlignmentFailed { inliers }
        }
    }
}
