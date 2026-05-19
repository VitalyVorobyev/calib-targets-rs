//! Detector output types.

use calib_targets_core::{GridAlignment, GridCoords, LabeledCorner, TargetDetection, TargetKind};
use nalgebra::Point2;
use serde::Serialize;

/// A decoded PuzzleBoard corner in master-board coordinates.
#[non_exhaustive]
#[derive(Clone, Debug, Serialize)]
pub struct PuzzleBoardCorner {
    /// Sub-pixel image position.
    pub position: Point2<f32>,
    /// Absolute master-board corner coordinate.
    pub grid: GridCoords,
    /// Absolute master-board corner ID.
    pub id: u32,
    /// Physical master-board position in millimetres.
    pub target_position: Point2<f32>,
    /// Detector-specific corner score; higher is better.
    pub score: f32,
}

impl PuzzleBoardCorner {
    /// Create a PuzzleBoard corner from its required fields.
    pub fn new(
        position: Point2<f32>,
        grid: GridCoords,
        id: u32,
        target_position: Point2<f32>,
        score: f32,
    ) -> Self {
        Self {
            position,
            grid,
            id,
            target_position,
            score,
        }
    }

    pub(crate) fn from_labeled(corner: LabeledCorner) -> Option<Self> {
        Some(Self {
            position: corner.position,
            grid: corner.grid?,
            id: corner.id?,
            target_position: corner.target_position?,
            score: corner.score,
        })
    }

    /// Convert this typed corner to the shared carrier used by diagnostics and bindings.
    pub fn to_labeled(&self) -> LabeledCorner {
        LabeledCorner::new(self.position, self.score)
            .with_grid(self.grid)
            .with_id(self.id)
            .with_target_position(self.target_position)
    }
}

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
    /// Labelled corners in absolute master-board coordinates.
    pub corners: Vec<PuzzleBoardCorner>,
    /// Alignment from the detected local grid into master-board coordinates.
    pub alignment: GridAlignment,
    /// Compact decode quality summary.
    pub decode: PuzzleBoardDecodeInfo,
}

impl PuzzleBoardDetectionResult {
    /// Create a result from its typed corners, alignment, and decode summary.
    pub fn new(
        corners: Vec<PuzzleBoardCorner>,
        alignment: GridAlignment,
        decode: PuzzleBoardDecodeInfo,
    ) -> Self {
        Self {
            corners,
            alignment,
            decode,
        }
    }

    pub(crate) fn from_target_detection(
        detection: TargetDetection,
        alignment: GridAlignment,
        decode: PuzzleBoardDecodeInfo,
    ) -> Self {
        debug_assert_eq!(detection.kind, TargetKind::PuzzleBoard);
        let input_len = detection.corners.len();
        let corners: Vec<PuzzleBoardCorner> = detection
            .corners
            .into_iter()
            .filter_map(PuzzleBoardCorner::from_labeled)
            .collect();
        debug_assert_eq!(corners.len(), input_len);
        Self::new(corners, alignment, decode)
    }

    /// Convert typed corners into the shared `TargetDetection` carrier.
    pub fn target_detection(&self) -> TargetDetection {
        TargetDetection::new(
            TargetKind::PuzzleBoard,
            self.corners
                .iter()
                .map(PuzzleBoardCorner::to_labeled)
                .collect(),
        )
    }
}
