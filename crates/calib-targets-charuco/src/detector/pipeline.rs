use super::alignment_select::select_alignment;
use super::corner_mapping::map_charuco_corners;
use super::corner_validation::{validate_and_fix_corners, CornerValidationConfig};
use super::grid_smoothness::smooth_grid_corners;
use super::marker_sampling::{build_corner_map, build_marker_cells};
use super::merge::merge_charuco_results;
use super::params::to_chess_params;
use super::{CharucoDetectError, CharucoDetectionResult, CharucoParams};
use crate::alignment::CharucoAlignment;
use crate::board::{CharucoBoard, CharucoBoardError};
use calib_targets_aruco::{scan_decode_markers_in_cells, MarkerDetection, Matcher};
use calib_targets_chessboard::{Detection as ChessDetection, Detector as ChessDetector};
use calib_targets_core::{Corner, GrayImageView};
use log::{debug, warn};

#[cfg(feature = "tracing")]
use tracing::instrument;

/// Grid-first ChArUco detector.
#[derive(Debug)]
pub struct CharucoDetector {
    board: CharucoBoard,
    params: CharucoParams,
    matcher: Matcher,
}

impl CharucoDetector {
    /// Create a detector from parameters (board spec lives in `params.board`).
    pub fn new(mut params: CharucoParams) -> Result<Self, CharucoBoardError> {
        let board_cfg = params.board;
        if !params.scan.marker_size_rel.is_finite() || params.scan.marker_size_rel <= 0.0 {
            params.scan.marker_size_rel = board_cfg.marker_size_rel;
        }

        // Cap max_hamming at max_correction_bits when the dictionary declares a
        // non-zero value.  AprilTag families report max_correction_bits == 0 in
        // OpenCV metadata even though their minimum inter-code Hamming distance
        // is large (e.g. 10 for 36h10), so we skip capping in that case and let
        // the user control error tolerance directly.
        let max_hamming = if board_cfg.dictionary.max_correction_bits > 0 {
            params
                .max_hamming
                .min(board_cfg.dictionary.max_correction_bits)
        } else {
            params.max_hamming
        };
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
    pub fn params(&self) -> &CharucoParams {
        &self.params
    }

    /// Detect a ChArUco board from an image and a set of corners.
    ///
    /// When the grid graph contains multiple disconnected components, each
    /// qualifying component is processed independently and results with
    /// consistent alignments are merged.
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
        let detector = ChessDetector::new(self.params.chessboard.clone());
        let components = detector.detect_all(corners);

        if components.is_empty() {
            warn!(
                "chessboard stage failed: input_corners={}, min_corner_strength={:.3}, cluster_tol={:.1} deg, max_components={}",
                corners.len(),
                self.params.chessboard.min_corner_strength,
                self.params.chessboard.cluster_tol_deg,
                self.params.chessboard.max_components,
            );
            return Err(CharucoDetectError::ChessboardNotDetected);
        }

        debug!(
            "chessboard stage produced {} qualifying components: {:?}",
            components.len(),
            components
                .iter()
                .map(|c| c.target.corners.len())
                .collect::<Vec<_>>()
        );

        let mut results: Vec<CharucoDetectionResult> = Vec::new();
        for (i, chessboard) in components.iter().enumerate() {
            let min_inliers = if i == 0 {
                self.params.min_marker_inliers
            } else {
                self.params.min_secondary_marker_inliers
            };

            match self.detect_component(image, chessboard, min_inliers) {
                Ok(result) => {
                    debug!(
                        "component {i}: {} corners, {} markers",
                        result.detection.corners.len(),
                        result.markers.len()
                    );
                    results.push(result);
                }
                Err(e) => {
                    debug!("component {i} failed: {e}");
                }
            }
        }

        if results.is_empty() {
            return Err(CharucoDetectError::NoMarkers);
        }

        if results.len() == 1 {
            return Ok(results.into_iter().next().unwrap());
        }

        Ok(merge_charuco_results(results))
    }

    /// Run the full charuco pipeline on a single chessboard component.
    fn detect_component(
        &self,
        image: &GrayImageView<'_>,
        chessboard: &ChessDetection,
        min_marker_inliers: usize,
    ) -> Result<CharucoDetectionResult, CharucoDetectError> {
        // detector emits only validated corners — every entry in target.corners is
        // an inlier by construction.
        let inliers: Vec<usize> = (0..chessboard.target.corners.len()).collect();
        let mut corner_map = build_corner_map(&chessboard.target.corners, &inliers);
        let corner_redetect_params = to_chess_params(&self.params.corner_redetect_params);
        smooth_grid_corners(
            &mut corner_map,
            image,
            self.params.px_per_square,
            self.params.grid_smoothness_threshold_rel,
            &corner_redetect_params,
        );
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

        if alignment.marker_inliers.len() < min_marker_inliers {
            warn!(
                "marker-to-board alignment rejected: {} inliers < required {}",
                alignment.marker_inliers.len(),
                min_marker_inliers
            );
            return Err(CharucoDetectError::AlignmentFailed {
                inliers: alignment.marker_inliers.len(),
            });
        }

        let detection = map_charuco_corners(&self.board, &chessboard.target, &alignment);
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
                chess_params: &corner_redetect_params,
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
