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
    /// Two or more board frames explain the detected circles equally well.
    ///
    /// The three marker circles exist to break the board's 4-fold rotational
    /// symmetry; when a second frame explains them just as well they have
    /// failed at that, and returning either one would put a wrong `(i, j)`
    /// label on every corner. Reported as a failure rather than silently
    /// resolved, because a rotated frame is unrecoverable downstream.
    #[error("circle-marker alignment is ambiguous ({inliers} circles agree, and so do {runner_up} under another frame)")]
    AlignmentAmbiguous {
        /// Expected circles the best frame explained.
        inliers: usize,
        /// Expected circles the best competing frame explained.
        runner_up: usize,
    },
}
