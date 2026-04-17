//! Detector output types.

use calib_targets_core::{GridAlignment, TargetDetection};
use serde::Serialize;

use crate::code_maps::PuzzleBoardObservedEdge;
use crate::detector::params::PuzzleBoardSearchMode;

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

impl PuzzleBoardDetectionResult {
    /// Derive a [`PuzzleBoardSearchMode::KnownOrigin`] from this result so
    /// subsequent decodes of the same physical board can skip the full 501²
    /// scan.
    ///
    /// Typical workflow:
    /// ```ignore
    /// let first = detector_full.detect(&view, &corners)?;
    /// let fast_mode = first.as_known_origin(2);
    /// let mut params = params_full.clone();
    /// params.decode.search_mode = fast_mode;
    /// let next = PuzzleBoardDetector::new(params)?.detect(&view, &corners)?;
    /// ```
    pub fn as_known_origin(&self, window_radius: u32) -> PuzzleBoardSearchMode {
        PuzzleBoardSearchMode::KnownOrigin {
            origin_row: self.decode.master_origin_row,
            origin_col: self.decode.master_origin_col,
            window_radius,
        }
    }
}
