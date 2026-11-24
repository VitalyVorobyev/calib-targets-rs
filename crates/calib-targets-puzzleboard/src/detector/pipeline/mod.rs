//! End-to-end PuzzleBoard detection pipeline.

use std::cmp::Ordering;

use calib_targets_chessboard::{detect_corner_frame, ChessCorner};
use calib_targets_chessboard::{
    ChessboardCorner, ChessboardDetection, ChessboardDetector as ChessDetector,
};
use calib_targets_core::{Coord, GrayImageView, LabeledCorner, TargetDetection, TargetKind};
use nalgebra::Point2;

use crate::board::{PuzzleBoardSpec, PuzzleBoardSpecError, MASTER_COLS, MASTER_ROWS};
use crate::code_maps::{EdgeOrientation, PuzzleBoardObservedEdge};
use crate::detector::consensus::EdgeConsensus;
use crate::detector::decode::{
    decode as run_decode, decode_fixed_board, decode_fixed_board_soft, decode_soft, match_stats,
    BoardRect, SoftLlConfig,
};
use crate::detector::edge_sampling::sample_all_edges;
use crate::detector::error::PuzzleBoardDetectError;
use crate::detector::params::{
    ensure_min_edges, required_edges, required_logical_bits, PuzzleBoardDecodeConfig,
    PuzzleBoardScoringMode, PuzzleBoardSearchMode,
};
use crate::detector::result::{PuzzleBoardDecodeInfo, PuzzleBoardDetection};
#[cfg(feature = "diagnostics")]
use crate::diagnostics::PuzzleBoardDiagnostics;
use crate::params::PuzzleBoardParams;
#[cfg(feature = "tracing")]
use tracing::instrument;

mod diag;

use diag::ComponentRank;
#[cfg(feature = "diagnostics")]
pub(crate) use diag::PuzzleDiagCollector;
pub(crate) use diag::{DecodeDiagSummary, NoPuzzleDiag, PuzzleDiagSink};

/// One component's decode: the slim public result paired with its compact
/// [`ComponentRank`] for best-component selection.
struct ComponentDecode {
    result: PuzzleBoardDetection,
    rank: ComponentRank,
}

/// Owned PuzzleBoard detector.
pub struct PuzzleBoardDetector {
    params: PuzzleBoardParams,
    chessboard: ChessDetector,
}

impl PuzzleBoardDetector {
    /// Create a new detector from the given parameters.
    ///
    /// Validates the board geometry and returns
    /// [`PuzzleBoardSpecError`] if `params.board` is invalid (e.g. rows or
    /// cols are zero, or the master origin is out of range).
    pub fn new(params: PuzzleBoardParams) -> Result<Self, PuzzleBoardSpecError> {
        let _ = PuzzleBoardSpec::with_origin(
            params.board.rows,
            params.board.cols,
            params.board.cell_size,
            params.board.origin_row,
            params.board.origin_col,
        )?;
        let chessboard = ChessDetector::new(params.chessboard.clone())?;
        Ok(Self { params, chessboard })
    }

    /// Return a reference to the detector parameters.
    pub fn params(&self) -> &PuzzleBoardParams {
        &self.params
    }

    /// The declared board as the fixed-board decoders consume it.
    fn board_rect(&self) -> BoardRect {
        BoardRect::new(
            self.params.board.origin_row,
            self.params.board.origin_col,
            self.params.board.rows,
            self.params.board.cols,
        )
    }

    /// Run the ChESS corner front-end configured by
    /// [`PuzzleBoardParams::chess`](crate::PuzzleBoardParams::chess) over
    /// `image`.
    ///
    /// This is the corner pass [`Self::detect`] runs internally, exposed so a
    /// caller that wants to reuse one corner cloud across several detectors
    /// (or inspect it) can run it once and feed
    /// [`Self::detect_with_corners`].
    pub fn detect_corners(&self, image: &GrayImageView<'_>) -> Vec<ChessCorner> {
        calib_targets_chessboard::detect_corners(image, &self.params.chess)
    }

    /// Detect a PuzzleBoard in `image`, running the corner front-end
    /// configured by
    /// [`PuzzleBoardParams::chess`](crate::PuzzleBoardParams::chess).
    ///
    /// This is the ergonomic entry point: hand it an image and it does the
    /// whole pipeline. It gives the result of
    /// `self.detect_with_corners(image, &self.detect_corners(image))` — reach
    /// for [`Self::detect_with_corners`] when you already have a corner cloud
    /// (a custom upstream, or one shared across detectors).
    ///
    /// The error and tie-breaking semantics are those of
    /// [`Self::detect_with_corners`]. Running the corner pass itself, it also
    /// knows the frame's noise level and records it on the result's
    /// [`quality`](PuzzleBoardDetection::quality).
    pub fn detect(
        &self,
        image: &GrayImageView<'_>,
    ) -> Result<PuzzleBoardDetection, PuzzleBoardDetectError> {
        let frame = detect_corner_frame(image, &self.params.chess);
        let mut result = self.detect_with_corners(image, &frame.corners)?;
        result.quality = result.quality.with_noise_sigma(frame.noise_sigma);
        Ok(result)
    }

    /// Detect a PuzzleBoard in `image` using raw ChESS corner features.
    ///
    /// # Arguments
    ///
    /// - `image` — greyscale image view; **not** processed to extract corners.
    /// - `corners` — raw ChESS corner detections (subpixel position + strength);
    ///   typically obtained from [`Self::detect_corners`] or
    ///   [`calib_targets_chessboard::detect_corners`]. The detector will
    ///   internally refine them into a chessboard grid.
    ///
    /// # Errors
    ///
    /// - [`PuzzleBoardDetectError::ChessboardNotDetected`] — the chessboard
    ///   stage found no usable grid components in `corners`.
    /// - [`PuzzleBoardDetectError::NotEnoughEdges`] — fewer than
    ///   `params.decode.min_window²` interior edges were observable.
    /// - [`PuzzleBoardDetectError::DecodeFailed`] — no master-pattern origin
    ///   produced a bit-error-rate below `params.decode.max_bit_error_rate`.
    /// - [`PuzzleBoardDetectError::InconsistentPosition`] — two independent
    ///   grid components decoded to different master origins (only possible
    ///   when `params.decode.search_all_components` is `true`).
    ///
    /// # Tie-breaking
    ///
    /// When `params.decode.search_all_components` is `true`, all chessboard
    /// components are decoded and the best-supported component is returned.
    /// Ranking stays support-first in both scoring modes:
    /// - higher `edges_matched` wins
    /// - lower `bit_error_rate` breaks ties
    /// - soft mode then prefers higher `score_margin` / normalized soft score
    /// - hard mode then prefers higher `mean_confidence`
    ///
    /// If two successful decodes disagree on the master origin,
    /// [`PuzzleBoardDetectError::InconsistentPosition`] is returned instead.
    pub fn detect_with_corners(
        &self,
        image: &GrayImageView<'_>,
        corners: &[ChessCorner],
    ) -> Result<PuzzleBoardDetection, PuzzleBoardDetectError> {
        self.detect_inner(image, corners, &mut NoPuzzleDiag)
    }

    /// [`Self::detect`] + per-call diagnostics.
    ///
    /// Runs the corner front-end configured by
    /// [`PuzzleBoardParams::chess`](crate::PuzzleBoardParams::chess) and then
    /// [`Self::diagnose_with_corners`].
    ///
    /// Available only with the `diagnostics` feature enabled.
    #[cfg(feature = "diagnostics")]
    pub fn diagnose(
        &self,
        image: &GrayImageView<'_>,
    ) -> (
        Result<PuzzleBoardDetection, PuzzleBoardDetectError>,
        PuzzleBoardDiagnostics,
    ) {
        let frame = detect_corner_frame(image, &self.params.chess);
        let (mut result, diagnostics) = self.diagnose_with_corners(image, &frame.corners);
        if let Ok(result) = &mut result {
            result.quality = result.quality.with_noise_sigma(frame.noise_sigma);
        }
        (result, diagnostics)
    }

    /// Detect a PuzzleBoard and additionally return per-call diagnostics
    /// (the raw pre-alignment edge observations and the winner-vs-runner-up
    /// scoring evidence for the chosen component).
    ///
    /// Diagnostics are returned even when detection fails — best-effort, so
    /// overlay tools can render the edge observations that *were* sampled.
    /// See [`crate::diagnostics::PuzzleBoardDiagnostics`] for the shape and
    /// stability promise. The success/error semantics of the
    /// [`Result`] component match [`Self::detect_with_corners`] exactly.
    ///
    /// Available only with the `diagnostics` feature enabled.
    #[cfg(feature = "diagnostics")]
    pub fn diagnose_with_corners(
        &self,
        image: &GrayImageView<'_>,
        corners: &[ChessCorner],
    ) -> (
        Result<PuzzleBoardDetection, PuzzleBoardDetectError>,
        PuzzleBoardDiagnostics,
    ) {
        let mut collector = PuzzleDiagCollector::default();
        let result = self.detect_inner(image, corners, &mut collector);
        (result, collector.into_diagnostics())
    }

    #[cfg_attr(feature = "tracing", instrument(level = "info", skip_all))]
    fn detect_inner<S: PuzzleDiagSink>(
        &self,
        image: &GrayImageView<'_>,
        corners: &[ChessCorner],
        sink: &mut S,
    ) -> Result<PuzzleBoardDetection, PuzzleBoardDetectError> {
        let chess_results = {
            #[cfg(feature = "tracing")]
            let _span = tracing::info_span!("chess_detect_all").entered();
            self.chessboard.detect_all(corners)
        };
        if chess_results.is_empty() {
            return Err(PuzzleBoardDetectError::ChessboardNotDetected);
        }

        let mut last_err: Option<PuzzleBoardDetectError> = None;
        let mut best: Option<ComponentDecode> = None;
        let min_edges = required_edges(self.params.decode.min_window);

        for chess in &chess_results {
            match self.decode_component(image, chess, sink) {
                Ok((decoded, observed, diag_summary)) => {
                    // When searching all components, check for a master-origin
                    // conflict: two well-supported decodes that disagree on
                    // the absolute position (cyclic modulo 501×501).
                    if self.params.decode.search_all_components {
                        if let Some(ref prev) = best {
                            let both_well_supported = prev.result.decode.edges_matched >= min_edges
                                && decoded.result.decode.edges_matched >= min_edges;
                            if both_well_supported
                                && components_conflict(&prev.result, &decoded.result)
                            {
                                sink.record_conflict_component(&observed, diag_summary);
                                return Err(PuzzleBoardDetectError::InconsistentPosition);
                            }
                        }
                    }

                    let better = best.as_ref().is_none_or(|b| {
                        is_better_component_decode(
                            self.params.decode.scoring_mode,
                            decoded.rank,
                            b.rank,
                        )
                    });
                    sink.record_decoded_component(better, &observed, diag_summary);
                    if better {
                        best = Some(decoded);
                    }
                    if !self.params.decode.search_all_components {
                        break;
                    }
                }
                Err(e) => {
                    last_err = Some(e);
                }
            }
        }

        match best {
            Some(ComponentDecode { result, .. }) => Ok(result),
            None => Err(last_err.unwrap_or(PuzzleBoardDetectError::DecodeFailed)),
        }
    }

    #[cfg_attr(feature = "tracing", instrument(level = "info", skip_all))]
    fn decode_component<S: PuzzleDiagSink>(
        &self,
        image: &GrayImageView<'_>,
        chess: &ChessboardDetection,
        sink: &mut S,
    ) -> Result<
        (
            ComponentDecode,
            Vec<PuzzleBoardObservedEdge>,
            DecodeDiagSummary,
        ),
        PuzzleBoardDetectError,
    > {
        // Adapt the typed chessboard result into the generic
        // `LabeledCorner` slice the edge-sampling stage expects. The
        // detector emits only validated corners — every entry is an
        // inlier by construction; the original inliers index list
        // (subset of v1's pre-quality-filtered corners) is no longer
        // meaningful, so every labelled corner is treated as an inlier.
        let labeled: Vec<LabeledCorner> = chess
            .corners
            .iter()
            .map(ChessboardCorner::to_labeled)
            .collect();
        let labeled: &[LabeledCorner] = &labeled;
        let inliers: Vec<usize> = (0..labeled.len()).collect();
        let inliers: &[usize] = &inliers;

        let observed = self.sample_all_edges(image, labeled, inliers);
        let min_edges = required_edges(self.params.decode.min_window);
        if let Err(e) = ensure_min_edges(observed.len(), min_edges) {
            sink.record_failed_edges(&observed);
            return Err(e);
        }

        let filtered: Vec<PuzzleBoardObservedEdge> = observed
            .iter()
            .copied()
            .filter(|e| e.confidence >= self.params.decode.min_bit_confidence)
            .collect();
        if let Err(e) = ensure_min_edges(filtered.len(), min_edges) {
            sink.record_failed_edges(&observed);
            return Err(e);
        }

        // Limiting-dimension (bounded-distance) guard, applied to the
        // *post-confidence-filter* observation set — the window the decoder
        // actually decodes. The edge-count floor assumes a roughly-square window;
        // a wide-but-short strip can meet it yet still alias because its thin axis
        // carries too little code distance (measured: a 3-corner-tall strip meeting
        // the edge-count floor false-accepts at a low rate, while every window spanning
        // ≥ `min_window` corners on both axes is empirically alias-free). A
        // component that is wide overall can also collapse to a thin strip *after*
        // the `min_bit_confidence` filter drops its low-confidence edges, so the
        // span must be measured on `filtered`, not on the full labelled set.
        // Require both corner spans ≥ `min_window`.
        if let Some((span_i, span_j)) = observed_corner_span(&filtered) {
            let need = self.params.decode.min_window;
            if span_i < need || span_j < need {
                sink.record_failed_edges(&observed);
                return Err(PuzzleBoardDetectError::WindowTooThin {
                    span_i,
                    span_j,
                    needed: need,
                });
            }
        }

        // Period-3 consensus. Both code maps repeat every three rows or
        // columns, so the fragment's `~2w²` dots read only `~6w` distinct
        // master bits. Reducing to those bits needs no origin and no
        // orientation — the partition is invariant to both (see
        // `super::consensus`) — so it runs here, before any hypothesis exists.
        //
        // With the stage off, every dot stands for its own bit and the whole
        // pipeline behaves as it did before the stage existed.
        let consensus = EdgeConsensus::build(&filtered);
        let consensus_on = self.params.decode.effective_tuning().edge_consensus;
        let logical: Vec<PuzzleBoardObservedEdge> = if consensus_on {
            consensus.class_observations()
        } else {
            filtered.clone()
        };

        // Information floor. The span gate above guarantees the fragment's
        // geometry *offers* `6(min_window - 2)` distinct bits; this guarantees
        // that many were actually resolved. Voting erases a class whose members
        // split evenly rather than guessing it, so a noisy fragment can span the
        // required corners and still determine far too little code.
        //
        // Skipping this is not merely lossy, it is unsound: `margin > k_winner`
        // proves the winner is the only codeword inside its error radius, but
        // over a handful of surviving bits a great many master positions sit
        // inside that radius, so the proof stays true and stops meaning
        // anything. Measured — erasure without this guard produced wrong-origin
        // decodes (`decode::tests::consensus_noise_tolerance_report`).
        if consensus_on {
            let needed = required_logical_bits(self.params.decode.min_window);
            if logical.len() < needed {
                sink.record_failed_edges(&observed);
                return Err(PuzzleBoardDetectError::NotEnoughLogicalBits {
                    determined: logical.len(),
                    needed,
                });
            }
        }

        let max_err = self.params.decode.max_bit_error_rate;
        let soft_cfg = soft_cfg_from(&self.params.decode);
        let transforms = self.params.decode.symmetry_mode.transforms();
        #[cfg(feature = "tracing")]
        let decode_span = tracing::info_span!("decode_edges").entered();
        // The hard scorer decodes the voted bits: it counts matches, having
        // already discarded the per-dot confidence distribution, so voting
        // first is strictly better input. The soft scorer keeps the raw dots —
        // class members predict the same bit under every hypothesis, so summing
        // their per-dot log-likelihoods already *is* the optimal way to combine
        // the replicas, and voting first would decide too early. Both gate on
        // the logical bits.
        let decoded = match (
            self.params.decode.search_mode,
            self.params.decode.scoring_mode,
        ) {
            (PuzzleBoardSearchMode::Full, PuzzleBoardScoringMode::HardWeighted) => {
                run_decode(&logical, &filtered, transforms, max_err)
            }
            (PuzzleBoardSearchMode::Full, PuzzleBoardScoringMode::SoftLogLikelihood) => {
                decode_soft(&filtered, &logical, transforms, &soft_cfg, max_err)
            }
            (PuzzleBoardSearchMode::FixedBoard, PuzzleBoardScoringMode::HardWeighted) => {
                decode_fixed_board(&logical, &filtered, self.board_rect(), transforms, max_err)
            }
            (PuzzleBoardSearchMode::FixedBoard, PuzzleBoardScoringMode::SoftLogLikelihood) => {
                decode_fixed_board_soft(
                    &filtered,
                    &logical,
                    self.board_rect(),
                    transforms,
                    &soft_cfg,
                    max_err,
                )
            }
        };
        #[cfg(feature = "tracing")]
        drop(decode_span);
        let Some(decoded) = decoded else {
            sink.record_failed_edges(&observed);
            return Err(PuzzleBoardDetectError::DecodeFailed);
        };

        let mut out_corners: Vec<LabeledCorner> = Vec::with_capacity(labeled.len());
        for (idx, lc) in labeled.iter().enumerate() {
            if !inliers.contains(&idx) {
                continue;
            }
            let Some(grid) = lc.grid else {
                continue;
            };
            let raw = decoded.alignment.apply(grid);
            let (raw_i, raw_j) = (raw.u, raw.v);
            // Invariant: master coords must be wrapped into [0, 501) so that
            // `target_position == Point2::new((id % 501) * cell, (id / 501) * cell)`
            // holds for every LabeledCorner regardless of which D4 transform was
            // selected. Without wrapping, the 4 D4 transforms with negative a/d
            // entries can produce negative coords that give consistent `id` (via
            // rem_euclid inside master_ij_to_id) but wrong `target_position`.
            let (master_i, master_j) = wrap_master(raw_i, raw_j);
            let id = master_ij_to_id(master_i, master_j);
            let target = master_target_position(master_i, master_j, self.params.board.cell_size);
            out_corners.push(
                LabeledCorner::new(lc.position, lc.score)
                    .with_position_covariance(lc.position_covariance)
                    .with_grid(Coord::new(master_i, master_j))
                    .with_id(id)
                    .with_target_position(target),
            );
        }

        let detection = TargetDetection::new(TargetKind::PuzzleBoard, out_corners);
        let scoring_mode = self.params.decode.scoring_mode;
        // Soft-only score fields are `None` under hard-weighted scoring.
        let (score_best, score_margin) = match scoring_mode {
            PuzzleBoardScoringMode::SoftLogLikelihood => {
                (Some(decoded.score_best), Some(decoded.score_margin))
            }
            PuzzleBoardScoringMode::HardWeighted => (None, None),
        };
        // The two scorers rank over different sets — the hard one over voted
        // bits, the soft one over raw dots — so neither `decoded.edges_*` pair
        // means the same thing across modes. Restate both views from the
        // decided alignment instead, so the reported numbers are defined
        // identically whatever produced the winner. Two O(N) passes.
        let physical = match_stats(&filtered, &decoded.alignment);
        let logical_stats = match_stats(&logical, &decoded.alignment);

        // The scorer's own bookkeeping must agree with an independent
        // recomputation over the set it was actually handed. This is what
        // catches the two sets being crossed — a hard decode reported against
        // the dots, or a soft one against the classes — which would otherwise
        // surface only as a quietly wrong `bit_error_rate`.
        debug_assert_eq!(
            decoded.edges_observed,
            match self.params.decode.scoring_mode {
                PuzzleBoardScoringMode::HardWeighted => logical_stats.observed,
                PuzzleBoardScoringMode::SoftLogLikelihood => physical.observed,
            },
            "decoder reported a different set size than it was given"
        );
        debug_assert!(
            {
                let own = match self.params.decode.scoring_mode {
                    PuzzleBoardScoringMode::HardWeighted => &logical_stats,
                    PuzzleBoardScoringMode::SoftLogLikelihood => &physical,
                };
                decoded.edges_matched == own.matched
                    && (decoded.bit_error_rate - own.bit_error_rate).abs() < 1e-5
                    && (decoded.mean_confidence - own.mean_confidence).abs() < 1e-5
            },
            "decoder bookkeeping disagrees with the restated match statistics"
        );
        let decode_info = PuzzleBoardDecodeInfo::new(
            physical.observed,
            physical.matched,
            physical.mean_confidence,
            physical.bit_error_rate,
            decoded.master_origin_row,
            decoded.master_origin_col,
        )
        .with_consensus(
            logical_stats.observed,
            logical_stats.bit_error_rate,
            consensus.dissent_rate(),
        );
        let rank = ComponentRank {
            edges_observed: decode_info.edges_observed,
            edges_matched: decode_info.edges_matched,
            bit_error_rate: decode_info.bit_error_rate,
            mean_confidence: decode_info.mean_confidence,
            score_best,
            score_margin,
        };
        let diag_summary =
            DecodeDiagSummary::from_decode(score_best, score_margin, &decoded, scoring_mode);

        Ok((
            ComponentDecode {
                result: PuzzleBoardDetection::from_target_detection(
                    detection,
                    decoded.alignment,
                    decode_info,
                ),
                rank,
            },
            observed,
            diag_summary,
        ))
    }

    /// Sample every interior edge of `corners`, at the radius this detector was
    /// configured with.
    ///
    /// The sampler itself lives in `edge_sampling` and takes no detector: this
    /// binds the one parameter it needs out of `self.params`.
    fn sample_all_edges(
        &self,
        image: &GrayImageView<'_>,
        corners: &[LabeledCorner],
        inliers: &[usize],
    ) -> Vec<PuzzleBoardObservedEdge> {
        sample_all_edges(
            image,
            corners,
            inliers,
            self.params.decode.sample_radius_rel,
        )
    }
}

/// Wrap raw master coordinates (which may be negative after a D4 transform with
/// negative a/d entries) into `[0, MASTER_COLS)` so that both `master_ij_to_id`
/// and `master_target_position` receive canonical non-negative inputs.
///
/// Call this **before** both functions to guarantee the invariant:
/// `target_position == Point2::new((id % 501) * cell, (id / 501) * cell)`.
pub(crate) fn wrap_master(master_i: i32, master_j: i32) -> (i32, i32) {
    (
        master_i.rem_euclid(MASTER_COLS as i32),
        master_j.rem_euclid(MASTER_ROWS as i32),
    )
}

/// Compute the flat corner id from already-wrapped master coordinates in `[0, 501)`.
pub(crate) fn master_ij_to_id(master_i: i32, master_j: i32) -> u32 {
    // Inputs are pre-wrapped by wrap_master; the rem_euclid is a defensive no-op.
    debug_assert!(master_i >= 0 && master_i < MASTER_COLS as i32);
    debug_assert!(master_j >= 0 && master_j < MASTER_COLS as i32);
    (master_j as u32) * MASTER_COLS + (master_i as u32)
}

/// Compute the physical board-frame position from already-wrapped master coordinates.
///
/// Invariant: `target_position.x == (id % 501) as f32 * cell_size`
///            `target_position.y == (id / 501) as f32 * cell_size`.
pub(crate) fn master_target_position(master_i: i32, master_j: i32, cell_size: f32) -> Point2<f32> {
    debug_assert!(master_i >= 0 && master_j >= 0);
    Point2::new(master_i as f32 * cell_size, master_j as f32 * cell_size)
}

fn median_grid_pitch_px(detections: &[&PuzzleBoardDetection]) -> f32 {
    let mut dists = Vec::new();
    for det in detections {
        let mut by_grid = std::collections::BTreeMap::new();
        for c in &det.corners {
            by_grid.insert((c.grid.u, c.grid.v), c.position);
        }
        for (&(u, v), &p) in &by_grid {
            for next in [(u + 1, v), (u, v + 1)] {
                if let Some(&q) = by_grid.get(&next) {
                    let d = (q - p).norm();
                    if d.is_finite() && d > 0.0 {
                        dists.push(d);
                    }
                }
            }
        }
    }
    if dists.is_empty() {
        return 0.0;
    }
    dists.sort_by(f32::total_cmp);
    dists[dists.len() / 2]
}

/// Return `true` if two decoded board components contradict each other:
/// either the same master corner `id` maps further than `max(2.0, 0.25 * est_pitch)` px
/// apart, or the same image pixel (within `1.0` px) is assigned different master IDs.
fn components_conflict(a: &PuzzleBoardDetection, b: &PuzzleBoardDetection) -> bool {
    let est_pitch = median_grid_pitch_px(&[a, b]);
    let pos_tol = 2.0f32.max(0.25 * est_pitch);

    let mut b_by_id = std::collections::BTreeMap::new();
    let mut b_by_x: Vec<(f32, f32, u32)> = Vec::with_capacity(b.corners.len());
    for cb in &b.corners {
        b_by_id.insert(cb.id, cb.position);
        if cb.position.x.is_finite() && cb.position.y.is_finite() {
            b_by_x.push((cb.position.x, cb.position.y, cb.id));
        }
    }
    b_by_x.sort_by(|lhs, rhs| {
        lhs.0
            .total_cmp(&rhs.0)
            .then_with(|| lhs.1.total_cmp(&rhs.1))
            .then_with(|| lhs.2.cmp(&rhs.2))
    });

    for ca in &a.corners {
        if let Some(&pb) = b_by_id.get(&ca.id) {
            let d = (ca.position - pb).norm();
            if !d.is_finite() || d > pos_tol {
                return true;
            }
        }
        if ca.position.x.is_finite() && ca.position.y.is_finite() {
            let start = b_by_x.partition_point(|&(bx, _, _)| bx < ca.position.x - 1.0);
            for &(bx, by, bid) in &b_by_x[start..] {
                if bx > ca.position.x + 1.0 {
                    break;
                }
                if bid != ca.id {
                    let dx = ca.position.x - bx;
                    let dy = ca.position.y - by;
                    if dx * dx + dy * dy <= 1.0 {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn cmp_higher(candidate: f32, current: f32) -> Option<bool> {
    match candidate.partial_cmp(&current) {
        Some(Ordering::Greater) => Some(true),
        Some(Ordering::Less) => Some(false),
        _ => None,
    }
}

fn cmp_lower(candidate: f32, current: f32) -> Option<bool> {
    match candidate.partial_cmp(&current) {
        Some(Ordering::Less) => Some(true),
        Some(Ordering::Greater) => Some(false),
        _ => None,
    }
}

/// Normalized soft score for one component: the winning hypothesis'
/// `score_best` divided by `edges_observed`.
fn normalized_soft_component_score(rank: ComponentRank) -> f32 {
    let edges = rank.edges_observed.max(1) as f32;
    rank.score_best.unwrap_or(f32::NEG_INFINITY) / edges
}

/// Rank two component decodes. Support-first in both scoring modes:
/// higher `edges_matched`, then lower `bit_error_rate`; soft mode then prefers
/// higher `score_margin` / normalized soft score, hard mode higher
/// `mean_confidence`.
fn is_better_component_decode(
    scoring_mode: PuzzleBoardScoringMode,
    candidate: ComponentRank,
    current: ComponentRank,
) -> bool {
    if candidate.edges_matched != current.edges_matched {
        return candidate.edges_matched > current.edges_matched;
    }
    if let Some(wins) = cmp_lower(candidate.bit_error_rate, current.bit_error_rate) {
        return wins;
    }
    match scoring_mode {
        PuzzleBoardScoringMode::SoftLogLikelihood => {
            if let Some(wins) = cmp_higher(
                candidate.score_margin.unwrap_or(f32::NEG_INFINITY),
                current.score_margin.unwrap_or(f32::NEG_INFINITY),
            ) {
                return wins;
            }
            if let Some(wins) = cmp_higher(
                normalized_soft_component_score(candidate),
                normalized_soft_component_score(current),
            ) {
                return wins;
            }
            cmp_higher(candidate.mean_confidence, current.mean_confidence).unwrap_or(false)
        }
        PuzzleBoardScoringMode::HardWeighted => {
            cmp_higher(candidate.mean_confidence, current.mean_confidence).unwrap_or(false)
        }
    }
}

/// Corner-grid span `(span_col, span_row)` actually covered by an observed-edge
/// set, where each span is `max − min + 1` over the corner intersections the
/// edges reference. An edge anchored at local `(col, row)` references the corner
/// `(col, row)` plus its far endpoint — `(col + 1, row)` for a horizontal edge,
/// `(col, row + 1)` for a vertical one. `None` for an empty set.
///
/// This must be computed on the set the decoder actually decodes (the
/// post-confidence-filter observations), not the full labelled corner set: a
/// component that is wide overall can collapse to a long thin high-confidence
/// strip after the `min_bit_confidence` filter, and that strip is the window
/// whose code distance the bounded-distance guard governs.
fn observed_corner_span(edges: &[PuzzleBoardObservedEdge]) -> Option<(u32, u32)> {
    let mut min_col = i32::MAX;
    let mut max_col = i32::MIN;
    let mut min_row = i32::MAX;
    let mut max_row = i32::MIN;
    for e in edges {
        let (far_col, far_row) = match e.orientation {
            EdgeOrientation::Horizontal => (e.col + 1, e.row),
            EdgeOrientation::Vertical => (e.col, e.row + 1),
        };
        min_col = min_col.min(e.col).min(far_col);
        max_col = max_col.max(e.col).max(far_col);
        min_row = min_row.min(e.row).min(far_row);
        max_row = max_row.max(e.row).max(far_row);
    }
    if edges.is_empty() {
        return None;
    }
    Some((
        (max_col - min_col + 1) as u32,
        (max_row - min_row + 1) as u32,
    ))
}

/// Extract the soft-LL knobs from a decode config into the decoder-level
/// [`SoftLlConfig`] structure.
fn soft_cfg_from(cfg: &PuzzleBoardDecodeConfig) -> SoftLlConfig {
    let tuning = cfg.effective_tuning();
    SoftLlConfig {
        kappa: tuning.bit_likelihood_slope,
        per_bit_floor: tuning.per_bit_floor,
        alignment_min_margin: tuning.alignment_min_margin,
    }
}

#[cfg(test)]
mod tests;
