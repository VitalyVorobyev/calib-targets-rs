use crate::circle_score::CircleCandidate;
use crate::detect::{detect_circles_via_square_warp, top_k_by_polarity};
use crate::diagnostics::MarkerBoardDiagnostics;
use crate::error::MarkerBoardDetectError;
use crate::match_circles::{resolve_board_frame, FrameSearch};
use crate::types::{MarkerBoardDetection, MarkerBoardParams};

use nalgebra::Point2;

use calib_targets_chessboard::ChessCorner;
use calib_targets_chessboard::{
    ChessboardDetection, ChessboardDetector as ChessDetector, ChessboardParamsError,
};
use calib_targets_core::{
    CornerMap, GrayImageView, GridAlignment, LabeledCorner, TargetDetection, TargetKind,
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
    /// whole pipeline. It is exactly
    /// `self.detect_with_corners(image, &self.detect_corners(image))` — reach
    /// for [`Self::detect_with_corners`] when you already have a corner cloud
    /// (a custom upstream, or one shared across detectors).
    ///
    /// # Errors
    ///
    /// See [`MarkerBoardDetectError`].
    pub fn detect(
        &self,
        image: &GrayImageView<'_>,
    ) -> Result<MarkerBoardDetection, MarkerBoardDetectError> {
        self.detect_with_corners(image, &self.detect_corners(image))
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
        self.diagnose_with_corners(image, &self.detect_corners(image))
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
        self.apply_board_frame(&mut detection, alignment);
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

    /// Relabel every corner into the board frame, then attach the
    /// board-canonical id and physical position to the ones that land inside
    /// the board.
    fn apply_board_frame(&self, detection: &mut TargetDetection, alignment: GridAlignment) {
        for corner in &mut detection.corners {
            if let Some(grid) = &mut corner.grid {
                let g = alignment.apply(*grid);
                grid.u = g.u;
                grid.v = g.v;
            }
        }

        let Some((cols, rows)) = i32::try_from(self.params.board.cols)
            .ok()
            .zip(i32::try_from(self.params.board.rows).ok())
        else {
            return;
        };
        let cell_size = self
            .params
            .board
            .cell_size
            .filter(|s| s.is_finite() && *s > 0.0);
        for corner in &mut detection.corners {
            let Some(grid) = corner.grid else {
                continue;
            };
            if grid.u < 0 || grid.v < 0 || grid.u >= cols || grid.v >= rows {
                continue;
            }
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
        }

        detection.corners.sort_by(|a, b| {
            // INVARIANT: every corner reaching this stage came from the
            // chessboard detector with a grid coordinate, and the relabelling
            // above preserves it.
            let ga = a.grid.unwrap();
            let gb = b.grid.unwrap();
            (ga.v, ga.u).cmp(&(gb.v, gb.u))
        });
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
fn corner_frame(alignment: GridAlignment) -> GridAlignment {
    let [di, dj] = alignment.translation();
    alignment.with_translation([di - 1, dj - 1])
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

fn build_corner_map(det: &ChessboardDetection) -> CornerMap {
    det.corners.iter().map(|c| (c.grid, c.position)).collect()
}
