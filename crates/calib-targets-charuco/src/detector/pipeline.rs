use super::alignment_select::{maybe_refine_alignment, retain_inlier_markers, select_alignment};
use super::corner_mapping::{map_charuco_corners_from_markers, marker_board_cells};
use super::marker_sampling::{
    build_corner_map, build_marker_cells, refine_markers_for_alignment, CornerMap,
};
use super::{CharucoDetectError, CharucoDetectionResult, CharucoDetectorParams};
use crate::alignment::CharucoAlignment;
use crate::board::CharucoBoard;
use calib_targets_aruco::{
    scan_decode_markers, scan_decode_markers_in_cells, MarkerDetection, Matcher, ScanDecodeConfig,
};
use calib_targets_chessboard::{rectify_mesh_from_grid, ChessboardDetector};
use calib_targets_core::{Corner, GrayImageView};

/// Grid-first ChArUco detector.
pub struct CharucoDetector {
    board: CharucoBoard,
    params: CharucoDetectorParams,
    matcher: Matcher,
}

impl CharucoDetector {
    /// Create a detector for a given board and parameters.
    pub fn new(board: CharucoBoard, mut params: CharucoDetectorParams) -> Self {
        if params.chessboard.expected_rows.is_none() {
            params.chessboard.expected_rows = Some(board.expected_inner_rows());
        }
        if params.chessboard.expected_cols.is_none() {
            params.chessboard.expected_cols = Some(board.expected_inner_cols());
        }
        if !params.scan.marker_size_rel.is_finite() || params.scan.marker_size_rel <= 0.0 {
            params.scan.marker_size_rel = board.spec().marker_size_rel;
        }

        let max_hamming = params
            .max_hamming
            .min(board.spec().dictionary.max_correction_bits);
        params.max_hamming = max_hamming;

        let matcher = Matcher::new(board.spec().dictionary, max_hamming);

        Self {
            board,
            params,
            matcher,
        }
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

        let mut rectified_for_output = None;
        let (mut markers, mut alignment) = self
            .select_and_refine_markers(markers, image, &corner_map, &scan_cfg)
            .ok_or(CharucoDetectError::AlignmentFailed { inliers: 0usize })?;

        if alignment.marker_inliers.len() < self.params.min_marker_inliers
            && self.params.fallback_to_rectified
        {
            let rectified = rectify_mesh_from_grid(
                image,
                &chessboard.detection.corners,
                &chessboard.inliers,
                self.params.px_per_square,
            )?;
            let rect_view = GrayImageView {
                width: rectified.rect.width,
                height: rectified.rect.height,
                data: &rectified.rect.data,
            };
            let rect_markers = scan_decode_markers(
                &rect_view,
                rectified.cells_x,
                rectified.cells_y,
                rectified.px_per_square,
                &scan_cfg,
                &self.matcher,
            );
            if let Some((refined_markers, refined_alignment)) =
                self.select_and_refine_markers(rect_markers, image, &corner_map, &scan_cfg)
            {
                markers = refined_markers;
                alignment = refined_alignment;
                rectified_for_output = Some(rectified);
            }
        }

        if alignment.marker_inliers.len() < self.params.min_marker_inliers {
            return Err(CharucoDetectError::AlignmentFailed {
                inliers: alignment.marker_inliers.len(),
            });
        }

        let (markers, alignment) = retain_inlier_markers(markers, alignment);
        let marker_board_cells = marker_board_cells(&self.board, &markers, &alignment);

        let detection = map_charuco_corners_from_markers(
            &self.board,
            &chessboard.detection,
            &alignment,
            &marker_board_cells,
        );

        let rectified = if self.params.build_rectified_image && rectified_for_output.is_none() {
            Some(rectify_mesh_from_grid(
                image,
                &chessboard.detection.corners,
                &chessboard.inliers,
                self.params.px_per_square,
            )?)
        } else {
            rectified_for_output
        };

        Ok(CharucoDetectionResult {
            detection,
            chessboard: chessboard.detection,
            chessboard_inliers: chessboard.inliers,
            markers,
            marker_board_cells,
            alignment,
            rectified,
        })
    }

    fn select_and_refine_markers(
        &self,
        markers: Vec<MarkerDetection>,
        image: &GrayImageView<'_>,
        corner_map: &CornerMap,
        scan_cfg: &ScanDecodeConfig,
    ) -> Option<(Vec<MarkerDetection>, CharucoAlignment)> {
        let (mut markers, mut alignment) = select_alignment(&self.board, markers)?;

        let refined = refine_markers_for_alignment(
            &self.board,
            &alignment,
            image,
            corner_map,
            self.params.px_per_square,
            scan_cfg,
            &self.matcher,
        );
        if let Some((refined_markers, refined_alignment)) =
            maybe_refine_alignment(&self.board, refined, alignment.marker_inliers.len())
        {
            markers = refined_markers;
            alignment = refined_alignment;
        }

        Some((markers, alignment))
    }
}
