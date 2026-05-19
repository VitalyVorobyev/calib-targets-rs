use crate::circle_score::CircleCandidate;
use crate::detect::{detect_circles_via_square_warp, top_k_by_polarity};
use crate::diagnostics::MarkerBoardDiagnostics;
use crate::match_circles::{estimate_grid_alignment, match_expected_circles};
use crate::types::{CircleMatch, MarkerBoardDetectionResult, MarkerBoardParams};

use std::collections::HashMap;

use nalgebra::Point2;

use calib_targets_chessboard::ChessCorner;
use calib_targets_chessboard::{ChessboardDetection, Detector as ChessDetector};
use calib_targets_core::{
    GrayImageView, GridAlignment, GridCoords, LabeledCorner, TargetDetection, TargetKind,
};

/// Marker board detector: chessboard + three circle markers.
pub struct MarkerBoardDetector {
    params: MarkerBoardParams,
    chessboard_detector: ChessDetector,
}

impl MarkerBoardDetector {
    /// Construct a marker-board detector from its parameters.
    pub fn new(params: MarkerBoardParams) -> Self {
        // chessboard detector is scale-invariant — it does not need
        // expected_rows/cols hints. The marker circles supply the geometry
        // constraint.
        let chessboard_detector = ChessDetector::new(params.chessboard.clone());

        Self {
            params,
            chessboard_detector,
        }
    }

    /// Borrow the parameters this detector was constructed with.
    pub fn params(&self) -> &MarkerBoardParams {
        &self.params
    }

    /// Chessboard-only detection (no circle verification).
    pub fn detect_from_corners(
        &self,
        corners: &[ChessCorner],
    ) -> Option<MarkerBoardDetectionResult> {
        self.detect_from_corners_with_diagnostics(corners)
            .map(|(result, _)| result)
    }

    /// Chessboard-only detection (no circle verification), additionally
    /// returning per-call diagnostics.
    ///
    /// This path has no image to score circles against, so the returned
    /// [`MarkerBoardDiagnostics`] carries empty `circle_candidates` /
    /// `circle_matches` and `alignment_inliers = 0`; only `inliers` (the
    /// per-corner provenance from the chessboard stage) is populated. See
    /// [`crate::diagnostics::MarkerBoardDiagnostics`] for the stability
    /// promise.
    pub fn detect_from_corners_with_diagnostics(
        &self,
        corners: &[ChessCorner],
    ) -> Option<(MarkerBoardDetectionResult, MarkerBoardDiagnostics)> {
        let chess = self.chessboard_detector.detect(corners)?;
        Some(self.result_from_chessboard(chess, Vec::new(), Vec::new(), None, 0))
    }

    /// Full detection using image-space circle scoring.
    pub fn detect_from_image_and_corners(
        &self,
        image: &GrayImageView<'_>,
        corners: &[ChessCorner],
    ) -> Option<MarkerBoardDetectionResult> {
        self.detect_from_image_and_corners_with_diagnostics(image, corners)
            .map(|(result, _)| result)
    }

    /// Full detection using image-space circle scoring, additionally
    /// returning per-call diagnostics.
    ///
    /// The returned [`MarkerBoardDiagnostics`] carries every scored circle
    /// candidate, the expected-to-detected circle matches, the per-corner
    /// provenance, and the alignment-inlier count. See
    /// [`crate::diagnostics::MarkerBoardDiagnostics`] for the shape and
    /// stability promise.
    pub fn detect_from_image_and_corners_with_diagnostics(
        &self,
        image: &GrayImageView<'_>,
        corners: &[ChessCorner],
    ) -> Option<(MarkerBoardDetectionResult, MarkerBoardDiagnostics)> {
        let chess = self.chessboard_detector.detect(corners)?;
        let corner_map = build_corner_map(&chess);
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

        let alignment = alignment?;
        for m in &mut matches {
            let Some(idx) = m.matched_index else {
                continue;
            };
            let Some(cand) = candidates.get(idx) else {
                continue;
            };
            let r = alignment.transform.apply(cand.cell.i, cand.cell.j);
            m.offset_cells = Some(crate::coords::CellOffset {
                di: m.expected.cell.i - r.i,
                dj: m.expected.cell.j - r.j,
            });
        }

        Some(self.result_from_chessboard(
            chess,
            candidates,
            matches,
            Some(alignment),
            alignment_inliers,
        ))
    }

    fn result_from_chessboard(
        &self,
        chess: ChessboardDetection,
        circle_candidates: Vec<CircleCandidate>,
        circle_matches: Vec<CircleMatch>,
        alignment: Option<GridAlignment>,
        alignment_inliers: usize,
    ) -> (MarkerBoardDetectionResult, MarkerBoardDiagnostics) {
        let (target, inliers) = chessboard_detection_to_target(&chess);
        let mut detection = relabel_as_marker(target);
        if let Some(alignment) = alignment {
            for corner in &mut detection.corners {
                if let Some(grid) = &mut corner.grid {
                    let g = alignment.map(grid.i, grid.j);
                    grid.i = g.i;
                    grid.j = g.j;
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
                    // INVARIANT: grid coordinates are always populated for every corner
                    // at this point — they are assigned in the loop above via the
                    // `set_grid_coords_from_chess` call before this sort.
                    let ga = a.grid.unwrap();
                    let gb = b.grid.unwrap();
                    (ga.j, ga.i).cmp(&(gb.j, gb.i))
                });
            }
        }
        (
            MarkerBoardDetectionResult::from_target_detection(detection, alignment),
            MarkerBoardDiagnostics {
                inliers,
                circle_candidates,
                circle_matches,
                alignment_inliers,
            },
        )
    }
}

/// Adapt a [`ChessboardDetection`] into the generic [`TargetDetection`]
/// the marker pipeline operates on, plus the parallel input-index list
/// the marker diagnostics expose as `inliers`.
fn chessboard_detection_to_target(chess: &ChessboardDetection) -> (TargetDetection, Vec<usize>) {
    let mut corners = Vec::with_capacity(chess.corners.len());
    let mut inliers = Vec::with_capacity(chess.corners.len());
    for c in &chess.corners {
        corners.push(LabeledCorner::new(c.position, c.score).with_grid(c.grid));
        inliers.push(c.input_index);
    }
    (
        TargetDetection::new(TargetKind::Chessboard, corners),
        inliers,
    )
}

fn relabel_as_marker(mut detection: TargetDetection) -> TargetDetection {
    detection.kind = TargetKind::CheckerboardMarker;
    detection
}

fn build_corner_map(det: &ChessboardDetection) -> HashMap<GridCoords, Point2<f32>> {
    det.corners.iter().map(|c| (c.grid, c.position)).collect()
}
