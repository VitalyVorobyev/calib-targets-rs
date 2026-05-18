//! Opt-in diagnostics surface for the PuzzleBoard detector.
//!
//! These types carry evidence about *how* a decode was reached — the raw
//! per-edge bit observations sampled before alignment, and the
//! winner-vs-runner-up scoring evidence the decoder used to pick a
//! hypothesis. They are produced by
//! [`crate::PuzzleBoardDetector::detect_with_diagnostics`] and are
//! intentionally kept separate from the result API
//! ([`crate::PuzzleBoardDetectionResult`], [`crate::PuzzleBoardDecodeInfo`]).
//!
//! A consumer that only needs to *use* a PuzzleBoard detection wants the
//! labelled corners, the alignment, and the compact
//! [`crate::PuzzleBoardDecodeInfo`] quality summary — never the contents of
//! this module. The fields here exist only to *understand* or debug a
//! decode.
//!
//! This module carries a **looser stability promise** than the result API:
//! diagnostic fields may be added or restructured in minor releases as the
//! detector's internal evidence model evolves.

use calib_targets_core::GridTransform;
use serde::Serialize;

use crate::detector::PuzzleBoardScoringMode;

pub use crate::code_maps::PuzzleBoardObservedEdge;

/// Winner-vs-runner-up scoring evidence for the chosen decode hypothesis.
///
/// Populated for the component whose decode produced the returned
/// [`crate::PuzzleBoardDetectionResult`]. The runner-up fields are only
/// meaningful under [`PuzzleBoardScoringMode::SoftLogLikelihood`]; under
/// [`PuzzleBoardScoringMode::HardWeighted`] the soft-only fields are `None`.
#[non_exhaustive]
#[derive(Clone, Debug, Default, Serialize)]
pub struct PuzzleBoardDecodeDiagnostics {
    /// Raw score of the winning hypothesis. Under
    /// [`PuzzleBoardScoringMode::SoftLogLikelihood`] this is the summed
    /// per-bit log-likelihood; under [`PuzzleBoardScoringMode::HardWeighted`]
    /// it mirrors the confidence-weighted ratio so callers can read a
    /// single "best score" field regardless of mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_best: Option<f32>,
    /// Score of the runner-up hypothesis (soft scoring only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_runner_up: Option<f32>,
    /// Per-observation gap between winner and runner-up (soft scoring only).
    /// Populated as `(score_best − score_runner_up) / edges_observed`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_margin: Option<f32>,
    /// Runner-up master-row origin (soft scoring only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner_up_origin_row: Option<i32>,
    /// Runner-up master-col origin (soft scoring only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner_up_origin_col: Option<i32>,
    /// Runner-up D4 transform (soft scoring only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner_up_transform: Option<GridTransform>,
    /// Scoring mode used for this decode (elided from JSON when unset).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scoring_mode: Option<PuzzleBoardScoringMode>,
}

/// Per-call diagnostics captured by
/// [`crate::PuzzleBoardDetector::detect_with_diagnostics`].
///
/// Returned alongside the detection result on **every** call, including
/// failed detections (best-effort) so overlay tools can render the
/// edge observations that *were* sampled even when no master origin
/// decoded. On a failed call the fields are whatever the pipeline
/// produced before failing — typically `observed_edges` is populated and
/// [`Self::decode`] is the [`Default`] value.
#[non_exhaustive]
#[derive(Clone, Debug, Default, Serialize)]
pub struct PuzzleBoardDiagnostics {
    /// Raw per-edge bit observations sampled from the chosen chessboard
    /// component, **before** alignment resolution. The decoder consumes a
    /// confidence-filtered subset of these; this is the unfiltered dump.
    pub observed_edges: Vec<PuzzleBoardObservedEdge>,
    /// Winner-vs-runner-up scoring evidence for the chosen decode.
    pub decode: PuzzleBoardDecodeDiagnostics,
}
