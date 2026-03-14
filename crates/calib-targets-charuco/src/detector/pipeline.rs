use super::alignment_select::select_alignment;
use super::corner_mapping::map_charuco_corners;
use super::corner_validation::{validate_and_fix_corners, CornerValidationConfig};
use super::marker_sampling::{build_corner_map, build_marker_cells};
use super::{CharucoDetectError, CharucoDetectionResult, CharucoDetectorParams};
use crate::alignment::CharucoAlignment;
use crate::board::{CharucoBoard, CharucoBoardError};
use calib_targets_aruco::{scan_decode_markers_in_cells, MarkerDetection, Matcher};
use calib_targets_chessboard::ChessboardDetector;
use calib_targets_core::{Corner, GrayImageView};
use log::{debug, warn};

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
            // `board_cfg.rows/cols` are square counts; chessboard detector expects inner corners.
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
    ///
    /// This uses per-cell marker sampling by default. Set
    /// `build_rectified_image` if you need a rectified output image.
    #[cfg_attr(feature = "tracing", instrument(level = "info", skip(self, image, corners), fields(num_corners=corners.len())))]
    pub fn detect(
        &self,
        image: &GrayImageView<'_>,
        corners: &[Corner],
    ) -> Result<CharucoDetectionResult, CharucoDetectError> {
        debug!(
            "starting ChArUco detection: image={}x{}, input_corners={}, board_inner={}x{}, px_per_square={:.1}, min_marker_inliers={}",
            image.width,
            image.height,
            corners.len(),
            self.board.expected_inner_cols(),
            self.board.expected_inner_rows(),
            self.params.px_per_square,
            self.params.min_marker_inliers
        );
        let detector = ChessboardDetector::new(self.params.chessboard.clone())
            .with_grid_search(self.params.graph.clone());
        let chessboard = match detector.detect_from_corners(corners) {
            Some(chessboard) => {
                debug!(
                    "chessboard stage succeeded: detected_corners={}, inliers={}, orientations={:?}",
                    chessboard.detection.corners.len(),
                    chessboard.inliers.len(),
                    chessboard
                        .orientations
                        .map(|angles| [angles[0].to_degrees(), angles[1].to_degrees()])
                );
                chessboard
            }
            None => {
                warn!(
                    "chessboard stage failed: input_corners={}, min_corner_strength={:.3}, min_corners={}, spacing=[{:.1}, {:.1}], k_neighbors={}, orientation_tol={:.1} deg",
                    corners.len(),
                    self.params.chessboard.min_corner_strength,
                    self.params.chessboard.min_corners,
                    self.params.graph.min_spacing_pix,
                    self.params.graph.max_spacing_pix,
                    self.params.graph.k_neighbors,
                    self.params.graph.orientation_tolerance_deg
                );
                return Err(CharucoDetectError::ChessboardNotDetected);
            }
        };

        let corner_map = build_corner_map(&chessboard.detection.corners, &chessboard.inliers);
        let cells = build_marker_cells(&corner_map);
        debug!(
            "marker sampling inputs: corner_map_entries={}, complete_marker_cells={}",
            corner_map.len(),
            cells.len()
        );

        let scan_cfg = self.params.scan.clone();

        let markers = scan_decode_markers_in_cells(
            image,
            &cells,
            self.params.px_per_square,
            &scan_cfg,
            &self.matcher,
        );
        debug!("marker scan produced {} detections", markers.len());

        if markers.is_empty() {
            warn!(
                "marker scan failed: no markers decoded from {} candidate cells",
                cells.len()
            );
            return Err(CharucoDetectError::NoMarkers);
        }

        let Some((markers, alignment)) = self.select_and_refine_markers(markers) else {
            warn!("marker-to-board alignment failed before producing any inliers");
            return Err(CharucoDetectError::AlignmentFailed { inliers: 0usize });
        };
        debug!(
            "alignment result: kept_markers={}, marker_inliers={}, transform={:?}, translation={:?}",
            markers.len(),
            alignment.marker_inliers.len(),
            alignment.alignment.transform,
            alignment.alignment.translation
        );

        if alignment.marker_inliers.len() < self.params.min_marker_inliers {
            warn!(
                "marker-to-board alignment rejected: {} inliers < required {}",
                alignment.marker_inliers.len(),
                self.params.min_marker_inliers
            );
            return Err(CharucoDetectError::AlignmentFailed {
                inliers: alignment.marker_inliers.len(),
            });
        }

        let detection = map_charuco_corners(&self.board, &chessboard.detection, &alignment);
        debug!(
            "mapped {} ChArUco corners before validation",
            detection.corners.len()
        );

        let detection = validate_and_fix_corners(
            detection,
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
        debug!(
            "corner validation finished with {} ChArUco corners",
            detection.corners.len()
        );

        Ok(CharucoDetectionResult {
            detection,
            markers,
            alignment: alignment.alignment,
        })
    }

    #[cfg_attr(feature = "tracing", instrument(level = "info", skip(self, markers),
      fields(markers=markers.len())))]
    fn select_and_refine_markers(
        &self,
        markers: Vec<MarkerDetection>,
    ) -> Option<(Vec<MarkerDetection>, CharucoAlignment)> {
        // TODO: just run solve_aligment on the full set of markers
        let (markers, alignment) = select_alignment(&self.board, markers)?;

        Some((markers, alignment))
    }
}
