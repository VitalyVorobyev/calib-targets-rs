//! Detector output types.

use calib_targets_core::{Coord, GridAlignment, LabeledCorner, TargetDetection, TargetKind};
use nalgebra::Point2;
use serde::{Deserialize, Serialize};

/// A decoded PuzzleBoard corner in master-board coordinates.
#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PuzzleBoardCorner {
    /// Sub-pixel image position.
    pub position: Point2<f32>,
    /// Absolute master-board corner coordinate.
    ///
    /// `u` runs along grid `i` (rightward), `v` along grid `j` (downward).
    pub grid: Coord,
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
        grid: Coord,
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
/// raw per-edge observations live in the opt-in diagnostics channel.
#[cfg_attr(
    feature = "diagnostics",
    doc = "See [`crate::diagnostics::PuzzleBoardDiagnostics`], obtained via",
    doc = "[`crate::PuzzleBoardDetector::diagnose_with_corners`]."
)]
#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PuzzleBoardDecodeInfo {
    /// Total number of observed edges that contributed to the decode.
    pub edges_observed: usize,
    /// Number of observed edges whose bit matched the master after alignment.
    pub edges_matched: usize,
    /// Mean confidence across contributing edges.
    pub mean_confidence: f32,
    /// Hamming error rate across *all* observed bits after alignment.
    pub bit_error_rate: f32,
    /// Number of **distinct** master bits the fragment reads.
    ///
    /// Both code maps are cyclic with period 3 on their short axis, so a dot
    /// repeats the dot three edges along its own edge's direction. A fragment
    /// spanning `R` edge-rows and `C` edge-columns therefore samples `~2w²`
    /// dots but carries only `3R + 3C` independent bits — this count. It is the
    /// fragment's true code length, and the one the accept/reject gates are
    /// evaluated against.
    ///
    /// `edges_observed / logical_bits` is the redundancy the board gave you.
    pub logical_bits: usize,
    /// Hamming error rate over [`logical_bits`](Self::logical_bits), after the
    /// period-3 replicas have been majority-voted.
    ///
    /// This is what `max_bit_error_rate` gates on. It sits below
    /// [`bit_error_rate`](Self::bit_error_rate) whenever voting repaired a
    /// minority of dots, which is the margin the repetition structure exists to
    /// provide.
    pub logical_bit_error_rate: f32,
    /// Fraction of confidence mass that lost its period-3 class vote.
    ///
    /// A **hypothesis-free** read-quality meter: it is computed before any
    /// origin or orientation is hypothesised, so it is meaningful even when the
    /// decode itself is wrong. Near zero on a clean board. A large value means
    /// either genuinely noisy dots or — more usefully — a *mislabelled grid*,
    /// which mixes dots that read different master bits into one class.
    ///
    /// Monotone in the raw dot error rate but not calibrated to it: the mapping
    /// depends on how many times each bit was read, and it saturates well below
    /// `1`. Read it as "clean" versus "not clean".
    pub dot_dissent_rate: f32,
    /// Absolute master-board origin of local `(0, 0)`.
    pub master_origin_row: i32,
    /// Absolute master-board origin of local `(0, 0)`.
    pub master_origin_col: i32,
}

/// Full result of a PuzzleBoard detection call.
///
/// `#[non_exhaustive]`: construct with [`PuzzleBoardDetection::new`].
#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PuzzleBoardDetection {
    /// Labelled corners in absolute master-board coordinates.
    pub corners: Vec<PuzzleBoardCorner>,
    /// Alignment from the detected local grid into master-board coordinates.
    pub alignment: GridAlignment,
    /// Compact decode quality summary.
    pub decode: PuzzleBoardDecodeInfo,
}

impl PuzzleBoardDetection {
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
