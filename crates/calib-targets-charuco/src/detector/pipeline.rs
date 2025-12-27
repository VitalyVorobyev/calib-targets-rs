use super::alignment_select::select_alignment;
use super::corner_mapping::map_charuco_corners;
use super::marker_sampling::{build_corner_map, build_marker_cells};
use super::{CharucoDetectError, CharucoDetectionResult, CharucoDetectorParams};
use crate::alignment::CharucoAlignment;
use crate::board::{CharucoBoard, CharucoBoardError, CharucoBoardSpec};
use calib_targets_aruco::{scan_decode_markers_in_cells, MarkerDetection, Matcher};
use calib_targets_chessboard::ChessboardDetector;
use calib_targets_core::{Corner, GrayImageView};

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
    /// Create a detector for a given board and parameters.
    pub fn new(
        board_cfg: CharucoBoardSpec,
        mut params: CharucoDetectorParams,
    ) -> Result<Self, CharucoBoardError> {
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
        let detector = ChessboardDetector::new(self.params.chessboard.clone())
            .with_grid_search(self.params.graph.clone());
        let chessboard = detector
            .detect_from_corners(corners)
            .ok_or(CharucoDetectError::ChessboardNotDetected)?;

        let corner_map = build_corner_map(&chessboard.detection.corners, &chessboard.inliers);
        let cells = build_marker_cells(&corner_map);

        let scan_cfg = self.params.scan.clone();

        let markers = scan_decode_markers_in_cells(
            image,
            &cells,
            self.params.px_per_square,
            &scan_cfg,
            &self.matcher,
        );

        if markers.is_empty() {
            return Err(CharucoDetectError::NoMarkers);
        }

        let (markers, alignment) = self
            .select_and_refine_markers(markers)
            .ok_or(CharucoDetectError::AlignmentFailed { inliers: 0usize })?;

        if alignment.marker_inliers.len() < self.params.min_marker_inliers {
            return Err(CharucoDetectError::AlignmentFailed {
                inliers: alignment.marker_inliers.len(),
            });
        }

        let detection = map_charuco_corners(&self.board, &chessboard.detection, &alignment);

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
