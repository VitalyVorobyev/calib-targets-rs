//! Detector output types.

use calib_targets_core::{GridAlignment, TargetDetection};
use serde::Serialize;

/// Compact decode quality summary.
///
/// This is the part of the decode a consumer needs to *use* a PuzzleBoard
/// detection: how much support the decode had and where local `(0, 0)`
/// landed on the master board. Winner-vs-runner-up scoring evidence and the
/// raw per-edge observations live in
/// [`crate::diagnostics::PuzzleBoardDiagnostics`], obtained via
/// [`crate::PuzzleBoardDetector::detect_with_diagnostics`].
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
}

/// Full result of a PuzzleBoard detection call.
///
/// `#[non_exhaustive]`: construct with [`PuzzleBoardDetectionResult::new`].
#[non_exhaustive]
#[derive(Clone, Debug, Serialize)]
pub struct PuzzleBoardDetectionResult {
    /// Labelled corners — `LabeledCorner::id` is set from master coordinates.
    pub detection: TargetDetection,
    /// Alignment from the detected local grid into master-board coordinates.
    pub alignment: GridAlignment,
    /// Compact decode quality summary.
    pub decode: PuzzleBoardDecodeInfo,
}

impl PuzzleBoardDetectionResult {
    /// Create a result from its detection, alignment, and decode summary.
    pub fn new(
        detection: TargetDetection,
        alignment: GridAlignment,
        decode: PuzzleBoardDecodeInfo,
    ) -> Self {
        Self {
            detection,
            alignment,
            decode,
        }
    }
}
