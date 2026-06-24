use super::board_match::{match_board, BoardMatchConfig};
#[cfg(feature = "diagnostics")]
use super::board_match::{match_board_diag, BoardMatchDiagnostics};
use super::corner_mapping::map_charuco_corners;
use super::corner_refit::{validate_and_fix_corners, CornerValidationConfig};
use super::grid_smoothness::smooth_grid_corners;
use super::marker_sampling::{build_corner_map, build_marker_cells};
use super::merge::merge_charuco_results;
use super::params::to_chess_params;
use super::{CharucoDetectError, CharucoDetectionResult, CharucoParams};
use crate::alignment::CharucoAlignment;
use crate::board::{CharucoBoard, CharucoBoardError};
use calib_targets_aruco::MarkerDetection;
use calib_targets_chessboard::ChessCorner;
use calib_targets_chessboard::{ChessboardDetection, Detector as ChessDetector};
use calib_targets_core::{GrayImageView, LabeledCorner, TargetDetection, TargetKind};
use log::{debug, warn};

/// Adapt a [`ChessboardDetection`] into the generic [`TargetDetection`]
/// the ChArUco corner-mapping / marker-sampling stages consume. Every
/// corner the chessboard detector emits is a validated labelled grid
/// point, so all of them carry a non-optional `grid`.
fn chessboard_detection_to_target(chess: &ChessboardDetection) -> TargetDetection {
    let corners = chess
        .corners
        .iter()
        .map(|c| LabeledCorner::new(c.position, c.score).with_grid(c.grid))
        .collect();
    TargetDetection::new(TargetKind::Chessboard, corners)
}

/// Rich per-frame diagnostics captured by [`CharucoDetector::detect_with_diagnostics`].
///
/// One entry per chessboard connected component the detector tried to
/// match; fail-early stages (no chessboard) produce an empty list.
#[cfg(feature = "diagnostics")]
#[derive(Clone, Debug, Default, serde::Serialize)]
#[non_exhaustive]
pub struct CharucoDetectDiagnostics {
    /// One entry per chessboard connected component the detector tried
    /// to match.
    pub components: Vec<ComponentDiagnostics>,
    /// Total number of markers decoded out of candidate cells, **before**
    /// alignment-based inlier filtering, summed across the components that
    /// contributed to the returned [`CharucoDetectionResult`].
    ///
    /// `raw_marker_count - result.markers.len()` is the number of raw
    /// marker decodings rejected by the alignment stage.
    pub raw_marker_count: usize,
    /// Raw decodings whose id mapped to a valid board position that
    /// **disagreed** with the chosen alignment — a self-consistency
    /// wrong-id count. It excludes pure dictionary-noise decodings whose id
    /// did not correspond to any marker on this board.
    pub raw_marker_wrong_id_count: usize,
}

/// Per-component diagnostics for one chessboard connected component.
#[cfg(feature = "diagnostics")]
#[derive(Clone, Debug, serde::Serialize)]
#[non_exhaustive]
pub struct ComponentDiagnostics {
    /// Zero-based index of this component.
    pub index: usize,
    /// Number of chessboard corners in this component.
    pub chess_corner_count: usize,
    /// Number of candidate marker cells extracted from this component.
    pub candidate_cell_count: usize,
    /// Board-level matcher diagnostics (per-cell scores, chosen/runner-up
    /// hypotheses, margin, rejection reason).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub board: Option<BoardMatchDiagnostics>,
    /// Final detection outcome for this component.
    pub outcome: ComponentOutcome,
}

/// Final outcome of detecting one chessboard component.
#[cfg(feature = "diagnostics")]
#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ComponentOutcome {
    /// The component yielded a detection.
    Ok {
        /// Number of markers in the component's detection.
        markers: usize,
        /// Number of ChArUco corners in the component's detection.
        charuco_corners: usize,
        /// Raw markers decoded for this component before alignment-based
        /// inlier filtering.
        raw_marker_count: usize,
        /// Raw decodings for this component whose id disagreed with the
        /// chosen alignment.
        raw_marker_wrong_id_count: usize,
    },
    /// The component produced no detection.
    Failed {
        /// Human-readable reason the component failed.
        reason: String,
    },
}

/// Per-component raw marker counts captured before alignment-based inlier
/// filtering. Threaded alongside each component's [`CharucoDetectionResult`]
/// so [`merge_charuco_results`] can sum the winning group into
/// [`CharucoDetectDiagnostics`].
///
/// These are cheap result-flavoured counters (no per-cell allocation), so
/// they are always computed and threaded through the merge — the
/// `diagnostics` feature only controls whether the merged totals are
/// *recorded* onto the public diagnostics surface.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct RawMarkerCounts {
    pub raw_marker_count: usize,
    pub raw_marker_wrong_id_count: usize,
}

/// Result of running the board-level matcher on one component, together with
/// the per-component raw counts.
struct ComponentMatch {
    markers: Vec<MarkerDetection>,
    alignment: CharucoAlignment,
    raw_counts: RawMarkerCounts,
}

/// Identifying counts for one chessboard component, shared by the
/// [`PipelineSink`] outcome hooks. Bundled into a struct so the hooks stay
/// within the workspace argument-count limit.
///
/// The fields are diagnostics-only metadata (consumed solely by
/// [`DiagCollector`]), so they are compiled in only with the `diagnostics`
/// feature; on the production path the value is an empty marker the no-op
/// hooks ignore.
#[derive(Clone, Copy)]
struct ComponentContext {
    #[cfg(feature = "diagnostics")]
    index: usize,
    #[cfg(feature = "diagnostics")]
    chess_corner_count: usize,
    #[cfg(feature = "diagnostics")]
    candidate_cell_count: usize,
}

/// Sink for per-component / per-frame pipeline diagnostics.
///
/// Mirrors the board matcher's `MatchSink` split: the production [`detect`]
/// path uses the no-op [`NoPipelineDiag`] sink (all hooks inline away, so it
/// builds no `ComponentDiagnostics` / `CharucoDetectDiagnostics`), while
/// [`detect_with_diagnostics`] uses [`DiagCollector`], which runs the
/// diagnostic matcher and accumulates the full record.
///
/// The sink owns *which* board matcher runs (`match_board` vs
/// `match_board_diag`) so the production path never allocates a
/// [`BoardMatchDiagnostics`]. Hook parameters are restricted to
/// always-compiled types so the trait and the [`NoPipelineDiag`] impl compile
/// with the feature off.
trait PipelineSink {
    /// Run the board-level matcher for one component. The sink decides whether
    /// to capture per-cell board diagnostics; either way it returns the match
    /// result the pipeline consumes.
    fn run_match(
        &mut self,
        image: &GrayImageView<'_>,
        cells: &[calib_targets_aruco::MarkerCell],
        board: &CharucoBoard,
        scan_cfg: &calib_targets_aruco::ScanDecodeConfig,
        cfg: &BoardMatchConfig,
    ) -> Option<(Vec<MarkerDetection>, CharucoAlignment)>;

    /// Record a component that failed, with the reason and its corner / cell
    /// counts. `_reason` is materialised only by the diagnostics sink.
    fn component_failed(&mut self, _ctx: ComponentContext, _reason: &CharucoDetectError) {}

    /// Record a component that produced a detection.
    fn component_ok(
        &mut self,
        _ctx: ComponentContext,
        _markers: usize,
        _charuco_corners: usize,
        _raw_counts: RawMarkerCounts,
    ) {
    }

    /// Record the merged frame-level raw counts (the sum over the winning
    /// merge group).
    fn record_frame_counts(&mut self, _raw_counts: RawMarkerCounts) {}
}

/// No-op pipeline sink for the production [`detect`] path. Runs the lean
/// [`match_board`] and accumulates nothing.
struct NoPipelineDiag;

impl PipelineSink for NoPipelineDiag {
    fn run_match(
        &mut self,
        image: &GrayImageView<'_>,
        cells: &[calib_targets_aruco::MarkerCell],
        board: &CharucoBoard,
        scan_cfg: &calib_targets_aruco::ScanDecodeConfig,
        cfg: &BoardMatchConfig,
    ) -> Option<(Vec<MarkerDetection>, CharucoAlignment)> {
        match_board(image, cells, board, scan_cfg, cfg)
    }
}

#[cfg(feature = "tracing")]
use tracing::instrument;

/// Grid-first ChArUco detector.
#[derive(Debug)]
pub struct CharucoDetector {
    board: CharucoBoard,
    params: CharucoParams,
}

impl CharucoDetector {
    /// Create a detector from parameters (board spec lives in `params.board`).
    pub fn new(mut params: CharucoParams) -> Result<Self, CharucoBoardError> {
        let board_cfg = params.board;
        if !params.scan.marker_size_rel.is_finite() || params.scan.marker_size_rel <= 0.0 {
            params.scan.marker_size_rel = board_cfg.marker_size_rel;
        }

        let board = CharucoBoard::new(board_cfg)?;

        Ok(Self { board, params })
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
    ///
    /// This is the hot path: it computes and allocates **zero** diagnostics.
    // The link target exists only with the `diagnostics` feature; link it then,
    // and fall back to a plain mention otherwise so feature-off docs resolve.
    #[cfg_attr(
        feature = "diagnostics",
        doc = "For per-component evidence use [`CharucoDetector::detect_with_diagnostics`] (behind the `diagnostics` feature)."
    )]
    #[cfg_attr(
        not(feature = "diagnostics"),
        doc = "For per-component evidence enable the `diagnostics` feature and use `detect_with_diagnostics`."
    )]
    #[cfg_attr(feature = "tracing", instrument(level = "info", skip(self, image, corners), fields(num_corners=corners.len())))]
    pub fn detect(
        &self,
        image: &GrayImageView<'_>,
        corners: &[ChessCorner],
    ) -> Result<CharucoDetectionResult, CharucoDetectError> {
        self.detect_core(image, corners, &mut NoPipelineDiag)
    }

    /// Detect + return per-component diagnostics (matcher decisions, per-cell
    /// scores, chosen/runner-up hypotheses, rejection reasons). The caller
    /// receives diagnostics even when detection fails, so overlays can
    /// render failure modes.
    ///
    /// Available only with the `diagnostics` feature enabled.
    #[cfg(feature = "diagnostics")]
    pub fn detect_with_diagnostics(
        &self,
        image: &GrayImageView<'_>,
        corners: &[ChessCorner],
    ) -> (
        Result<CharucoDetectionResult, CharucoDetectError>,
        CharucoDetectDiagnostics,
    ) {
        let mut collector = DiagCollector::default();
        let result = self.detect_core(image, corners, &mut collector);
        (result, collector.diagnostics)
    }

    /// Shared detection core for both the production and diagnostics paths.
    ///
    /// Runs the chessboard stage, then drives every qualifying component
    /// through [`CharucoDetector::detect_component`], merges the consistent
    /// results, and routes all diagnostic capture through `sink`. The
    /// production path passes [`NoPipelineDiag`] (every hook inlines away).
    fn detect_core<S: PipelineSink>(
        &self,
        image: &GrayImageView<'_>,
        corners: &[ChessCorner],
        sink: &mut S,
    ) -> Result<CharucoDetectionResult, CharucoDetectError> {
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
        // ChArUco runs on the topological grid builder (the workspace default;
        // the `min_corner_strength` floor in `CharucoParams::for_board` keeps
        // marker-bit saddles out of the grid, and the marker-decode stages
        // downstream of grid construction are builder-agnostic).
        //
        // The chessboard detector still validates its own configuration; the
        // only combination it rejects is an orientation-source / graph-builder
        // mismatch (an orientation source ChArUco never sets, but a caller
        // could construct on the embedded `chessboard` field). Surface it as a
        // typed error rather than panicking.
        let chess_params = self.params.chessboard.clone();
        let detector = match ChessDetector::new(chess_params) {
            Ok(detector) => detector,
            Err(e) => {
                warn!("chessboard configuration rejected: {e}");
                return Err(CharucoDetectError::UnsupportedAlgorithm);
            }
        };
        let components = detector.detect_all(corners);

        if components.is_empty() {
            warn!(
                "chessboard stage failed: input_corners={}, min_corner_strength={:.3}, cluster_tol={:.1} deg, max_components={}",
                corners.len(),
                self.params.chessboard.min_corner_strength,
                self.params.chessboard.effective_tuning().cluster_tol_deg,
                self.params.chessboard.max_components,
            );
            return Err(CharucoDetectError::ChessboardNotDetected);
        }

        debug!(
            "chessboard stage produced {} qualifying components: {:?}",
            components.len(),
            components
                .iter()
                .map(|c| c.corners.len())
                .collect::<Vec<_>>()
        );

        let mut results: Vec<(CharucoDetectionResult, RawMarkerCounts)> = Vec::new();
        for (i, chessboard) in components.iter().enumerate() {
            let min_inliers = if i == 0 {
                self.params.min_marker_inliers
            } else {
                self.params.min_secondary_marker_inliers
            };

            match self.detect_component(image, chessboard, min_inliers, i, sink) {
                Ok((result, raw_counts)) => {
                    debug!(
                        "component {i}: {} corners, {} markers",
                        result.corners.len(),
                        result.markers.len()
                    );
                    results.push((result, raw_counts));
                }
                Err(e) => {
                    debug!("component {i} failed: {e}");
                }
            }
        }

        if results.is_empty() {
            return Err(CharucoDetectError::NoMarkers);
        }

        // Single-component and merged paths agree: the raw counts recorded on
        // the frame are exactly those of the components that contributed to
        // the returned result.
        let (merged, raw_counts) = if results.len() == 1 {
            results.into_iter().next().unwrap()
        } else {
            merge_charuco_results(results)
        };
        sink.record_frame_counts(raw_counts);
        Ok(merged)
    }

    /// Run the full charuco pipeline on a single chessboard component.
    ///
    /// On success returns the component's [`CharucoDetectionResult`] paired
    /// with its [`RawMarkerCounts`] (raw pre-inlier-filter totals); the
    /// counts are merged into [`CharucoDetectDiagnostics`] by the caller.
    /// All diagnostic capture is routed through `sink`, which also owns the
    /// choice of board matcher.
    fn detect_component<S: PipelineSink>(
        &self,
        image: &GrayImageView<'_>,
        chessboard: &ChessboardDetection,
        min_marker_inliers: usize,
        component_index: usize,
        sink: &mut S,
    ) -> Result<(CharucoDetectionResult, RawMarkerCounts), CharucoDetectError> {
        // Adapt the typed chessboard result into the generic
        // `TargetDetection` the corner-mapping / marker-sampling stages
        // expect. Every labelled corner is an inlier by construction.
        let chessboard = chessboard_detection_to_target(chessboard);
        let inliers: Vec<usize> = (0..chessboard.corners.len()).collect();
        let mut corner_map = build_corner_map(&chessboard.corners, &inliers);
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
            "component {component_index}: marker sampling inputs: corner_map_entries={}, complete_marker_cells={}",
            corner_map.len(),
            cells.len()
        );

        let ctx = ComponentContext {
            #[cfg(feature = "diagnostics")]
            index: component_index,
            #[cfg(feature = "diagnostics")]
            chess_corner_count: chessboard.corners.len(),
            #[cfg(feature = "diagnostics")]
            candidate_cell_count: cells.len(),
        };
        let scan_cfg = self.params.scan.clone();

        let board_cfg = BoardMatchConfig {
            px_per_square: self.params.px_per_square,
            bit_likelihood_slope: self.params.advanced.bit_likelihood_slope,
            per_bit_floor: self.params.advanced.per_bit_floor,
            alignment_min_margin: self.params.advanced.alignment_min_margin,
            cell_weight_border_threshold: self.params.advanced.cell_weight_border_threshold,
        };
        let matched = sink.run_match(image, &cells, &self.board, &scan_cfg, &board_cfg);

        let ComponentMatch {
            markers,
            alignment,
            raw_counts,
        } = match matched {
            Some((markers, alignment)) => {
                let count = markers.len();
                ComponentMatch {
                    markers,
                    alignment,
                    raw_counts: RawMarkerCounts {
                        raw_marker_count: count,
                        raw_marker_wrong_id_count: 0,
                    },
                }
            }
            None => {
                warn!(
                    "board-level matcher rejected: no hypothesis cleared the margin gate ({} candidate cells)",
                    cells.len()
                );
                let err = CharucoDetectError::AlignmentFailed { inliers: 0 };
                sink.component_failed(ctx, &err);
                return Err(err);
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
            sink.component_failed(ctx, &err);
            return Err(err);
        }

        let detection = map_charuco_corners(&self.board, &chessboard, &alignment);
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

        sink.component_ok(ctx, markers.len(), detection.corners.len(), raw_counts);

        Ok((
            CharucoDetectionResult::from_target_detection(detection, markers, alignment.alignment),
            raw_counts,
        ))
    }
}

/// Diagnostics-collecting pipeline sink. Runs [`match_board_diag`], captures
/// the per-cell board diagnostics for the component currently being matched,
/// and assembles the full [`CharucoDetectDiagnostics`].
#[cfg(feature = "diagnostics")]
#[derive(Default)]
struct DiagCollector {
    diagnostics: CharucoDetectDiagnostics,
    /// Board-level diagnostics from the most recent [`PipelineSink::run_match`],
    /// consumed by the next `component_ok` / `component_failed`.
    pending_board: Option<BoardMatchDiagnostics>,
}

#[cfg(feature = "diagnostics")]
impl PipelineSink for DiagCollector {
    fn run_match(
        &mut self,
        image: &GrayImageView<'_>,
        cells: &[calib_targets_aruco::MarkerCell],
        board: &CharucoBoard,
        scan_cfg: &calib_targets_aruco::ScanDecodeConfig,
        cfg: &BoardMatchConfig,
    ) -> Option<(Vec<MarkerDetection>, CharucoAlignment)> {
        let (result, board_diag) = match_board_diag(image, cells, board, scan_cfg, cfg);
        self.pending_board = Some(board_diag);
        result
    }

    fn component_failed(&mut self, ctx: ComponentContext, reason: &CharucoDetectError) {
        let board = self.pending_board.take();
        self.diagnostics.components.push(ComponentDiagnostics {
            index: ctx.index,
            chess_corner_count: ctx.chess_corner_count,
            candidate_cell_count: ctx.candidate_cell_count,
            board,
            outcome: ComponentOutcome::Failed {
                reason: reason.to_string(),
            },
        });
    }

    fn component_ok(
        &mut self,
        ctx: ComponentContext,
        markers: usize,
        charuco_corners: usize,
        raw_counts: RawMarkerCounts,
    ) {
        let board = self.pending_board.take();
        self.diagnostics.components.push(ComponentDiagnostics {
            index: ctx.index,
            chess_corner_count: ctx.chess_corner_count,
            candidate_cell_count: ctx.candidate_cell_count,
            board,
            outcome: ComponentOutcome::Ok {
                markers,
                charuco_corners,
                raw_marker_count: raw_counts.raw_marker_count,
                raw_marker_wrong_id_count: raw_counts.raw_marker_wrong_id_count,
            },
        });
    }

    fn record_frame_counts(&mut self, raw_counts: RawMarkerCounts) {
        self.diagnostics.raw_marker_count = raw_counts.raw_marker_count;
        self.diagnostics.raw_marker_wrong_id_count = raw_counts.raw_marker_wrong_id_count;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{CharucoBoardSpec, MarkerLayout};
    use calib_targets_aruco::builtins;

    fn test_board() -> CharucoBoardSpec {
        CharucoBoardSpec::new(5, 7, 20.0, 0.75, builtins::DICT_4X4_50)
            .with_marker_layout(MarkerLayout::OpenCvCharuco)
    }

    /// `for_board` produces a valid detector config.
    #[test]
    fn for_board_builds_detector() {
        let params = CharucoParams::for_board(&test_board());
        CharucoDetector::new(params).expect("detector must build from for_board params");
    }

    /// Every `sweep_for_board` config produces a valid detector.
    #[test]
    fn sweep_for_board_builds_detectors() {
        for params in CharucoParams::sweep_for_board(&test_board()) {
            CharucoDetector::new(params).expect("detector must build from sweep_for_board params");
        }
    }

    /// The default detector config reaches the chessboard stage and reports
    /// `ChessboardNotDetected` on an empty image (no algorithm guard rejects
    /// it up front).
    #[test]
    fn detector_reaches_chessboard_stage() {
        let params = CharucoParams::for_board(&test_board());
        let detector = CharucoDetector::new(params).expect("detector");
        let buf = [0u8; 16];
        let image = GrayImageView {
            width: 4,
            height: 4,
            data: &buf,
        };
        let result = detector.detect(&image, &[]);
        assert!(
            matches!(result, Err(CharucoDetectError::ChessboardNotDetected)),
            "detector must reach the chessboard stage, got {result:?}"
        );
    }
}
