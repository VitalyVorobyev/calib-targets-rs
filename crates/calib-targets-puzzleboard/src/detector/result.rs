//! Detector output types.

use calib_targets_core::{GridAlignment, GridTransform, TargetDetection};
use serde::Serialize;

use crate::code_maps::PuzzleBoardObservedEdge;
use crate::detector::params::PuzzleBoardScoringMode;

/// Per-decode diagnostics.
#[non_exhaustive]
#[derive(Clone, Debug, Serialize)]
pub struct PuzzleBoardDecodeInfo {
    /// Total number of observed edges that contributed to the decode.
    pub edges_observed: usize,
    /// Number of observed edges whose bit matched the master after alignment.
    pub edges_matched: usize,
    /// Mean confidence across contributing edges.
    pub mean_confidence: f32,
    /// Hamming error rate across *all* observed bits after alignment.
    pub bit_error_rate: f32,
    /// Absolute master-board origin of local `(0, 0)`.
    pub master_origin_row: i32,
    /// Absolute master-board origin of local `(0, 0)`.
    pub master_origin_col: i32,
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

/// Full result of a PuzzleBoard detection call.
#[derive(Clone, Debug, Serialize)]
pub struct PuzzleBoardDetectionResult {
    /// Labelled corners — `LabeledCorner::id` is set from master coordinates.
    pub detection: TargetDetection,
    /// Alignment from the detected local grid into master-board coordinates.
    pub alignment: GridAlignment,
    /// Decode diagnostics.
    pub decode: PuzzleBoardDecodeInfo,
    /// Raw per-edge observations (before alignment resolution).
    pub observed_edges: Vec<PuzzleBoardObservedEdge>,
}
