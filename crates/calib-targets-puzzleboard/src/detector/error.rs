use crate::board::PuzzleBoardSpecError;

/// Errors returned by the PuzzleBoard detector.
#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
pub enum PuzzleBoardDetectError {
    /// The board spec supplied to the detector was invalid.
    #[error(transparent)]
    BoardSpec(#[from] PuzzleBoardSpecError),
    /// No chessboard grid could be recovered from the input corners.
    #[error("chessboard not detected")]
    ChessboardNotDetected,
    /// Too few interior-edge dots were sampled to attempt a decode.
    #[error("not enough edges sampled (got {observed}, need {needed})")]
    NotEnoughEdges {
        /// Number of edge dots actually sampled.
        observed: usize,
        /// Minimum number of edge dots the decoder requires.
        needed: usize,
    },
    /// The edge-code decoder found no master position above the
    /// confidence threshold.
    #[error("decoding failed: no position match above confidence threshold")]
    DecodeFailed,
    /// The decoded position disagrees with another detected component.
    #[error("decoded position is inconsistent with other components")]
    InconsistentPosition,
}
