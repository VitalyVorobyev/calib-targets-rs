use crate::circle_score::CircleCandidate;
use crate::detect::{detect_circles_via_square_warp, top_k_by_polarity};
use crate::match_circles::{estimate_grid_alignment, match_expected_circles};
use crate::types::{CircleMatch, MarkerBoardDetectionResult, MarkerBoardParams};

use std::collections::HashMap;

use nalgebra::Point2;

use calib_targets_chessboard::{ChessboardDetectionResult, ChessboardDetector};
use calib_targets_core::{
    Corner, GrayImageView, GridAlignment, GridCoords, TargetDetection, TargetKind,
};

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

        let mut matches = match_expected_circles(
            &self.params.layout.circles,
            &candidates,
            &self.params.match_params,
        );
        let (alignment, alignment_inliers) = estimate_grid_alignment(
            &matches,
            &candidates,
            self.params.match_params.min_offset_inliers,
        )
        .map(|(alignment, inliers)| (Some(alignment), inliers))
        .unwrap_or((None, 0));

        if let Some(alignment) = alignment {
            for m in &mut matches {
                let Some(idx) = m.matched_index else {
                    continue;
                };
                let Some(cand) = candidates.get(idx) else {
                    continue;
                };
                let [rx, ry] = alignment.transform.apply(cand.cell.i, cand.cell.j);
                m.offset_cells = Some(crate::coords::CellOffset {
                    di: m.expected.cell.i - rx,
                    dj: m.expected.cell.j - ry,
                });
            }
        }

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

            let cols = i32::try_from(self.params.layout.cols).ok();
            let rows = i32::try_from(self.params.layout.rows).ok();
            if let Some((cols, rows)) = cols.zip(rows) {
                let cell_size = self.params.layout.cell_size;
                for corner in &mut detection.corners {
                    let Some(grid) = corner.grid else {
                        continue;
                    };
                    if grid.i < 0 || grid.j < 0 || grid.i >= cols || grid.j >= rows {
                        continue;
                    }
                    let id = (grid.j as u32)
                        .checked_mul(self.params.layout.cols)
                        .and_then(|base| base.checked_add(grid.i as u32));
                    corner.id = id;
                    if let Some(size) = cell_size.filter(|s| s.is_finite() && *s > 0.0) {
                        corner.target_position =
                            Some(Point2::new(grid.i as f32 * size, grid.j as f32 * size));
                    }
                }

                detection.corners.sort_by(|a, b| {
                    let ga = a.grid.unwrap();
                    let gb = b.grid.unwrap();
                    (ga.j, ga.i).cmp(&(gb.j, gb.i))
                });
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
