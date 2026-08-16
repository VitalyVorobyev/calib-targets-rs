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
    /// Too few *distinct* master bits survived the period-3 vote.
    ///
    /// [`WindowTooThin`](Self::WindowTooThin) guarantees the fragment's
    /// geometry *offers* enough independent bits; this guarantees enough of
    /// them were actually resolved. The two differ whenever dots disagree: a
    /// class whose members split evenly is an erasure, not a bit, so a noisy
    /// fragment can span the required corners and still determine far less than
    /// the code needs.
    ///
    /// Without this guard the uniqueness predicate silently changes meaning.
    /// `margin > k_winner` proves the winner is the only codeword within its
    /// error radius — but over a handful of surviving bits *many* master
    /// positions are within that radius, so the proof holds while saying
    /// nothing. Measured: erasures alone, ungated, produced wrong-origin
    /// decodes at high dot corruption
    /// (`decode::tests::consensus_noise_tolerance_report`).
    #[error("not enough distinct code bits resolved (got {determined}, need {needed})")]
    NotEnoughLogicalBits {
        /// Distinct master bits the fragment resolved after voting.
        determined: usize,
        /// Minimum the configured `min_window` is required to yield.
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
