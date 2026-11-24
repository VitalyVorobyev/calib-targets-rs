use crate::circle_score::CircleCandidate;
use crate::detect::{detect_circles_via_square_warp, top_k_by_polarity};
use crate::diagnostics::MarkerBoardDiagnostics;
use crate::error::{MarkerBoardDetectError, MarkerBoardParamsError};
use crate::match_circles::{
    corner_frame, resolve_board_frame_with_bounds, BoardCornerBounds, FrameSearch,
};
use crate::types::{MarkerBoardDetection, MarkerBoardParams};

use nalgebra::Point2;

use calib_targets_chessboard::{detect_corner_frame, ChessCorner};
use calib_targets_chessboard::{ChessboardDetection, ChessboardDetector as ChessDetector};
use calib_targets_core::{
    Coord, GrayImageView, GridAlignment, LabeledCorner, TargetDetection, TargetKind,
};

/// Marker board detector: chessboard + three circle markers.
pub struct MarkerBoardDetector {
    params: MarkerBoardParams,
    chessboard_detector: ChessDetector,
}

impl MarkerBoardDetector {
    /// Construct a marker-board detector from its parameters.
    ///
    /// Validates board geometry, circle marker layout, circle-scoring tuning,
    /// and the embedded chessboard detector parameters.
    pub fn new(params: MarkerBoardParams) -> Result<Self, MarkerBoardParamsError> {
        validate_marker_board_params(&params)?;
        // chessboard detector is scale-invariant — it does not need
        // expected_rows/cols hints. The marker circles supply the geometry
        // constraint.
        let chessboard_detector = ChessDetector::new(params.chessboard.clone())?;

        Ok(Self {
            params,
            chessboard_detector,
        })
    }

    /// Borrow the parameters this detector was constructed with.
    pub fn params(&self) -> &MarkerBoardParams {
        &self.params
    }

    /// Run the ChESS corner front-end configured by
    /// [`MarkerBoardParams::chess`](crate::MarkerBoardParams::chess) over
    /// `image`.
    ///
    /// This is the corner pass [`Self::detect`] runs internally, exposed so a
    /// caller that wants to reuse one corner cloud across several detectors
    /// (or inspect it) can run it once and feed
    /// [`Self::detect_with_corners`].
    pub fn detect_corners(&self, image: &GrayImageView<'_>) -> Vec<ChessCorner> {
        calib_targets_chessboard::detect_corners(image, &self.params.chess)
    }

    /// Detect a marker board in `image`, running the corner front-end
    /// configured by [`MarkerBoardParams::chess`](crate::MarkerBoardParams::chess).
    ///
    /// This is the ergonomic entry point: hand it an image and it does the
    /// whole pipeline. It gives the result of
    /// `self.detect_with_corners(image, &self.detect_corners(image))` — reach
    /// for [`Self::detect_with_corners`] when you already have a corner cloud
    /// (a custom upstream, or one shared across detectors).
    ///
    /// Running the corner pass itself, it also knows the frame's noise level
    /// and records it on the result's [`quality`](MarkerBoardDetection::quality).
    ///
    /// # Errors
    ///
    /// See [`MarkerBoardDetectError`].
    pub fn detect(
        &self,
        image: &GrayImageView<'_>,
    ) -> Result<MarkerBoardDetection, MarkerBoardDetectError> {
        let frame = detect_corner_frame(image, &self.params.chess);
        let mut result = self.detect_with_corners(image, &frame.corners)?;
        result.quality = result.quality.with_noise_sigma(frame.noise_sigma);
        Ok(result)
    }

    /// Full detection from a pre-detected corner cloud, using image-space
    /// circle scoring.
    ///
    /// # Errors
    ///
    /// - [`MarkerBoardDetectError::ChessboardNotDetected`] — the chessboard
    ///   stage recovered no grid from `corners`.
    /// - [`MarkerBoardDetectError::AlignmentFailed`] — a grid was found but
    ///   the scored circles did not pin it to the board layout.
    pub fn detect_with_corners(
        &self,
        image: &GrayImageView<'_>,
        corners: &[ChessCorner],
    ) -> Result<MarkerBoardDetection, MarkerBoardDetectError> {
        self.detect_inner(image, corners).0
    }

    /// [`Self::detect`] + per-call diagnostics.
    ///
    /// Runs the corner front-end configured by
    /// [`MarkerBoardParams::chess`](crate::MarkerBoardParams::chess) and then
    /// [`Self::diagnose_with_corners`].
    ///
    /// Available only with the `diagnostics` feature enabled.
    #[cfg(feature = "diagnostics")]
    pub fn diagnose(
        &self,
        image: &GrayImageView<'_>,
    ) -> (
        Result<MarkerBoardDetection, MarkerBoardDetectError>,
        MarkerBoardDiagnostics,
    ) {
        let frame = detect_corner_frame(image, &self.params.chess);
        let (mut result, diagnostics) = self.diagnose_with_corners(image, &frame.corners);
        if let Ok(result) = &mut result {
            result.quality = result.quality.with_noise_sigma(frame.noise_sigma);
        }
        (result, diagnostics)
    }

    /// Full detection using image-space circle scoring, additionally
    /// returning per-call diagnostics.
    ///
    /// The returned [`MarkerBoardDiagnostics`] carries every scored circle
    /// candidate, the expected-to-detected circle matches, the per-corner
    /// provenance, and the alignment-inlier count. Diagnostics come back even
    /// when detection fails — best-effort, so overlay tools can render the
    /// circle hypotheses that *were* scored. See
    /// [`crate::diagnostics::MarkerBoardDiagnostics`] for the shape and
    /// stability promise.
    ///
    /// Available only with the `diagnostics` feature enabled.
    #[cfg(feature = "diagnostics")]
    pub fn diagnose_with_corners(
        &self,
        image: &GrayImageView<'_>,
        corners: &[ChessCorner],
    ) -> (
        Result<MarkerBoardDetection, MarkerBoardDetectError>,
        MarkerBoardDiagnostics,
    ) {
        self.detect_inner(image, corners)
    }

    fn detect_inner(
        &self,
        image: &GrayImageView<'_>,
        corners: &[ChessCorner],
    ) -> (
        Result<MarkerBoardDetection, MarkerBoardDetectError>,
        MarkerBoardDiagnostics,
    ) {
        // `detect`, not `detect_all`: a marker board is localised by its three
        // circle markers, which realistically sit inside a single connected
        // grid component — so the largest component is the only one that can
        // carry the layout constraint, and the extra components would only add
        // corners the alignment cannot place. ChArUco and PuzzleBoard call
        // `detect_all` because each of their components is *independently*
        // anchorable (a decoded marker / a decoded edge-code window), so a
        // board split into two fragments is still fully recoverable there.
        // This asymmetry is deliberate; do not "fix" it into `detect_all`.
        let Some(chess) = self.chessboard_detector.detect(corners) else {
            return (
                Err(MarkerBoardDetectError::ChessboardNotDetected),
                MarkerBoardDiagnostics::default(),
            );
        };
        let corner_map = chess.corner_map();
        let roi = self
            .params
            .roi_cells
            .map(|[i0, j0, i1, j1]| (i0, j0, i1, j1));

        let mut candidates = detect_circles_via_square_warp(
            image,
            &corner_map,
            self.params.board.circle_diameter_rel,
            &self.params.circle_score,
            roi,
        );
        let raw_candidates = candidates.clone();

        let max_per = self.params.match_params.max_candidates_per_polarity;
        if max_per > 0 && !candidates.is_empty() {
            let (white, black) = top_k_by_polarity(candidates, max_per, max_per);
            candidates = [white, black].concat();
        }

        let detected_grid_coords: Vec<Coord> = chess.corners.iter().map(|c| c.grid).collect();
        let mut frame = resolve_board_frame_with_bounds(
            &self.params.board.circles,
            &candidates,
            &self.params.match_params,
            Some(BoardCornerBounds {
                cols: self.params.board.cols,
                rows: self.params.board.rows,
                detected_corners: &detected_grid_coords,
            }),
        );
        // Remap `matched_index` from the truncated candidate slice back into
        // `raw_candidates` so diagnostics expose all scored hypotheses consistently.
        for m in &mut frame.matches {
            if let Some(idx) = m.matched_index {
                let target_cell = candidates[idx].cell;
                m.matched_index = raw_candidates.iter().position(|c| c.cell == target_cell);
            }
        }

        let Some(alignment) = frame.alignment else {
            // Counted before the vectors move into the diagnostics struct.
            let matched = frame
                .matches
                .iter()
                .filter(|m| m.matched_index.is_some())
                .count();
            let candidate_count = raw_candidates.len();
            let error = if frame.ambiguous {
                MarkerBoardDetectError::AlignmentAmbiguous {
                    inliers: frame.inliers,
                    runner_up: frame.runner_up_inliers,
                }
            } else {
                MarkerBoardDetectError::AlignmentFailed {
                    matched,
                    candidates: candidate_count,
                }
            };
            return (
                Err(error),
                MarkerBoardDiagnostics {
                    inliers: Vec::new(),
                    circle_candidates: raw_candidates,
                    circle_matches: frame.matches,
                    alignment_inliers: frame.inliers,
                    alignment_runner_up_inliers: frame.runner_up_inliers,
                    alignment_ambiguous: frame.ambiguous,
                },
            );
        };

        self.result_from_chessboard(chess, raw_candidates, frame, corner_frame(alignment))
    }

    fn result_from_chessboard(
        &self,
        chess: ChessboardDetection,
        circle_candidates: Vec<CircleCandidate>,
        frame: FrameSearch,
        alignment: GridAlignment,
    ) -> (
        Result<MarkerBoardDetection, MarkerBoardDetectError>,
        MarkerBoardDiagnostics,
    ) {
        // The parallel input-index list the diagnostics expose as `inliers`.
        let inliers: Vec<usize> = chess.corners.iter().map(|c| c.input_index).collect();
        let mut detection = relabel_as_marker(chess.to_target_detection());
        let inliers = self.apply_board_frame(&mut detection, inliers, alignment);
        let diagnostics = MarkerBoardDiagnostics {
            inliers,
            circle_candidates,
            circle_matches: frame.matches,
            alignment_inliers: frame.inliers,
            alignment_runner_up_inliers: frame.runner_up_inliers,
            alignment_ambiguous: frame.ambiguous,
        };
        if detection.corners.is_empty() {
            let matched = diagnostics
                .circle_matches
                .iter()
                .filter(|m| m.matched_index.is_some())
                .count();
            return (
                Err(MarkerBoardDetectError::AlignmentFailed {
                    matched,
                    candidates: diagnostics.circle_candidates.len(),
                }),
                diagnostics,
            );
        }
        (
            Ok(MarkerBoardDetection::from_target_detection(
                detection,
                Some(alignment),
            )),
            diagnostics,
        )
    }

    /// Relabel every corner into the board frame, keep only the ones that are
    /// inner corners of the board, and attach their board-canonical id and
    /// physical position.
    ///
    /// The layout fixes the board's extent: inner corners exist at
    /// `0..cols` × `0..rows` and nowhere else. A grid corner the frame places
    /// outside that range is therefore not an inner corner of this board — at
    /// the board's edge it is the T-junction where the outer squares meet the
    /// margin, a lattice point the printed pattern does not make an X-corner
    /// of — and delivering it would ship a label outside the board, negative
    /// on the low side. It is dropped. Every delivered corner carries an `id`.
    ///
    /// `inliers` runs parallel to `detection.corners` (each corner's index in
    /// the input corner slice); the returned list runs parallel to the
    /// filtered, sorted corners.
    fn apply_board_frame(
        &self,
        detection: &mut TargetDetection,
        inliers: Vec<usize>,
        alignment: GridAlignment,
    ) -> Vec<usize> {
        let cols = i32::try_from(self.params.board.cols).unwrap_or(i32::MAX);
        let rows = i32::try_from(self.params.board.rows).unwrap_or(i32::MAX);
        let cell_size = self
            .params
            .board
            .cell_size
            .filter(|s| s.is_finite() && *s > 0.0);

        let mut kept: Vec<(LabeledCorner, usize)> = std::mem::take(&mut detection.corners)
            .into_iter()
            .zip(inliers)
            .filter_map(|(mut corner, input_index)| {
                // INVARIANT: every corner reaching this stage came from the
                // chessboard detector with a grid coordinate.
                let grid = alignment.apply(corner.grid?);
                if !((0..cols).contains(&grid.u) && (0..rows).contains(&grid.v)) {
                    return None;
                }
                corner.grid = Some(grid);
                corner.id = (grid.v as u32)
                    .checked_mul(self.params.board.cols)
                    .and_then(|base| base.checked_add(grid.u as u32));
                if let Some(size) = cell_size {
                    // Inner corner `(u, v)` sits one full square in from the
                    // board's outer corner, which is where board space starts —
                    // the same origin the printable spec's `resolved_points`
                    // measures from.
                    corner.target_position = Some(Point2::new(
                        (grid.u + 1) as f32 * size,
                        (grid.v + 1) as f32 * size,
                    ));
                }
                Some((corner, input_index))
            })
            .collect();

        kept.sort_by_key(|(corner, _)| corner.grid.map(|g| (g.v, g.u)));
        let (corners, inliers) = kept.into_iter().unzip();
        detection.corners = corners;
        inliers
    }
}

fn validate_marker_board_params(params: &MarkerBoardParams) -> Result<(), MarkerBoardParamsError> {
    let board = &params.board;
    if board.rows < 2 || board.cols < 2 {
        return Err(MarkerBoardParamsError::InvalidBoardDimensions {
            rows: board.rows,
            cols: board.cols,
        });
    }
    if let Some(cell_size) = board.cell_size {
        if !cell_size.is_finite() || cell_size <= 0.0 {
            return Err(MarkerBoardParamsError::InvalidCellSize);
        }
    }
    if !board.circle_diameter_rel.is_finite()
        || board.circle_diameter_rel <= 0.0
        || board.circle_diameter_rel > 1.0
    {
        return Err(MarkerBoardParamsError::InvalidCircleDiameter);
    }

    let mut seen = [(0i32, 0i32); 3];
    for (idx, circle) in board.circles.iter().enumerate() {
        let (ci, cj) = (circle.cell.i, circle.cell.j);
        if ci < 0 || cj < 0 || (ci as u32) > board.cols || (cj as u32) > board.rows {
            return Err(MarkerBoardParamsError::CircleOutOfBounds {
                i: ci,
                j: cj,
                cols: board.cols,
                rows: board.rows,
            });
        }
        if seen[..idx].contains(&(ci, cj)) {
            return Err(MarkerBoardParamsError::DuplicateCircleCells);
        }
        seen[idx] = (ci, cj);
    }

    let cs = &params.circle_score;
    if cs.patch_size < 4
        || cs.samples < 1
        || cs.center_search_px < 0
        || !(cs.min_contrast.is_finite() && (0.0..=255.0).contains(&cs.min_contrast))
        || !(cs.ring_thickness_frac.is_finite() && cs.ring_thickness_frac >= 0.0)
        || !(cs.ring_radius_mul.is_finite()
            && cs.ring_radius_mul > 0.0
            && cs.ring_radius_mul > 0.5 * cs.ring_thickness_frac)
    {
        return Err(MarkerBoardParamsError::InvalidCircleScoreParams);
    }

    Ok(())
}

fn relabel_as_marker(mut detection: TargetDetection) -> TargetDetection {
    detection.kind = TargetKind::CheckerboardMarker;
    detection
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CellCoords, CirclePolarity, MarkerBoardSpec, MarkerCircleSpec};
    use calib_targets_core::{Coord, GRID_TRANSFORMS_C4};

    fn detector() -> MarkerBoardDetector {
        let circles = [
            MarkerCircleSpec::new(CellCoords { i: 0, j: 0 }, CirclePolarity::White),
            MarkerCircleSpec::new(CellCoords { i: 1, j: 0 }, CirclePolarity::Black),
            MarkerCircleSpec::new(CellCoords { i: 1, j: 1 }, CirclePolarity::White),
        ];
        let board = MarkerBoardSpec::new(3, 4, circles).with_cell_size(10.0);
        MarkerBoardDetector::new(MarkerBoardParams::for_board(board)).expect("valid params")
    }

    /// A grid corner the frame places off the board — the markerboard sample's
    /// top-edge T-junction landed at `(8, -1)` — is not an inner corner of the
    /// board and is not delivered; every delivered corner carries an id, and
    /// the provenance list stays parallel to the corners after the filter and
    /// the sort.
    #[test]
    fn corners_off_the_board_are_not_delivered() {
        // (grid, input index); the board has 4 columns and 3 rows of corners.
        let grid = [
            ((3, 2), 10),
            ((-1, 0), 11),
            ((0, 0), 12),
            ((4, 0), 13),
            ((2, 1), 14),
            ((0, 3), 15),
            ((1, -1), 16),
        ];
        let corners = grid
            .iter()
            .map(|&((u, v), _)| {
                LabeledCorner::new(Point2::new(u as f32, v as f32), 1.0).with_grid(Coord::new(u, v))
            })
            .collect();
        let mut detection = TargetDetection::new(TargetKind::CheckerboardMarker, corners);
        let inliers = grid.iter().map(|&(_, index)| index).collect();
        let identity = GRID_TRANSFORMS_C4[0].with_translation([0, 0]);

        let inliers = detector().apply_board_frame(&mut detection, inliers, identity);

        let delivered: Vec<_> = detection
            .corners
            .iter()
            .map(|c| (c.grid.map(|g| (g.u, g.v)), c.id))
            .collect();
        assert_eq!(
            delivered,
            vec![
                (Some((0, 0)), Some(0)),
                (Some((2, 1)), Some(6)),
                (Some((3, 2)), Some(11)),
            ]
        );
        assert_eq!(inliers, vec![12, 14, 10]);
        assert_eq!(
            detection.corners[2].target_position,
            Some(Point2::new(40.0, 30.0))
        );
    }

    #[test]
    fn new_validates_board_spec_circles_and_score_params() {
        let mut p = MarkerBoardParams::default();
        p.board.rows = 1;
        assert!(matches!(
            MarkerBoardDetector::new(p),
            Err(MarkerBoardParamsError::InvalidBoardDimensions { rows: 1, .. })
        ));

        let mut p = MarkerBoardParams::default();
        p.board.cell_size = Some(-1.0);
        assert!(matches!(
            MarkerBoardDetector::new(p),
            Err(MarkerBoardParamsError::InvalidCellSize)
        ));

        let mut p = MarkerBoardParams::default();
        p.board.circle_diameter_rel = 0.0;
        assert!(matches!(
            MarkerBoardDetector::new(p),
            Err(MarkerBoardParamsError::InvalidCircleDiameter)
        ));

        let mut p = MarkerBoardParams::default();
        p.board.circles[0].cell = CellCoords {
            i: p.board.cols as i32 + 1,
            j: 0,
        };
        assert!(matches!(
            MarkerBoardDetector::new(p),
            Err(MarkerBoardParamsError::CircleOutOfBounds { .. })
        ));

        let mut p = MarkerBoardParams::default();
        p.board.circles[1].cell = p.board.circles[0].cell;
        assert!(matches!(
            MarkerBoardDetector::new(p),
            Err(MarkerBoardParamsError::DuplicateCircleCells)
        ));

        let mut p = MarkerBoardParams::default();
        p.circle_score.patch_size = 2;
        assert!(matches!(
            MarkerBoardDetector::new(p),
            Err(MarkerBoardParamsError::InvalidCircleScoreParams)
        ));
    }

    #[test]
    fn empty_post_filter_corners_return_alignment_failed_preserving_diagnostics() {
        let det = detector();
        // Board is 3×4 inner corners -> valid inner corners are (0..4, 0..3).
        // Provide corners only at (10..12, 10..12) so none lie in [0,4)×[0,3).
        let chess_corners: Vec<_> = (10..12)
            .flat_map(|v| {
                (10..12).map(move |u| {
                    calib_targets_chessboard::ChessboardCorner::new(
                        Point2::new(u as f32 * 10.0, v as f32 * 10.0),
                        Coord::new(u, v),
                        (v * 10 + u) as usize,
                        1.0,
                    )
                })
            })
            .collect();
        let chess = ChessboardDetection::new(chess_corners).with_cell_size(10.0);
        let cand = CircleCandidate {
            center_img: Point2::new(5.0, 5.0),
            cell: CellCoords { i: 0, j: 0 },
            polarity: CirclePolarity::White,
            score: 50.0,
            contrast: 50.0,
            squareness: 0.0,
        };
        let frame = FrameSearch {
            matches: Vec::new(),
            alignment: Some(GridAlignment::IDENTITY),
            inliers: 3,
            runner_up_inliers: 0,
            ambiguous: false,
        };
        let (res, diags) =
            det.result_from_chessboard(chess, vec![cand; 4], frame, GridAlignment::IDENTITY);
        assert!(matches!(
            res,
            Err(MarkerBoardDetectError::AlignmentFailed { candidates: 4, .. })
        ));
        assert_eq!(diags.circle_candidates.len(), 4);
    }

    #[test]
    fn pre_truncation_circle_candidates_are_preserved_in_diagnostics() {
        // 6×8 inner corners = 7×9 squares (31+ squares of each colour, meeting
        // `MIN_SQUARES_PER_COLOUR = 7`), at 40 px per square.
        let s = 40i32;
        let ox = 20i32;
        let oy = 20i32;
        let inner_rows = 6i32;
        let inner_cols = 8i32;
        let w = ((inner_cols + 1) * s + 2 * ox) as usize;
        let h = ((inner_rows + 1) * s + 2 * oy) as usize;
        let mut data = vec![230u8; w * h];
        for y in 0..h {
            for x in 0..w {
                let cx = (x as i32 - ox).div_euclid(s);
                let cy = (y as i32 - oy).div_euclid(s);
                if (0..=inner_cols).contains(&cx) && (0..=inner_rows).contains(&cy) {
                    let black_sq = (cx + cy) % 2 == 0;
                    let dx = (x as f32 + 0.5) - (ox as f32 + (cx as f32 + 0.5) * s as f32);
                    let dy = (y as f32 + 0.5) - (oy as f32 + (cy as f32 + 0.5) * s as f32);
                    let r2 = dx * dx + dy * dy;
                    let disk_r2 = (0.25 * s as f32).powi(2);
                    let is_circle = matches!((cx, cy), (2, 2) | (3, 2) | (2, 3));
                    let dark = if is_circle && r2 <= disk_r2 {
                        !black_sq
                    } else {
                        black_sq
                    };
                    data[y * w + x] = if dark { 20 } else { 235 };
                }
            }
        }
        let view = GrayImageView {
            width: w,
            height: h,
            data: &data,
        };
        let circles = [
            MarkerCircleSpec::new(CellCoords { i: 2, j: 2 }, CirclePolarity::White),
            MarkerCircleSpec::new(CellCoords { i: 3, j: 2 }, CirclePolarity::Black),
            MarkerCircleSpec::new(CellCoords { i: 2, j: 3 }, CirclePolarity::White),
        ];
        let mut params = MarkerBoardParams::for_board(MarkerBoardSpec::new(
            inner_rows as u32,
            inner_cols as u32,
            circles,
        ));
        let corners = calib_targets_chessboard::detect_corners(&view, &params.chess);
        // Only keep 1 candidate per polarity -> cannot align all 3 circles (2 white + 1 black),
        // yet diagnostics must preserve all 3 pre-truncation candidates.
        params.match_params.max_candidates_per_polarity = 1;
        let det = MarkerBoardDetector::new(params).expect("valid params");
        let (res, diags) = det.detect_inner(&view, &corners);
        assert!(
            diags.circle_candidates.len() >= 3,
            "expected all >=3 raw candidates before truncation, got {}",
            diags.circle_candidates.len()
        );
        assert!(matches!(
            res,
            Err(MarkerBoardDetectError::AlignmentFailed { candidates, .. }) if candidates == diags.circle_candidates.len()
        ));
    }
}
