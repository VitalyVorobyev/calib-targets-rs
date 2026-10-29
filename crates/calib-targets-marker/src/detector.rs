use crate::circle_score::CircleCandidate;
use crate::detect::{detect_circles_via_square_warp, top_k_by_polarity};
use crate::diagnostics::MarkerBoardDiagnostics;
use crate::error::MarkerBoardDetectError;
use crate::match_circles::{resolve_board_frame, FrameSearch};
use crate::types::{MarkerBoardDetection, MarkerBoardParams};

use nalgebra::Point2;

use calib_targets_chessboard::{detect_corner_frame, ChessCorner};
use calib_targets_chessboard::{
    ChessboardDetection, ChessboardDetector as ChessDetector, ChessboardParamsError,
};
use calib_targets_core::{
    cell_to_corner_alignment, CornerMap, GrayImageView, GridAlignment, LabeledCorner,
    TargetDetection, TargetKind,
};

/// Marker board detector: chessboard + three circle markers.
pub struct MarkerBoardDetector {
    params: MarkerBoardParams,
    chessboard_detector: ChessDetector,
}

impl MarkerBoardDetector {
    /// Construct a marker-board detector from its parameters.
    ///
    /// Returns the chessboard detector's typed [`ChessboardParamsError`] if the
    /// embedded [`MarkerBoardParams::chessboard`](crate::MarkerBoardParams)
    /// configuration is one the chessboard stage cannot honour.
    pub fn new(params: MarkerBoardParams) -> Result<Self, ChessboardParamsError> {
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
        let corner_map = build_corner_map(&chess);
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

        let max_per = self.params.match_params.max_candidates_per_polarity;
        if max_per > 0 && !candidates.is_empty() {
            let (white, black) = top_k_by_polarity(candidates, max_per, max_per);
            candidates = [white, black].concat();
        }

        let frame = resolve_board_frame(
            &self.params.board.circles,
            &candidates,
            &self.params.match_params,
        );

        let Some(alignment) = frame.alignment else {
            // Counted before the vectors move into the diagnostics struct.
            let matched = frame
                .matches
                .iter()
                .filter(|m| m.matched_index.is_some())
                .count();
            let candidate_count = candidates.len();
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
                    circle_candidates: candidates,
                    circle_matches: frame.matches,
                    alignment_inliers: frame.inliers,
                    alignment_runner_up_inliers: frame.runner_up_inliers,
                    alignment_ambiguous: frame.ambiguous,
                },
            );
        };

        let (detection, diagnostics) =
            self.result_from_chessboard(chess, candidates, frame, corner_frame(alignment));
        (Ok(detection), diagnostics)
    }

    fn result_from_chessboard(
        &self,
        chess: ChessboardDetection,
        circle_candidates: Vec<CircleCandidate>,
        frame: FrameSearch,
        alignment: GridAlignment,
    ) -> (MarkerBoardDetection, MarkerBoardDiagnostics) {
        let (target, inliers) = chessboard_detection_to_target(&chess);
        let mut detection = relabel_as_marker(target);
        let inliers = self.apply_board_frame(&mut detection, inliers, alignment);
        (
            MarkerBoardDetection::from_target_detection(detection, Some(alignment)),
            MarkerBoardDiagnostics {
                inliers,
                circle_candidates,
                circle_matches: frame.matches,
                alignment_inliers: frame.inliers,
                alignment_runner_up_inliers: frame.runner_up_inliers,
                alignment_ambiguous: frame.ambiguous,
            },
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

/// Re-express an accepted board frame in inner-corner coordinates.
///
/// A circle layout is written in *square* indices — cell `(i, j)` is the
/// square whose top-left corner is grid `(i, j)` — so a frame that carries
/// detected cells onto board cells labels corners `1..=cols`, leaving the last
/// row and column of a fully detected board outside the board's own index
/// range and stripped of their ids. Every other surface indexes inner corners
/// from zero: the printable spec's `resolved_points`, the board-canonical
/// corner ids, and the workspace rule that a labelled set's bounding-box
/// minimum is `(0, 0)`. One shift converts, and it belongs here rather than in
/// the layout, so a hand-written `MarkerBoardSpec` keeps naming squares.
///
/// The frame is found by matching cells, so before that shift it is turned
/// into the alignment of the cells' corners: under a rotation a cell's
/// top-left corner lands on another corner of its board cell.
fn corner_frame(alignment: GridAlignment) -> GridAlignment {
    let [di, dj] = cell_to_corner_alignment(alignment).translation();
    alignment.with_translation([di - 1, dj - 1])
}

/// Adapt a [`ChessboardDetection`] into the generic [`TargetDetection`]
/// the marker pipeline operates on, plus the parallel input-index list
/// the marker diagnostics expose as `inliers`.
fn chessboard_detection_to_target(chess: &ChessboardDetection) -> (TargetDetection, Vec<usize>) {
    let mut corners = Vec::with_capacity(chess.corners.len());
    let mut inliers = Vec::with_capacity(chess.corners.len());
    for c in &chess.corners {
        corners.push(c.to_labeled());
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

fn build_corner_map(det: &ChessboardDetection) -> CornerMap {
    det.corners.iter().map(|c| (c.grid, c.position)).collect()
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
}
