use std::collections::HashMap;

use nalgebra::Point2;

use calib_targets_chessboard::{ChessboardDetectionResult, ChessboardDetector};
use calib_targets_core::{
    Corner, GrayImageView, GridAlignment, GridCoords, GridTransform, TargetDetection, TargetKind,
};

use crate::circle_score::CircleCandidate;
use crate::detect::{detect_circles_via_square_warp, top_k_by_polarity};
use crate::match_circles::{estimate_grid_offset, match_expected_circles};
use crate::types::{CircleMatch, MarkerBoardDetectionResult, MarkerBoardParams};

/// Marker board detector: chessboard + three circle markers.
pub struct MarkerBoardDetector {
    params: MarkerBoardParams,
    chessboard_detector: ChessboardDetector,
}

impl MarkerBoardDetector {
    pub fn new(mut params: MarkerBoardParams) -> Self {
        if params.chessboard.expected_rows.is_none() {
            params.chessboard.expected_rows = Some(params.layout.rows);
        }
        if params.chessboard.expected_cols.is_none() {
            params.chessboard.expected_cols = Some(params.layout.cols);
        }

        let chessboard_detector = ChessboardDetector::new(params.chessboard.clone())
            .with_grid_search(params.grid_graph.clone());

        Self {
            params,
            chessboard_detector,
        }
    }

    pub fn params(&self) -> &MarkerBoardParams {
        &self.params
    }

    /// Chessboard-only detection (no circle verification).
    pub fn detect_from_corners(&self, corners: &[Corner]) -> Option<MarkerBoardDetectionResult> {
        let chess = self.chessboard_detector.detect_from_corners(corners)?;
        Some(self.result_from_chessboard(chess, Vec::new(), Vec::new(), None, 0))
    }

    /// Full detection using image-space circle scoring.
    ///
    /// Returns circle candidates, matched circles, and an optional grid offset
    /// that maps detected grid coordinates to board coordinates.
    pub fn detect_from_image_and_corners(
        &self,
        image: &GrayImageView<'_>,
        corners: &[Corner],
    ) -> Option<MarkerBoardDetectionResult> {
        let chess = self.chessboard_detector.detect_from_corners(corners)?;
        let corner_map = build_corner_map(&chess.detection);
        let roi = self
            .params
            .roi_cells
            .map(|[i0, j0, i1, j1]| (i0, j0, i1, j1));

        let mut candidates =
            detect_circles_via_square_warp(image, &corner_map, &self.params.circle_score, roi);

        let max_per = self.params.match_params.max_candidates_per_polarity;
        if max_per > 0 && !candidates.is_empty() {
            let (white, black) = top_k_by_polarity(candidates, max_per, max_per);
            candidates = [white, black].concat();
        }

        let matches = match_expected_circles(
            &self.params.layout.circles,
            &candidates,
            &self.params.match_params,
        );
        let (alignment, alignment_inliers) =
            estimate_grid_offset(&matches, self.params.match_params.min_offset_inliers)
                .map(|(offset, inliers)| {
                    (
                        Some(GridAlignment {
                            transform: GridTransform::IDENTITY,
                            translation: [offset.di, offset.dj],
                        }),
                        inliers,
                    )
                })
                .unwrap_or((None, 0));

        Some(self.result_from_chessboard(chess, candidates, matches, alignment, alignment_inliers))
    }

    fn result_from_chessboard(
        &self,
        chess: ChessboardDetectionResult,
        circle_candidates: Vec<CircleCandidate>,
        circle_matches: Vec<CircleMatch>,
        alignment: Option<GridAlignment>,
        alignment_inliers: usize,
    ) -> MarkerBoardDetectionResult {
        let mut detection = relabel_as_marker(chess.detection);
        if let Some(alignment) = alignment {
            for corner in &mut detection.corners {
                if let Some(grid) = &mut corner.grid {
                    let [i, j] = alignment.map(grid.i, grid.j);
                    grid.i = i;
                    grid.j = j;
                }
            }
        }
        MarkerBoardDetectionResult {
            detection,
            inliers: chess.inliers,
            circle_candidates,
            circle_matches,
            alignment,
            alignment_inliers,
        }
    }
}

fn relabel_as_marker(mut detection: TargetDetection) -> TargetDetection {
    detection.kind = TargetKind::CheckerboardMarker;
    detection
}

fn build_corner_map(det: &TargetDetection) -> HashMap<GridCoords, Point2<f32>> {
    det.corners
        .iter()
        .filter_map(|c| Some((c.grid?, c.position)))
        .collect()
}
