use super::alignment_select::{
    alignment_has_sufficient_support, select_alignment, AlignmentAttempt, AlignmentSelection,
};
use super::candidate_eval::{
    candidate_failure_to_error, compare_evaluations, select_best_evaluation, CandidateEvaluation,
    CandidateFailure,
};
use super::corner_mapping::map_charuco_corners;
use super::corner_validation::{
    validate_and_fix_corners, CornerValidationConfig, CornerValidationRun,
};
use super::marker_decode::{
    decode_cell_evidence, dedup_markers_by_id, summarize_cell_decode_diagnostics,
    CellDecodeEvidence,
};
use super::marker_sampling::{build_corner_map, build_marker_cell_candidates, MarkerCellSource};
use super::patch_placement::{add_alignment_match_diagnostics, select_patch_alignment};
use super::rectified_recovery::decode_markers_from_rectified_view;
use super::{
    CharucoDetectError, CharucoDetectionResult, CharucoDetectionRun, CharucoDetectorParams,
    CharucoDiagnostics, MarkerPathDiagnostics, PatchPlacementDiagnostics,
};
use crate::alignment::CharucoAlignment;
use crate::board::{CharucoBoard, CharucoBoardError};
use calib_targets_aruco::{MarkerDetection, Matcher};
use calib_targets_chessboard::{ChessboardDetectionResult, ChessboardDetector};
use calib_targets_core::{Corner, GrayImageView, TargetDetection};
use std::time::Instant;

#[cfg(feature = "tracing")]
use tracing::instrument;

/// Grid-first ChArUco detector.
#[derive(Debug)]
pub struct CharucoDetector {
    board: CharucoBoard,
    params: CharucoDetectorParams,
    matcher: Matcher,
}

struct CandidateContext {
    chessboard: ChessboardDetectionResult,
    candidate_cell_count: usize,
    complete_candidate_cell_count: usize,
    inferred_candidate_cell_count: usize,
    marker_path: MarkerPathDiagnostics,
    patch_placement: PatchPlacementDiagnostics,
    cell_evidence: Vec<CellDecodeEvidence>,
    decoded_marker_count: usize,
    decode_ms: f64,
}

#[derive(Clone)]
struct CandidateBase {
    chessboard: ChessboardDetectionResult,
    complete_candidate_cell_count: usize,
    inferred_candidate_cell_count: usize,
    marker_path: MarkerPathDiagnostics,
    patch_placement: PatchPlacementDiagnostics,
    cell_evidence: Vec<CellDecodeEvidence>,
    decode_ms: f64,
}

impl CandidateBase {
    fn into_context(self, decoded_marker_count: usize) -> CandidateContext {
        CandidateContext {
            candidate_cell_count: self.complete_candidate_cell_count
                + self.inferred_candidate_cell_count,
            chessboard: self.chessboard,
            complete_candidate_cell_count: self.complete_candidate_cell_count,
            inferred_candidate_cell_count: self.inferred_candidate_cell_count,
            marker_path: self.marker_path,
            patch_placement: self.patch_placement,
            cell_evidence: self.cell_evidence,
            decoded_marker_count,
            decode_ms: self.decode_ms,
        }
    }
}

impl CharucoDetector {
    /// Create a detector from parameters (board spec lives in `params.charuco`).
    pub fn new(mut params: CharucoDetectorParams) -> Result<Self, CharucoBoardError> {
        let board_cfg = params.charuco;
        if params.chessboard.expected_rows.is_none() {
            params.chessboard.expected_rows = board_cfg.rows.checked_sub(1);
        }
        if params.chessboard.expected_cols.is_none() {
            params.chessboard.expected_cols = board_cfg.cols.checked_sub(1);
        }
        if !params.scan.marker_size_rel.is_finite() || params.scan.marker_size_rel <= 0.0 {
            params.scan.marker_size_rel = board_cfg.marker_size_rel;
        }

        let max_hamming = params
            .max_hamming
            .min(board_cfg.dictionary.max_correction_bits);
        params.max_hamming = max_hamming;

        let matcher = Matcher::new(board_cfg.dictionary, max_hamming);
        let board = CharucoBoard::new(board_cfg)?;

        Ok(Self {
            board,
            params,
            matcher,
        })
    }

    /// Board definition used by the detector.
    #[inline]
    pub fn board(&self) -> &CharucoBoard {
        &self.board
    }

    /// Detector parameters.
    #[inline]
    pub fn params(&self) -> &CharucoDetectorParams {
        &self.params
    }

    /// Detect a ChArUco board from an image and a set of corners.
    #[cfg_attr(feature = "tracing", instrument(level = "info", skip(self, image, corners), fields(num_corners=corners.len())))]
    pub fn detect(
        &self,
        image: &GrayImageView<'_>,
        corners: &[Corner],
    ) -> Result<CharucoDetectionResult, CharucoDetectError> {
        self.detect_with_diagnostics(image, corners).result
    }

    #[cfg_attr(feature = "tracing", instrument(level = "info", skip(self, image, corners), fields(num_corners=corners.len())))]
    pub fn detect_with_diagnostics(
        &self,
        image: &GrayImageView<'_>,
        corners: &[Corner],
    ) -> CharucoDetectionRun {
        let total_start = Instant::now();
        let chessboard_start = Instant::now();
        let detector = ChessboardDetector::new(self.params.chessboard.clone())
            .with_grid_search(self.params.graph.clone());
        let chessboard_run = detector.detect_from_corners_with_diagnostics(corners);
        let mut diagnostics = CharucoDiagnostics {
            chessboard: chessboard_run.diagnostics.clone(),
            ..CharucoDiagnostics::default()
        };
        diagnostics.timings.chessboard_ms = elapsed_ms(chessboard_start);

        if chessboard_run.candidates.is_empty() {
            diagnostics.timings.total_ms = elapsed_ms(total_start);
            return CharucoDetectionRun {
                result: Err(CharucoDetectError::ChessboardNotDetected),
                diagnostics,
                markers: Vec::new(),
                alignment: None,
            };
        }

        let mut evaluations = Vec::with_capacity(chessboard_run.candidates.len());
        for candidate in chessboard_run.candidates {
            evaluations.push(self.evaluate_candidate(image, candidate));
        }

        let selected = select_best_evaluation(&evaluations)
            .expect("at least one candidate evaluation should exist");

        diagnostics.chessboard.selected_grid_width = Some(selected.chessboard.grid_width);
        diagnostics.chessboard.selected_grid_height = Some(selected.chessboard.grid_height);
        diagnostics.chessboard.selected_grid_completeness = Some(selected.chessboard.completeness);
        diagnostics.chessboard.final_corner_count = selected.chessboard.detection.corners.len();
        diagnostics.candidate_cell_count = selected.candidate_cell_count;
        diagnostics.complete_candidate_cell_count = selected.complete_candidate_cell_count;
        diagnostics.inferred_candidate_cell_count = selected.inferred_candidate_cell_count;
        diagnostics.marker_path = selected.marker_path.clone();
        diagnostics.patch_placement = selected.patch_placement.clone();
        diagnostics.decoded_marker_count = selected.decoded_marker_count;
        diagnostics.aligned_marker_count = selected.aligned_marker_count;
        diagnostics.alignment_inlier_count = selected
            .alignment
            .as_ref()
            .map(|alignment| alignment.marker_inliers.len())
            .unwrap_or(0);
        diagnostics.alignment_candidate_count = selected.alignment_candidate_count;
        diagnostics.alignment_corner_in_bounds_count = selected.alignment_corner_in_bounds_count;
        diagnostics.alignment_corner_in_bounds_ratio = selected.alignment_corner_in_bounds_ratio;
        diagnostics.alignment_runner_up_inlier_count = selected.alignment_runner_up_inlier_count;
        diagnostics.alignment_runner_up_corner_in_bounds_ratio =
            selected.alignment_runner_up_corner_in_bounds_ratio;
        diagnostics.mapped_corner_count_before_validation =
            selected.mapped_corner_count_before_validation;
        diagnostics.final_corner_count = selected.final_corner_count;
        diagnostics.corner_validation = selected.corner_validation.clone();
        diagnostics.timings.decode_ms = selected.decode_ms;
        diagnostics.timings.alignment_ms = selected.alignment_ms;
        diagnostics.timings.map_validate_ms = selected.map_validate_ms;
        diagnostics.timings.total_ms = elapsed_ms(total_start);

        if let Some(failure) = selected.failure {
            return CharucoDetectionRun {
                result: Err(candidate_failure_to_error(failure)),
                diagnostics,
                markers: selected.markers.clone(),
                alignment: selected
                    .alignment
                    .as_ref()
                    .map(|alignment| alignment.alignment),
            };
        }

        CharucoDetectionRun {
            result: Ok(selected.result.clone().expect("success result")),
            diagnostics,
            markers: selected.markers.clone(),
            alignment: selected
                .alignment
                .as_ref()
                .map(|alignment| alignment.alignment),
        }
    }

    #[cfg_attr(feature = "tracing", instrument(level = "info", skip(self, markers), fields(markers=markers.len())))]
    fn select_and_refine_markers(
        &self,
        chessboard: &TargetDetection,
        markers: Vec<MarkerDetection>,
    ) -> AlignmentAttempt {
        select_alignment(&self.board, chessboard, markers)
    }

    fn evaluate_candidate(
        &self,
        image: &GrayImageView<'_>,
        chessboard: ChessboardDetectionResult,
    ) -> CandidateEvaluation {
        let corner_map = build_corner_map(&chessboard.detection.corners, &chessboard.inliers);
        let cell_candidates = build_marker_cell_candidates(&corner_map);
        let complete_candidate_cell_count = cell_candidates
            .iter()
            .filter(|candidate| matches!(candidate.source, MarkerCellSource::CompleteQuad))
            .count();
        let inferred_candidate_cell_count = cell_candidates
            .len()
            .saturating_sub(complete_candidate_cell_count);

        let local_decode_start = Instant::now();
        let cell_evidence = decode_cell_evidence(
            image,
            &cell_candidates,
            self.params.px_per_square,
            &self.params.scan,
            &self.matcher,
            self.params.augmentation.multi_hypothesis_decode,
        );
        let local_markers = dedup_markers_by_id(
            cell_evidence
                .iter()
                .filter_map(|evidence| evidence.selected_marker.clone())
                .collect(),
        );
        let local_decode_ms = elapsed_ms(local_decode_start);
        let base_marker_path = summarize_cell_decode_diagnostics(&cell_evidence);
        let patch_alignment_start = Instant::now();
        let patch_attempt = select_patch_alignment(
            &self.board,
            &chessboard.detection,
            &cell_evidence,
            &self.params.scan,
        );
        let patch_alignment_ms = elapsed_ms(patch_alignment_start);

        let base = CandidateBase {
            chessboard: chessboard.clone(),
            complete_candidate_cell_count,
            inferred_candidate_cell_count,
            marker_path: base_marker_path.clone(),
            patch_placement: patch_attempt.diagnostics.clone(),
            cell_evidence: cell_evidence.clone(),
            decode_ms: local_decode_ms,
        };

        let patch_eval = self.evaluate_patch_placement(
            image,
            base.clone(),
            patch_attempt.selection,
            patch_alignment_ms,
        );
        let local_eval =
            self.evaluate_marker_hypothesis(image, base.clone(), local_markers.clone());
        let local_eval = select_preferred_local_evaluation(local_eval, patch_eval);

        if !self.params.augmentation.rectified_recovery {
            return local_eval;
        }

        let (rectified_markers, rectified_cell_count) = decode_markers_from_rectified_view(
            image,
            &chessboard,
            &corner_map,
            self.params.px_per_square,
            &self.params.scan,
            &self.matcher,
        );
        if rectified_markers.is_empty() {
            return local_eval;
        }

        let augmented_markers =
            dedup_markers_by_id(local_markers.into_iter().chain(rectified_markers).collect());
        let augmented_eval = self.evaluate_marker_hypothesis(
            image,
            CandidateBase {
                chessboard,
                complete_candidate_cell_count: complete_candidate_cell_count + rectified_cell_count,
                inferred_candidate_cell_count,
                marker_path: base_marker_path,
                patch_placement: PatchPlacementDiagnostics::default(),
                cell_evidence,
                decode_ms: local_decode_ms,
            },
            augmented_markers,
        );
        let mut augmented_eval = augmented_eval;
        augmented_eval.marker_path.covers_selected_evaluation = false;
        augmented_eval.patch_placement.covers_selected_evaluation = false;

        match compare_evaluations(&local_eval, &augmented_eval) {
            std::cmp::Ordering::Less => augmented_eval,
            _ => local_eval,
        }
    }

    fn evaluate_marker_hypothesis(
        &self,
        image: &GrayImageView<'_>,
        base: CandidateBase,
        markers: Vec<MarkerDetection>,
    ) -> CandidateEvaluation {
        let ctx = base.into_context(markers.len());
        if markers.is_empty() {
            return CandidateEvaluation {
                chessboard: ctx.chessboard,
                candidate_cell_count: ctx.candidate_cell_count,
                complete_candidate_cell_count: ctx.complete_candidate_cell_count,
                inferred_candidate_cell_count: ctx.inferred_candidate_cell_count,
                marker_path: ctx.marker_path,
                patch_placement: ctx.patch_placement,
                decoded_marker_count: ctx.decoded_marker_count,
                aligned_marker_count: 0,
                alignment_candidate_count: 0,
                alignment_corner_in_bounds_count: 0,
                alignment_corner_in_bounds_ratio: 0.0,
                alignment_runner_up_inlier_count: 0,
                alignment_runner_up_corner_in_bounds_ratio: 0.0,
                markers,
                alignment: None,
                mapped_corner_count_before_validation: 0,
                final_corner_count: 0,
                corner_validation: None,
                result: None,
                decode_ms: ctx.decode_ms,
                alignment_ms: 0.0,
                map_validate_ms: 0.0,
                failure: Some(CandidateFailure::NoMarkers),
            };
        }

        let alignment_start = Instant::now();
        let decoded_markers = markers.clone();
        let alignment_attempt = self.select_and_refine_markers(&ctx.chessboard.detection, markers);
        let Some(selection) = alignment_attempt.selection else {
            return CandidateEvaluation {
                chessboard: ctx.chessboard,
                candidate_cell_count: ctx.candidate_cell_count,
                complete_candidate_cell_count: ctx.complete_candidate_cell_count,
                inferred_candidate_cell_count: ctx.inferred_candidate_cell_count,
                marker_path: ctx.marker_path,
                patch_placement: ctx.patch_placement,
                decoded_marker_count: ctx.decoded_marker_count,
                aligned_marker_count: 0,
                alignment_candidate_count: alignment_attempt.candidate_count,
                alignment_corner_in_bounds_count: 0,
                alignment_corner_in_bounds_ratio: 0.0,
                alignment_runner_up_inlier_count: 0,
                alignment_runner_up_corner_in_bounds_ratio: 0.0,
                markers: decoded_markers,
                alignment: None,
                mapped_corner_count_before_validation: 0,
                final_corner_count: 0,
                corner_validation: None,
                result: None,
                decode_ms: ctx.decode_ms,
                alignment_ms: elapsed_ms(alignment_start),
                map_validate_ms: 0.0,
                failure: Some(CandidateFailure::AlignmentFailed { inliers: 0 }),
            };
        };
        let alignment_ms = elapsed_ms(alignment_start);
        let inliers = selection.alignment.marker_inliers.len();
        if !alignment_has_sufficient_support(
            &selection,
            self.params.min_marker_inliers,
            self.params.allow_low_inlier_unique_alignment,
        ) {
            return self.failed_alignment_evaluation(ctx, selection, alignment_ms, inliers);
        }

        self.finish_candidate_evaluation(image, ctx, selection, alignment_ms)
    }

    fn evaluate_patch_placement(
        &self,
        image: &GrayImageView<'_>,
        base: CandidateBase,
        selection: Option<AlignmentSelection>,
        alignment_ms: f64,
    ) -> Option<CandidateEvaluation> {
        let selection = selection?;
        if !alignment_has_sufficient_support(
            &selection,
            self.params.min_marker_inliers,
            self.params.allow_low_inlier_unique_alignment,
        ) {
            return None;
        }

        let mut ctx = base.into_context(selection.markers.len());
        ctx.patch_placement.covers_selected_evaluation = true;
        Some(self.finish_candidate_evaluation(image, ctx, selection, alignment_ms))
    }

    fn finish_candidate_evaluation(
        &self,
        image: &GrayImageView<'_>,
        ctx: CandidateContext,
        selection: AlignmentSelection,
        alignment_ms: f64,
    ) -> CandidateEvaluation {
        let CandidateContext {
            chessboard,
            candidate_cell_count,
            complete_candidate_cell_count,
            inferred_candidate_cell_count,
            mut marker_path,
            patch_placement,
            cell_evidence,
            decoded_marker_count,
            decode_ms,
        } = ctx;
        let AlignmentSelection {
            markers,
            alignment,
            candidate_count,
            corner_in_bounds_count,
            corner_in_bounds_ratio,
            runner_up_inlier_count,
            runner_up_corner_in_bounds_ratio,
        } = selection;
        let aligned_marker_count = markers.len();
        add_alignment_match_diagnostics(
            &self.board,
            &cell_evidence,
            alignment.alignment,
            &mut marker_path,
        );

        let map_validate_start = Instant::now();
        let mapped = map_charuco_corners(&self.board, &chessboard.detection, &alignment);
        let mapped_corner_count_before_validation = mapped.corners.len();
        let validation = self.run_corner_validation(mapped, &markers, &alignment, image);
        let map_validate_ms = elapsed_ms(map_validate_start);

        let result = CharucoDetectionResult {
            detection: validation.detection.clone(),
            markers: markers.clone(),
            alignment: alignment.alignment,
        };

        CandidateEvaluation {
            chessboard,
            candidate_cell_count,
            complete_candidate_cell_count,
            inferred_candidate_cell_count,
            marker_path,
            patch_placement,
            decoded_marker_count,
            aligned_marker_count,
            alignment_candidate_count: candidate_count,
            alignment_corner_in_bounds_count: corner_in_bounds_count,
            alignment_corner_in_bounds_ratio: corner_in_bounds_ratio,
            alignment_runner_up_inlier_count: runner_up_inlier_count,
            alignment_runner_up_corner_in_bounds_ratio: runner_up_corner_in_bounds_ratio,
            markers,
            alignment: Some(alignment),
            mapped_corner_count_before_validation,
            final_corner_count: validation.detection.corners.len(),
            corner_validation: Some(validation.diagnostics),
            result: Some(result),
            decode_ms,
            alignment_ms,
            map_validate_ms,
            failure: None,
        }
    }

    fn failed_alignment_evaluation(
        &self,
        ctx: CandidateContext,
        selection: AlignmentSelection,
        alignment_ms: f64,
        inliers: usize,
    ) -> CandidateEvaluation {
        let CandidateContext {
            chessboard,
            candidate_cell_count,
            complete_candidate_cell_count,
            inferred_candidate_cell_count,
            mut marker_path,
            patch_placement,
            cell_evidence,
            decoded_marker_count,
            decode_ms,
        } = ctx;
        let AlignmentSelection {
            markers,
            alignment,
            candidate_count,
            corner_in_bounds_count,
            corner_in_bounds_ratio,
            runner_up_inlier_count,
            runner_up_corner_in_bounds_ratio,
        } = selection;
        let aligned_marker_count = markers.len();
        add_alignment_match_diagnostics(
            &self.board,
            &cell_evidence,
            alignment.alignment,
            &mut marker_path,
        );

        CandidateEvaluation {
            chessboard,
            candidate_cell_count,
            complete_candidate_cell_count,
            inferred_candidate_cell_count,
            marker_path,
            patch_placement,
            decoded_marker_count,
            aligned_marker_count,
            alignment_candidate_count: candidate_count,
            alignment_corner_in_bounds_count: corner_in_bounds_count,
            alignment_corner_in_bounds_ratio: corner_in_bounds_ratio,
            alignment_runner_up_inlier_count: runner_up_inlier_count,
            alignment_runner_up_corner_in_bounds_ratio: runner_up_corner_in_bounds_ratio,
            markers,
            alignment: Some(alignment),
            mapped_corner_count_before_validation: 0,
            final_corner_count: 0,
            corner_validation: None,
            result: None,
            decode_ms,
            alignment_ms,
            map_validate_ms: 0.0,
            failure: Some(CandidateFailure::AlignmentFailed { inliers }),
        }
    }

    fn run_corner_validation(
        &self,
        detection: calib_targets_core::TargetDetection,
        markers: &[MarkerDetection],
        alignment: &CharucoAlignment,
        image: &GrayImageView<'_>,
    ) -> CornerValidationRun {
        let threshold_rel = if self.params.use_global_corner_validation {
            self.params.corner_validation_threshold_rel
        } else {
            f32::INFINITY
        };
        validate_and_fix_corners(
            detection,
            &self.board,
            markers,
            alignment,
            image,
            &CornerValidationConfig {
                px_per_square: self.params.px_per_square,
                threshold_rel,
                chess_params: &self.params.corner_redetect_params,
            },
        )
    }
}

fn select_preferred_local_evaluation(
    local_eval: CandidateEvaluation,
    patch_eval: Option<CandidateEvaluation>,
) -> CandidateEvaluation {
    let Some(patch_eval) = patch_eval else {
        return local_eval;
    };

    match (local_eval.failure.is_none(), patch_eval.failure.is_none()) {
        (false, true) => patch_eval,
        (true, false) => local_eval,
        (true, true) => {
            let patch_improves = (patch_eval.final_corner_count > local_eval.final_corner_count
                && patch_eval.markers.len() >= local_eval.markers.len())
                || (patch_eval.final_corner_count == local_eval.final_corner_count
                    && patch_eval.markers.len() > local_eval.markers.len());
            if patch_improves {
                patch_eval
            } else {
                local_eval
            }
        }
        (false, false) => match compare_evaluations(&local_eval, &patch_eval) {
            std::cmp::Ordering::Less => patch_eval,
            _ => local_eval,
        },
    }
}

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1_000.0
}
