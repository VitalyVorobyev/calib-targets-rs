use super::alignment_select::select_alignment;
use super::corner_mapping::map_charuco_corners;
use super::corner_validation::{validate_and_fix_corners, CornerValidationConfig};
use super::marker_sampling::{build_corner_map, build_marker_cells};
use super::{
    CharucoDetectError, CharucoDetectionResult, CharucoDetectionRun, CharucoDetectorParams,
    CharucoDiagnostics,
};
use crate::alignment::CharucoAlignment;
use crate::board::{CharucoBoard, CharucoBoardError};
use calib_targets_aruco::{scan_decode_markers_in_cells, MarkerDetection, Matcher};
use calib_targets_chessboard::ChessboardDetector;
use calib_targets_core::{Corner, GrayImageView};
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

        let Some(chessboard) = chessboard_run.detection else {
            diagnostics.timings.total_ms = elapsed_ms(total_start);
            return CharucoDetectionRun {
                result: Err(CharucoDetectError::ChessboardNotDetected),
                diagnostics,
            };
        };

        let decode_start = Instant::now();
        let corner_map = build_corner_map(&chessboard.detection.corners, &chessboard.inliers);
        let cells = build_marker_cells(&corner_map);
        diagnostics.candidate_cell_count = cells.len();

        let markers = scan_decode_markers_in_cells(
            image,
            &cells,
            self.params.px_per_square,
            &self.params.scan,
            &self.matcher,
        );
        diagnostics.decoded_marker_count = markers.len();
        diagnostics.timings.decode_ms = elapsed_ms(decode_start);

        if markers.is_empty() {
            diagnostics.timings.total_ms = elapsed_ms(total_start);
            return CharucoDetectionRun {
                result: Err(CharucoDetectError::NoMarkers),
                diagnostics,
            };
        }

        let alignment_start = Instant::now();
        let (markers, alignment) = match self.select_and_refine_markers(markers) {
            Some(run) => run,
            None => {
                diagnostics.timings.alignment_ms = elapsed_ms(alignment_start);
                diagnostics.timings.total_ms = elapsed_ms(total_start);
                return CharucoDetectionRun {
                    result: Err(CharucoDetectError::AlignmentFailed { inliers: 0 }),
                    diagnostics,
                };
            }
        };
        diagnostics.aligned_marker_count = markers.len();
        diagnostics.alignment_inlier_count = alignment.marker_inliers.len();
        diagnostics.timings.alignment_ms = elapsed_ms(alignment_start);

        if alignment.marker_inliers.len() < self.params.min_marker_inliers {
            diagnostics.timings.total_ms = elapsed_ms(total_start);
            return CharucoDetectionRun {
                result: Err(CharucoDetectError::AlignmentFailed {
                    inliers: alignment.marker_inliers.len(),
                }),
                diagnostics,
            };
        }

        let map_validate_start = Instant::now();
        let mapped = map_charuco_corners(&self.board, &chessboard.detection, &alignment);
        diagnostics.mapped_corner_count_before_validation = mapped.corners.len();

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
        diagnostics.final_corner_count = validation.detection.corners.len();
        diagnostics.corner_validation = Some(validation.diagnostics);
        diagnostics.timings.map_validate_ms = elapsed_ms(map_validate_start);
        diagnostics.timings.total_ms = elapsed_ms(total_start);

        CharucoDetectionRun {
            result: Ok(CharucoDetectionResult {
                detection: validation.detection,
                markers,
                alignment: alignment.alignment,
            }),
            diagnostics,
        }
    }

    #[cfg_attr(feature = "tracing", instrument(level = "info", skip(self, markers),
      fields(markers=markers.len())))]
    fn select_and_refine_markers(
        &self,
        markers: Vec<MarkerDetection>,
    ) -> Option<(Vec<MarkerDetection>, CharucoAlignment)> {
        let (markers, alignment) = select_alignment(&self.board, markers)?;
        Some((markers, alignment))
    }
}

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1_000.0
}
