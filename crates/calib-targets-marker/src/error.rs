//! Typed failure modes of the marker-board detector.

/// Errors returned by the marker-board detector.
///
/// The detector reports "no board here" as a typed `Err` rather than a bare
/// `None`, matching the ChArUco and PuzzleBoard detectors: a miss is expected
/// and recoverable, and the variant says *which* stage came up empty so a
/// caller can tell "no grid at all" from "grid found, circles did not pin it
/// to the layout".
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum MarkerBoardDetectError {
    /// No chessboard grid could be recovered from the input corners.
    #[error("chessboard not detected")]
    ChessboardNotDetected,
    /// The detected circle markers could not be aligned to the board spec.
    #[error("circle-marker alignment failed (matched={matched}, candidates={candidates})")]
    AlignmentFailed {
        /// Expected circles that found a candidate match.
        matched: usize,
        /// Total scored circle candidates considered.
        candidates: usize,
    },
}
