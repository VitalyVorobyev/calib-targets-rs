use super::alignment_select::select_alignment;
use super::corner_mapping::map_charuco_corners;
use super::corner_validation::{validate_and_fix_corners, CornerValidationConfig};
use super::marker_sampling::{
    build_corner_map, build_marker_cell_candidates, CornerMap, MarkerCellSource, SampledMarkerCell,
};
use super::{
    CharucoDetectError, CharucoDetectionResult, CharucoDetectionRun, CharucoDetectorParams,
    CharucoDiagnostics,
};
use crate::alignment::CharucoAlignment;
use crate::board::{CharucoBoard, CharucoBoardError};
use calib_targets_aruco::{
    decode_marker_in_cell, scan_decode_markers, GridCell, MarkerDetection, Matcher,
};
use calib_targets_chessboard::{
    rectify_from_chessboard_result, ChessboardDetectionResult, ChessboardDetector,
};
use calib_targets_core::{
    Corner, GrayImageView, GridAlignment, GridCoords, TargetDetection, GRID_TRANSFORMS_D4,
};
use nalgebra::Point2;
use std::cmp::Ordering;
use std::collections::HashMap;
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

struct CandidateEvaluation {
    chessboard: ChessboardDetectionResult,
    candidate_cell_count: usize,
    complete_candidate_cell_count: usize,
    inferred_candidate_cell_count: usize,
    decoded_marker_count: usize,
    aligned_marker_count: usize,
    alignment_candidate_count: usize,
    alignment_corner_in_bounds_count: usize,
    alignment_corner_in_bounds_ratio: f32,
    alignment_runner_up_inlier_count: usize,
    alignment_runner_up_corner_in_bounds_ratio: f32,
    markers: Vec<MarkerDetection>,
    alignment: Option<CharucoAlignment>,
    mapped_corner_count_before_validation: usize,
    final_corner_count: usize,
    corner_validation: Option<super::corner_validation::CornerValidationDiagnostics>,
    result: Option<CharucoDetectionResult>,
    decode_ms: f64,
    alignment_ms: f64,
    map_validate_ms: f64,
    failure: Option<CandidateFailure>,
}

#[derive(Clone)]
struct CellDecodeEvidence {
    candidate: SampledMarkerCell,
    selected_marker: Option<MarkerDetection>,
    hypothesis_detections: Vec<(usize, MarkerDetection)>,
}

#[derive(Clone)]
struct PlacementSelectionCandidate {
    alignment: GridAlignment,
    markers: Vec<MarkerDetection>,
    matched_count: usize,
    contradiction_count: usize,
    score_sum: f32,
    corner_in_bounds_count: usize,
    corner_in_bounds_ratio: f32,
}

#[derive(Clone, Copy)]
enum CandidateFailure {
    NoMarkers,
    AlignmentFailed { inliers: usize },
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

    #[cfg_attr(feature = "tracing", instrument(level = "info", skip(self, markers),
      fields(markers=markers.len())))]
    fn select_and_refine_markers(
        &self,
        chessboard: &calib_targets_core::TargetDetection,
        markers: Vec<MarkerDetection>,
    ) -> super::alignment_select::AlignmentAttempt {
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
        let cell_evidence = self.decode_cell_evidence(image, &cell_candidates);
        let local_markers = dedup_markers_by_id(
            cell_evidence
                .iter()
                .filter_map(|evidence| evidence.selected_marker.clone())
                .collect(),
        );
        let local_decode_ms = elapsed_ms(local_decode_start);
        let patch_eval = self.evaluate_patch_placement(
            image,
            chessboard.clone(),
            complete_candidate_cell_count,
            inferred_candidate_cell_count,
            &cell_evidence,
            local_decode_ms,
        );
        let local_eval = self.evaluate_marker_hypothesis(
            image,
            chessboard.clone(),
            complete_candidate_cell_count,
            inferred_candidate_cell_count,
            local_markers.clone(),
            local_decode_ms,
        );
        let local_eval = if let Some(patch_eval) = patch_eval {
            match (local_eval.failure.is_none(), patch_eval.failure.is_none()) {
                (false, true) => patch_eval,
                (true, false) => local_eval,
                (true, true) => {
                    let patch_improves = (patch_eval.final_corner_count
                        > local_eval.final_corner_count
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
                    Ordering::Less => patch_eval,
                    _ => local_eval,
                },
            }
        } else {
            local_eval
        };

        let (rectified_markers, rectified_cell_count) =
            self.decode_markers_from_rectified_view(image, &chessboard, &corner_map);
        if rectified_markers.is_empty() {
            return local_eval;
        }

        let augmented_markers =
            dedup_markers_by_id(local_markers.into_iter().chain(rectified_markers).collect());
        let augmented_eval = self.evaluate_marker_hypothesis(
            image,
            chessboard,
            complete_candidate_cell_count + rectified_cell_count,
            inferred_candidate_cell_count,
            augmented_markers,
            local_decode_ms,
        );

        match compare_evaluations(&local_eval, &augmented_eval) {
            Ordering::Less => augmented_eval,
            _ => local_eval,
        }
    }

    fn evaluate_marker_hypothesis(
        &self,
        image: &GrayImageView<'_>,
        chessboard: ChessboardDetectionResult,
        complete_candidate_cell_count: usize,
        inferred_candidate_cell_count: usize,
        markers: Vec<MarkerDetection>,
        decode_ms: f64,
    ) -> CandidateEvaluation {
        let candidate_cell_count = complete_candidate_cell_count + inferred_candidate_cell_count;
        let decoded_marker_count = markers.len();
        if markers.is_empty() {
            return CandidateEvaluation {
                chessboard,
                candidate_cell_count,
                complete_candidate_cell_count,
                inferred_candidate_cell_count,
                decoded_marker_count,
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
                decode_ms,
                alignment_ms: 0.0,
                map_validate_ms: 0.0,
                failure: Some(CandidateFailure::NoMarkers),
            };
        }

        let alignment_start = Instant::now();
        let decoded_markers = markers.clone();
        let alignment_attempt = self.select_and_refine_markers(&chessboard.detection, markers);
        let Some(selection) = alignment_attempt.selection else {
            return CandidateEvaluation {
                chessboard,
                candidate_cell_count,
                complete_candidate_cell_count,
                inferred_candidate_cell_count,
                decoded_marker_count,
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
                decode_ms,
                alignment_ms: elapsed_ms(alignment_start),
                map_validate_ms: 0.0,
                failure: Some(CandidateFailure::AlignmentFailed { inliers: 0 }),
            };
        };
        let alignment_ms = elapsed_ms(alignment_start);
        let alignment_supported =
            alignment_has_sufficient_support(&selection, self.params.min_marker_inliers);
        let alignment = selection.alignment;
        let markers = selection.markers;
        let inliers = alignment.marker_inliers.len();
        let aligned_marker_count = markers.len();
        if !alignment_supported {
            return CandidateEvaluation {
                chessboard,
                candidate_cell_count,
                complete_candidate_cell_count,
                inferred_candidate_cell_count,
                decoded_marker_count,
                aligned_marker_count,
                alignment_candidate_count: selection.candidate_count,
                alignment_corner_in_bounds_count: selection.corner_in_bounds_count,
                alignment_corner_in_bounds_ratio: selection.corner_in_bounds_ratio,
                alignment_runner_up_inlier_count: selection.runner_up_inlier_count,
                alignment_runner_up_corner_in_bounds_ratio: selection
                    .runner_up_corner_in_bounds_ratio,
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
            };
        }

        let map_validate_start = Instant::now();
        let mapped = map_charuco_corners(&self.board, &chessboard.detection, &alignment);
        let mapped_corner_count_before_validation = mapped.corners.len();
        let validation = validate_and_fix_corners(
            mapped,
            &self.board,
            &markers,
            &alignment,
            image,
            &CornerValidationConfig {
                px_per_square: self.params.px_per_square,
                threshold_rel: self.params.corner_validation_threshold_rel,
                chess_params: &self.params.corner_redetect_params,
            },
        );
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
            decoded_marker_count,
            aligned_marker_count,
            alignment_candidate_count: selection.candidate_count,
            alignment_corner_in_bounds_count: selection.corner_in_bounds_count,
            alignment_corner_in_bounds_ratio: selection.corner_in_bounds_ratio,
            alignment_runner_up_inlier_count: selection.runner_up_inlier_count,
            alignment_runner_up_corner_in_bounds_ratio: selection.runner_up_corner_in_bounds_ratio,
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

    fn evaluate_patch_placement(
        &self,
        image: &GrayImageView<'_>,
        chessboard: ChessboardDetectionResult,
        complete_candidate_cell_count: usize,
        inferred_candidate_cell_count: usize,
        cell_evidence: &[CellDecodeEvidence],
        decode_ms: f64,
    ) -> Option<CandidateEvaluation> {
        let alignment_start = Instant::now();
        let selection = self.select_patch_alignment(&chessboard.detection, cell_evidence)?;
        let alignment_ms = elapsed_ms(alignment_start);
        if !alignment_has_sufficient_support(&selection, self.params.min_marker_inliers) {
            return None;
        }

        let alignment = selection.alignment;
        let markers = selection.markers;
        let decoded_marker_count = markers.len();
        let aligned_marker_count = markers.len();
        let candidate_cell_count = complete_candidate_cell_count + inferred_candidate_cell_count;

        let map_validate_start = Instant::now();
        let mapped = map_charuco_corners(&self.board, &chessboard.detection, &alignment);
        let mapped_corner_count_before_validation = mapped.corners.len();
        let validation = validate_and_fix_corners(
            mapped,
            &self.board,
            &markers,
            &alignment,
            image,
            &CornerValidationConfig {
                px_per_square: self.params.px_per_square,
                threshold_rel: self.params.corner_validation_threshold_rel,
                chess_params: &self.params.corner_redetect_params,
            },
        );
        let map_validate_ms = elapsed_ms(map_validate_start);

        let result = CharucoDetectionResult {
            detection: validation.detection.clone(),
            markers: markers.clone(),
            alignment: alignment.alignment,
        };

        Some(CandidateEvaluation {
            chessboard,
            candidate_cell_count,
            complete_candidate_cell_count,
            inferred_candidate_cell_count,
            decoded_marker_count,
            aligned_marker_count,
            alignment_candidate_count: selection.candidate_count,
            alignment_corner_in_bounds_count: selection.corner_in_bounds_count,
            alignment_corner_in_bounds_ratio: selection.corner_in_bounds_ratio,
            alignment_runner_up_inlier_count: selection.runner_up_inlier_count,
            alignment_runner_up_corner_in_bounds_ratio: selection.runner_up_corner_in_bounds_ratio,
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
        })
    }

    fn select_patch_alignment(
        &self,
        chessboard: &TargetDetection,
        cell_evidence: &[CellDecodeEvidence],
    ) -> Option<super::alignment_select::AlignmentSelection> {
        let mut candidates = enumerate_legal_patch_alignments(&self.board, chessboard)
            .into_iter()
            .filter_map(|alignment| {
                self.evaluate_patch_alignment_candidate(chessboard, cell_evidence, alignment)
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return None;
        }

        candidates.sort_by(|a, b| compare_patch_selection_candidates(b, a));
        let best = candidates.first()?.clone();
        let runner_up = candidates.get(1);
        if runner_up.is_some_and(|runner_up| {
            compare_patch_selection_candidates(&best, runner_up) == Ordering::Equal
                && runner_up.alignment != best.alignment
        }) {
            return None;
        }

        let marker_inliers = (0..best.markers.len()).collect();
        Some(super::alignment_select::AlignmentSelection {
            markers: best.markers.clone(),
            alignment: CharucoAlignment {
                alignment: best.alignment,
                marker_inliers,
            },
            candidate_count: candidates.len(),
            corner_in_bounds_count: best.corner_in_bounds_count,
            corner_in_bounds_ratio: best.corner_in_bounds_ratio,
            runner_up_inlier_count: runner_up
                .map(|candidate| candidate.matched_count)
                .unwrap_or(0),
            runner_up_corner_in_bounds_ratio: runner_up
                .map(|candidate| candidate.corner_in_bounds_ratio)
                .unwrap_or(0.0),
        })
    }

    fn evaluate_patch_alignment_candidate(
        &self,
        chessboard: &TargetDetection,
        cell_evidence: &[CellDecodeEvidence],
        alignment: GridAlignment,
    ) -> Option<PlacementSelectionCandidate> {
        let (corner_in_bounds_count, corner_in_bounds_ratio) =
            alignment_corner_fit(&self.board, chessboard, alignment);
        if corner_in_bounds_count == 0 {
            return None;
        }

        let mut matched_markers = Vec::new();
        let mut contradiction_count = 0usize;

        for evidence in cell_evidence {
            let [sx, sy] =
                alignment.map(evidence.candidate.cell.gc.gx, evidence.candidate.cell.gc.gy);
            let expected_id = self.board.marker_id_at_cell(sx, sy);
            match expected_id {
                Some(expected_id) => {
                    if let Some(marker) = match_expected_marker_from_hypotheses(
                        evidence.candidate.source,
                        expected_id,
                        &evidence.hypothesis_detections,
                        &self.params.scan,
                    ) {
                        matched_markers.push(marker);
                    } else if cell_has_confident_wrong_decode(
                        evidence,
                        Some(expected_id),
                        &self.params.scan,
                    ) {
                        contradiction_count += 1;
                    }
                }
                None => {
                    if cell_has_confident_wrong_decode(evidence, None, &self.params.scan) {
                        contradiction_count += 1;
                    }
                }
            }
        }

        let matched_markers = dedup_markers_by_id(matched_markers);
        if matched_markers.is_empty() {
            return None;
        }

        let score_sum = matched_markers.iter().map(|marker| marker.score).sum();
        Some(PlacementSelectionCandidate {
            alignment,
            matched_count: matched_markers.len(),
            contradiction_count,
            score_sum,
            markers: matched_markers,
            corner_in_bounds_count,
            corner_in_bounds_ratio,
        })
    }

    fn decode_cell_evidence(
        &self,
        image: &GrayImageView<'_>,
        cell_candidates: &[SampledMarkerCell],
    ) -> Vec<CellDecodeEvidence> {
        let scan_hypotheses = marker_scan_hypotheses(&self.params.scan);
        let mut evidence = Vec::with_capacity(cell_candidates.len());

        for candidate in cell_candidates {
            let mut hypothesis_detections = Vec::new();
            for (hypothesis_idx, scan_cfg) in scan_hypotheses.iter().enumerate() {
                let Some(marker) = decode_marker_in_cell(
                    image,
                    &candidate.cell,
                    self.params.px_per_square,
                    scan_cfg,
                    &self.matcher,
                ) else {
                    continue;
                };
                hypothesis_detections.push((hypothesis_idx, marker));
            }

            let selected_marker = select_marker_from_scan_hypotheses(
                candidate.source,
                &hypothesis_detections,
                &self.params.scan,
            );
            evidence.push(CellDecodeEvidence {
                candidate: candidate.clone(),
                selected_marker,
                hypothesis_detections,
            });
        }

        evidence
    }

    fn decode_markers_from_rectified_view(
        &self,
        image: &GrayImageView<'_>,
        chessboard: &ChessboardDetectionResult,
        corner_map: &CornerMap,
    ) -> (Vec<MarkerDetection>, usize) {
        let Ok(rectified) = rectify_from_chessboard_result(
            image,
            &chessboard.detection.corners,
            &chessboard.inliers,
            self.params.px_per_square,
            0.0,
        ) else {
            return (Vec::new(), 0);
        };

        let cells_x = (rectified.max_i - rectified.min_i).max(0) as usize;
        let cells_y = (rectified.max_j - rectified.min_j).max(0) as usize;
        if cells_x == 0 || cells_y == 0 {
            return (Vec::new(), 0);
        }

        let supported_cells = count_rectified_supported_cells(&rectified, corner_map);
        if supported_cells.is_empty() {
            return (Vec::new(), 0);
        }

        let supported_lookup: HashMap<(i32, i32), usize> = supported_cells
            .iter()
            .map(|&(gx, gy, support)| ((gx, gy), support))
            .collect();

        let mut decoded = Vec::new();
        for mut marker in scan_decode_markers(
            &rectified.rect.view(),
            cells_x,
            cells_y,
            rectified.px_per_square,
            &self.params.scan,
            &self.matcher,
        ) {
            let gx = marker.gc.gx + rectified.min_i;
            let gy = marker.gc.gy + rectified.min_j;
            let Some(&support) = supported_lookup.get(&(gx, gy)) else {
                continue;
            };
            if support != 2 {
                continue;
            }
            marker.gc = GridCell { gx, gy };
            marker.corners_img = Some(rectified_cell_corners_img(
                &rectified,
                marker.gc.gx - rectified.min_i,
                marker.gc.gy - rectified.min_j,
            ));
            decoded.push(marker);
        }

        let extra_supported_cells = supported_cells
            .iter()
            .filter(|(_, _, support)| *support == 2)
            .count();
        (decoded, extra_supported_cells)
    }
}

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1_000.0
}

fn select_best_evaluation(evaluations: &[CandidateEvaluation]) -> Option<&CandidateEvaluation> {
    evaluations.iter().max_by(|a, b| compare_evaluations(a, b))
}

fn compare_evaluations(a: &CandidateEvaluation, b: &CandidateEvaluation) -> Ordering {
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

fn candidate_failure_to_error(failure: CandidateFailure) -> CharucoDetectError {
    match failure {
        CandidateFailure::NoMarkers => CharucoDetectError::NoMarkers,
        CandidateFailure::AlignmentFailed { inliers } => {
            CharucoDetectError::AlignmentFailed { inliers }
        }
    }
}

fn inferred_marker_is_reliable(
    marker: &MarkerDetection,
    scan: &calib_targets_aruco::ScanDecodeConfig,
) -> bool {
    marker.hamming == 0
        && marker.score >= 0.92
        && marker.border_score >= scan.min_border_score.max(0.92)
}

fn alignment_has_sufficient_support(
    selection: &super::alignment_select::AlignmentSelection,
    min_marker_inliers: usize,
) -> bool {
    let inliers = selection.alignment.marker_inliers.len();
    if inliers >= min_marker_inliers {
        return true;
    }

    inliers >= 4
        && selection.corner_in_bounds_ratio >= 0.95
        && selection.runner_up_inlier_count + 2 <= inliers
}

fn compare_patch_selection_candidates(
    a: &PlacementSelectionCandidate,
    b: &PlacementSelectionCandidate,
) -> Ordering {
    a.matched_count
        .cmp(&b.matched_count)
        .then_with(|| b.contradiction_count.cmp(&a.contradiction_count))
        .then_with(|| {
            a.score_sum
                .partial_cmp(&b.score_sum)
                .unwrap_or(Ordering::Equal)
        })
        .then_with(|| {
            a.corner_in_bounds_ratio
                .partial_cmp(&b.corner_in_bounds_ratio)
                .unwrap_or(Ordering::Equal)
        })
}

fn alignment_corner_fit(
    board: &CharucoBoard,
    chessboard: &TargetDetection,
    alignment: GridAlignment,
) -> (usize, f32) {
    let mut total = 0usize;
    let mut in_bounds = 0usize;
    for corner in &chessboard.corners {
        let Some(grid) = corner.grid else {
            continue;
        };
        total += 1;
        let [bi, bj] = alignment.map(grid.i, grid.j);
        if board.charuco_corner_id_from_board_corner(bi, bj).is_some() {
            in_bounds += 1;
        }
    }
    let ratio = if total == 0 {
        0.0
    } else {
        in_bounds as f32 / total as f32
    };
    (in_bounds, ratio)
}

fn enumerate_legal_patch_alignments(
    board: &CharucoBoard,
    chessboard: &TargetDetection,
) -> Vec<GridAlignment> {
    let Some((min_i, max_i, min_j, max_j)) = chessboard_grid_bounds(chessboard) else {
        return Vec::new();
    };
    let inner_cols = board.expected_inner_cols() as i32;
    let inner_rows = board.expected_inner_rows() as i32;
    let bbox = [
        (min_i, min_j),
        (max_i, min_j),
        (max_i, max_j),
        (min_i, max_j),
    ];

    let mut alignments = Vec::new();
    for transform in GRID_TRANSFORMS_D4 {
        let transformed = bbox.map(|(i, j)| transform.apply(i, j));
        let min_x = transformed.iter().map(|p| p[0]).min().unwrap_or(0);
        let max_x = transformed.iter().map(|p| p[0]).max().unwrap_or(0);
        let min_y = transformed.iter().map(|p| p[1]).min().unwrap_or(0);
        let max_y = transformed.iter().map(|p| p[1]).max().unwrap_or(0);

        let tx_min = 1 - min_x;
        let tx_max = inner_cols - max_x;
        let ty_min = 1 - min_y;
        let ty_max = inner_rows - max_y;
        if tx_min > tx_max || ty_min > ty_max {
            continue;
        }

        for tx in tx_min..=tx_max {
            for ty in ty_min..=ty_max {
                alignments.push(GridAlignment {
                    transform,
                    translation: [tx, ty],
                });
            }
        }
    }
    alignments
}

fn chessboard_grid_bounds(chessboard: &TargetDetection) -> Option<(i32, i32, i32, i32)> {
    let mut min_i = i32::MAX;
    let mut max_i = i32::MIN;
    let mut min_j = i32::MAX;
    let mut max_j = i32::MIN;

    for corner in &chessboard.corners {
        let Some(grid) = corner.grid else {
            continue;
        };
        min_i = min_i.min(grid.i);
        max_i = max_i.max(grid.i);
        min_j = min_j.min(grid.j);
        max_j = max_j.max(grid.j);
    }

    (min_i != i32::MAX).then_some((min_i, max_i, min_j, max_j))
}

fn match_expected_marker_from_hypotheses(
    source: MarkerCellSource,
    expected_id: u32,
    hypothesis_detections: &[(usize, MarkerDetection)],
    base_scan: &calib_targets_aruco::ScanDecodeConfig,
) -> Option<MarkerDetection> {
    let base_detection = hypothesis_detections
        .iter()
        .find(|(hypothesis_idx, marker)| *hypothesis_idx == 0 && marker.id == expected_id)
        .map(|(_, marker)| marker.clone());
    if let Some(marker) = base_detection {
        return marker_allowed_for_source(source, &marker, base_scan, false).then_some(marker);
    }

    let matching: Vec<&MarkerDetection> = hypothesis_detections
        .iter()
        .filter(|(_, marker)| marker.id == expected_id)
        .map(|(_, marker)| marker)
        .collect();
    if matching.len() < 2 {
        return None;
    }

    let marker = best_marker_from_group(&matching).clone();
    marker_allowed_for_source(source, &marker, base_scan, true).then_some(marker)
}

fn cell_has_confident_wrong_decode(
    evidence: &CellDecodeEvidence,
    expected_id: Option<u32>,
    base_scan: &calib_targets_aruco::ScanDecodeConfig,
) -> bool {
    evidence.selected_marker.as_ref().is_some_and(|marker| {
        (match expected_id {
            Some(expected_id) => marker.id != expected_id,
            None => true,
        }) && marker_allowed_for_source(evidence.candidate.source, marker, base_scan, false)
    })
}

fn marker_scan_hypotheses(
    base: &calib_targets_aruco::ScanDecodeConfig,
) -> Vec<calib_targets_aruco::ScanDecodeConfig> {
    let mut hypotheses = Vec::with_capacity(3);
    hypotheses.push(base.clone());

    let mut tighter = base.clone();
    tighter.marker_size_rel = (base.marker_size_rel + 0.06).clamp(0.1, 1.0);
    tighter.inset_frac = (base.inset_frac - 0.025).clamp(0.01, 0.20);
    push_unique_scan_hypothesis(&mut hypotheses, tighter);

    let mut looser = base.clone();
    looser.marker_size_rel = (base.marker_size_rel - 0.06).clamp(0.1, 1.0);
    looser.inset_frac = (base.inset_frac + 0.03).clamp(0.01, 0.20);
    push_unique_scan_hypothesis(&mut hypotheses, looser);

    hypotheses
}

fn push_unique_scan_hypothesis(
    hypotheses: &mut Vec<calib_targets_aruco::ScanDecodeConfig>,
    candidate: calib_targets_aruco::ScanDecodeConfig,
) {
    let exists = hypotheses.iter().any(|existing| {
        existing.border_bits == candidate.border_bits
            && existing.dedup_by_id == candidate.dedup_by_id
            && (existing.inset_frac - candidate.inset_frac).abs() <= 1e-6
            && (existing.marker_size_rel - candidate.marker_size_rel).abs() <= 1e-6
            && (existing.min_border_score - candidate.min_border_score).abs() <= 1e-6
    });
    if !exists {
        hypotheses.push(candidate);
    }
}

fn select_marker_from_scan_hypotheses(
    source: MarkerCellSource,
    hypothesis_detections: &[(usize, MarkerDetection)],
    base_scan: &calib_targets_aruco::ScanDecodeConfig,
) -> Option<MarkerDetection> {
    let base_detection = hypothesis_detections
        .iter()
        .find(|(hypothesis_idx, _)| *hypothesis_idx == 0)
        .map(|(_, marker)| marker.clone());

    if let Some(marker) = base_detection {
        return marker_allowed_for_source(source, &marker, base_scan, false).then_some(marker);
    }

    let mut groups: HashMap<(u32, i32, i32, u8), Vec<&MarkerDetection>> = HashMap::new();
    for (_, marker) in hypothesis_detections {
        groups
            .entry((marker.id, marker.gc.gx, marker.gc.gy, marker.rotation))
            .or_default()
            .push(marker);
    }

    let best_group = groups
        .into_values()
        .filter(|group| group.len() >= 2)
        .max_by(|a, b| {
            a.len().cmp(&b.len()).then_with(|| {
                best_marker_from_group(a)
                    .score
                    .partial_cmp(&best_marker_from_group(b).score)
                    .unwrap_or(Ordering::Equal)
            })
        })?;
    let marker = best_marker_from_group(&best_group).clone();
    marker_allowed_for_source(source, &marker, base_scan, true).then_some(marker)
}

fn best_marker_from_group<'a>(group: &'a [&'a MarkerDetection]) -> &'a MarkerDetection {
    group
        .iter()
        .copied()
        .max_by(|a, b| {
            a.score
                .partial_cmp(&b.score)
                .unwrap_or(Ordering::Equal)
                .then_with(|| {
                    a.border_score
                        .partial_cmp(&b.border_score)
                        .unwrap_or(Ordering::Equal)
                })
        })
        .expect("marker group should be non-empty")
}

fn marker_allowed_for_source(
    source: MarkerCellSource,
    marker: &MarkerDetection,
    base_scan: &calib_targets_aruco::ScanDecodeConfig,
    from_consensus: bool,
) -> bool {
    match source {
        MarkerCellSource::CompleteQuad => {
            !from_consensus
                || (marker.hamming == 0
                    && marker.border_score >= base_scan.min_border_score.max(0.88))
        }
        MarkerCellSource::InferredThreeCorners { .. } => {
            inferred_marker_is_reliable(marker, base_scan)
        }
    }
}

fn dedup_markers_by_id(markers: Vec<MarkerDetection>) -> Vec<MarkerDetection> {
    let mut best: HashMap<u32, MarkerDetection> = HashMap::new();
    for marker in markers {
        match best.get(&marker.id) {
            None => {
                best.insert(marker.id, marker);
            }
            Some(current) if marker.score > current.score => {
                best.insert(marker.id, marker);
            }
            _ => {}
        }
    }

    let mut deduped: Vec<MarkerDetection> = best.into_values().collect();
    deduped.sort_by_key(|marker| marker.id);
    deduped
}

fn count_rectified_supported_cells(
    rectified: &calib_targets_chessboard::RectifiedBoardView,
    corner_map: &CornerMap,
) -> Vec<(i32, i32, usize)> {
    let mut out = Vec::new();
    for gy in rectified.min_j..rectified.max_j {
        for gx in rectified.min_i..rectified.max_i {
            let support = cell_support_count(corner_map, gx, gy);
            if support >= 2 {
                out.push((gx, gy, support));
            }
        }
    }
    out
}

fn cell_support_count(corner_map: &CornerMap, gx: i32, gy: i32) -> usize {
    let corners = [
        GridCoords { i: gx, j: gy },
        GridCoords { i: gx + 1, j: gy },
        GridCoords {
            i: gx + 1,
            j: gy + 1,
        },
        GridCoords { i: gx, j: gy + 1 },
    ];
    corners
        .iter()
        .filter(|grid| corner_map.contains_key(grid))
        .count()
}

fn rectified_cell_corners_img(
    rectified: &calib_targets_chessboard::RectifiedBoardView,
    local_gx: i32,
    local_gy: i32,
) -> [Point2<f32>; 4] {
    let s = rectified.px_per_square;
    let x0 = local_gx as f32 * s;
    let y0 = local_gy as f32 * s;
    [
        rectified.h_img_from_rect.apply(Point2::new(x0, y0)),
        rectified.h_img_from_rect.apply(Point2::new(x0 + s, y0)),
        rectified.h_img_from_rect.apply(Point2::new(x0 + s, y0 + s)),
        rectified.h_img_from_rect.apply(Point2::new(x0, y0 + s)),
    ]
}
