use super::alignment_select::select_alignment;
use super::board_match::{match_board_diag, BoardMatchConfig, BoardMatchDiagnostics};
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

/// Rich per-frame diagnostics captured by [`CharucoDetector::detect_with_diagnostics`].
///
/// One entry per chessboard connected component the detector tried to
/// match; fail-early stages (no chessboard) produce an empty list.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct CharucoDetectDiagnostics {
    pub components: Vec<ComponentDiagnostics>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ComponentDiagnostics {
    pub index: usize,
    pub chess_corner_count: usize,
    pub candidate_cell_count: usize,
    /// Which matcher branch produced this component. Callers get the
    /// board-level diagnostics only when
    /// [`CharucoParams::use_board_level_matcher`] is `true`.
    pub matcher: MatcherDiagKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub board: Option<BoardMatchDiagnostics>,
    /// Final detection outcome for this component.
    pub outcome: ComponentOutcome,
}

#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatcherDiagKind {
    Legacy,
    BoardLevel,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ComponentOutcome {
    Ok {
        markers: usize,
        charuco_corners: usize,
    },
    Failed {
        reason: String,
    },
}

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
        self.detect_inner(image, corners).0
    }

    /// Detect + return per-component diagnostics (matcher decisions, per-cell
    /// scores, chosen/runner-up hypotheses, rejection reasons). The caller
    /// receives diagnostics even when detection fails, so overlays can
    /// render failure modes.
    pub fn detect_with_diagnostics(
        &self,
        image: &GrayImageView<'_>,
        corners: &[Corner],
    ) -> (
        Result<CharucoDetectionResult, CharucoDetectError>,
        CharucoDetectDiagnostics,
    ) {
        self.detect_inner(image, corners)
    }

    fn detect_inner(
        &self,
        image: &GrayImageView<'_>,
        corners: &[Corner],
    ) -> (
        Result<CharucoDetectionResult, CharucoDetectError>,
        CharucoDetectDiagnostics,
    ) {
        let mut diagnostics = CharucoDetectDiagnostics::default();

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
            return (Err(CharucoDetectError::ChessboardNotDetected), diagnostics);
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

            let (result, comp_diag) = self.detect_component(image, chessboard, min_inliers, i);
            match result {
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
            diagnostics.components.push(comp_diag);
        }

        if results.is_empty() {
            return (Err(CharucoDetectError::NoMarkers), diagnostics);
        }

        let merged = if results.len() == 1 {
            results.into_iter().next().unwrap()
        } else {
            merge_charuco_results(results)
        };
        (Ok(merged), diagnostics)
    }

    /// Run the full charuco pipeline on a single chessboard component.
    fn detect_component(
        &self,
        image: &GrayImageView<'_>,
        chessboard: &ChessDetection,
        min_marker_inliers: usize,
        component_index: usize,
    ) -> (
        Result<CharucoDetectionResult, CharucoDetectError>,
        ComponentDiagnostics,
    ) {
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

        let chess_corner_count = chessboard.target.corners.len();
        let candidate_cell_count = cells.len();
        let scan_cfg = self.params.scan.clone();

        let use_board_level = self.params.use_board_level_matcher;
        let mut board_diag: Option<BoardMatchDiagnostics> = None;

        let matched = if use_board_level {
            let board_cfg = BoardMatchConfig {
                px_per_square: self.params.px_per_square,
                bit_likelihood_slope: self.params.bit_likelihood_slope,
                per_bit_floor: self.params.per_bit_floor,
                alignment_min_margin: self.params.alignment_min_margin,
                cell_weight_border_threshold: self.params.cell_weight_border_threshold,
            };
            let (matched, diag) =
                match_board_diag(image, &cells, &self.board, &scan_cfg, &board_cfg);
            board_diag = Some(diag);
            match matched {
                Some((markers, alignment)) => {
                    let count = markers.len();
                    Ok((markers, alignment, count, 0usize))
                }
                None => {
                    warn!(
                        "board-level matcher rejected: no hypothesis cleared the margin gate ({} candidate cells)",
                        cells.len()
                    );
                    Err(CharucoDetectError::AlignmentFailed { inliers: 0 })
                }
            }
        } else {
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
                Err(CharucoDetectError::NoMarkers)
            } else {
                let raw_count = markers.len();
                let raw_snapshot = markers.clone();
                match self.select_and_refine_markers(markers) {
                    Some((markers, alignment)) => {
                        let wrong_id =
                            count_wrong_id_raw_markers(&self.board, &raw_snapshot, &alignment);
                        Ok((markers, alignment, raw_count, wrong_id))
                    }
                    None => {
                        warn!("marker-to-board alignment failed before producing any inliers");
                        Err(CharucoDetectError::AlignmentFailed { inliers: 0 })
                    }
                }
            }
        };

        let make_comp_diag = |outcome: ComponentOutcome, board: Option<BoardMatchDiagnostics>| {
            ComponentDiagnostics {
                index: component_index,
                chess_corner_count,
                candidate_cell_count,
                matcher: if use_board_level {
                    MatcherDiagKind::BoardLevel
                } else {
                    MatcherDiagKind::Legacy
                },
                board,
                outcome,
            }
        };

        let (markers, alignment, raw_marker_count, raw_marker_wrong_id_count) = match matched {
            Ok(tup) => tup,
            Err(e) => {
                let comp_diag = make_comp_diag(
                    ComponentOutcome::Failed {
                        reason: e.to_string(),
                    },
                    board_diag,
                );
                return (Err(e), comp_diag);
            }
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
            let err = CharucoDetectError::AlignmentFailed {
                inliers: alignment.marker_inliers.len(),
            };
            let comp_diag = make_comp_diag(
                ComponentOutcome::Failed {
                    reason: err.to_string(),
                },
                board_diag,
            );
            return (Err(err), comp_diag);
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

        let comp_diag = make_comp_diag(
            ComponentOutcome::Ok {
                markers: markers.len(),
                charuco_corners: detection.corners.len(),
            },
            board_diag,
        );

        (
            Ok(CharucoDetectionResult {
                detection,
                markers,
                alignment: alignment.alignment,
                raw_marker_count,
                raw_marker_wrong_id_count,
            }),
            comp_diag,
        )
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

/// Count raw marker decodings that map to a valid board position but
/// disagree with the chosen alignment.
///
/// Decodings whose id does not correspond to any marker on this board are
/// treated as dictionary noise and excluded from this count.
fn count_wrong_id_raw_markers(
    board: &CharucoBoard,
    raw_markers: &[MarkerDetection],
    alignment: &CharucoAlignment,
) -> usize {
    raw_markers
        .iter()
        .filter(|m| {
            let Some(expected_bc) = board.marker_position(m.id) else {
                return false; // pure dict noise — not counted as "wrong id"
            };
            let [bx, by] = alignment.map(m.gc.i, m.gc.j);
            bx != expected_bc.i || by != expected_bc.j
        })
        .count()
}
