//! Detector output types.

use calib_targets_core::{GridAlignment, TargetDetection};
use serde::Serialize;

use crate::code_maps::ObservedEdge;

/// Per-decode diagnostics.
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
    pub observed_edges: Vec<ObservedEdge>,
}
