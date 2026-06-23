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
    /// The labelled corner region is too thin along one axis to decode safely.
    ///
    /// Bounded-distance decoding requires the observed window to span at least
    /// `min_window` corners in *both* grid directions: a wide-but-short strip
    /// can meet the total edge-count floor yet still alias (its limiting
    /// dimension carries too little code distance). Rejecting it is a soundness
    /// guard, not a recall choice.
    #[error("decode window too thin (spans {span_i}×{span_j} corners, need {needed}×{needed})")]
    WindowTooThin {
        /// Corner span along the grid `i` axis (`max_i − min_i + 1`).
        span_i: u32,
        /// Corner span along the grid `j` axis (`max_j − min_j + 1`).
        span_j: u32,
        /// Minimum span required along each axis (`min_window`).
        needed: u32,
    },
    /// The edge-code decoder found no master position above the
    /// confidence threshold.
    #[error("decoding failed: no position match above confidence threshold")]
    DecodeFailed,
    /// The decoded position disagrees with another detected component.
    #[error("decoded position is inconsistent with other components")]
    InconsistentPosition,
}
