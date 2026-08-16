use crate::circle_score::CircleCandidate;
use crate::detect::{detect_circles_via_square_warp, top_k_by_polarity};
use crate::diagnostics::MarkerBoardDiagnostics;
use crate::error::MarkerBoardDetectError;
use crate::match_circles::{estimate_grid_alignment, match_expected_circles};
use crate::types::{CircleMatch, MarkerBoardDetection, MarkerBoardParams};

use nalgebra::Point2;

use calib_targets_chessboard::ChessCorner;
use calib_targets_chessboard::{
    ChessboardDetection, ChessboardDetector as ChessDetector, ChessboardParamsError,
};
use calib_targets_core::{
    Coord, CornerMap, GrayImageView, GridAlignment, LabeledCorner, TargetDetection, TargetKind,
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

        let mut candidates =
            detect_circles_via_square_warp(image, &corner_map, &self.params.circle_score, roi);

        let max_per = self.params.match_params.max_candidates_per_polarity;
        if max_per > 0 && !candidates.is_empty() {
            let (white, black) = top_k_by_polarity(candidates, max_per, max_per);
            candidates = [white, black].concat();
        }

        let mut matches = match_expected_circles(
            &self.params.board.circles,
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

        let Some(alignment) = alignment else {
            // Counted before the vectors move into the diagnostics struct.
            let matched = matches.iter().filter(|m| m.matched_index.is_some()).count();
            let candidate_count = candidates.len();
            return (
                Err(MarkerBoardDetectError::AlignmentFailed {
                    matched,
                    candidates: candidate_count,
                }),
                MarkerBoardDiagnostics {
                    inliers: Vec::new(),
                    circle_candidates: candidates,
                    circle_matches: matches,
                    alignment_inliers: 0,
                },
            );
        };
        for m in &mut matches {
            let Some(idx) = m.matched_index else {
                continue;
            };
            let Some(cand) = candidates.get(idx) else {
                continue;
            };
            let r = alignment
                .with_translation([0, 0])
                .apply(Coord::new(cand.cell.i, cand.cell.j));
            m.offset_cells = Some(crate::coords::CellOffset {
                di: m.expected.cell.i - r.u,
                dj: m.expected.cell.j - r.v,
            });
        }

        let (detection, diagnostics) = self.result_from_chessboard(
            chess,
            candidates,
            matches,
            Some(alignment),
            alignment_inliers,
        );
        (Ok(detection), diagnostics)
    }

    fn result_from_chessboard(
        &self,
        chess: ChessboardDetection,
        circle_candidates: Vec<CircleCandidate>,
        circle_matches: Vec<CircleMatch>,
        alignment: Option<GridAlignment>,
        alignment_inliers: usize,
    ) -> (MarkerBoardDetection, MarkerBoardDiagnostics) {
        let (target, inliers) = chessboard_detection_to_target(&chess);
        let mut detection = relabel_as_marker(target);
        if let Some(alignment) = alignment {
            for corner in &mut detection.corners {
                if let Some(grid) = &mut corner.grid {
                    let g = alignment.apply(*grid);
                    grid.u = g.u;
                    grid.v = g.v;
                }
            }

            let cols = i32::try_from(self.params.board.cols).ok();
            let rows = i32::try_from(self.params.board.rows).ok();
            if let Some((cols, rows)) = cols.zip(rows) {
                let cell_size = self.params.board.cell_size;
                for corner in &mut detection.corners {
                    let Some(grid) = corner.grid else {
                        continue;
                    };
                    if grid.u < 0 || grid.v < 0 || grid.u >= cols || grid.v >= rows {
                        continue;
                    }
                    let id = (grid.v as u32)
                        .checked_mul(self.params.board.cols)
                        .and_then(|base| base.checked_add(grid.u as u32));
                    corner.id = id;
                    if let Some(size) = cell_size.filter(|s| s.is_finite() && *s > 0.0) {
                        corner.target_position =
                            Some(Point2::new(grid.u as f32 * size, grid.v as f32 * size));
                    }
                }

                detection.corners.sort_by(|a, b| {
                    // INVARIANT: grid coordinates are always populated for every corner
                    // at this point — they are assigned in the loop above via the
                    // `set_grid_coords_from_chess` call before this sort.
                    let ga = a.grid.unwrap();
                    let gb = b.grid.unwrap();
                    (ga.v, ga.u).cmp(&(gb.v, gb.u))
                });
            }
        }
        (
            MarkerBoardDetection::from_target_detection(detection, alignment),
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

fn build_corner_map(det: &ChessboardDetection) -> CornerMap {
    det.corners.iter().map(|c| (c.grid, c.position)).collect()
}
