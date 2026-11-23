//! Typed failure modes of the marker-board detector.

use calib_targets_chessboard::ChessboardParamsError;

/// Invalid configuration supplied to [`crate::MarkerBoardDetector::new`].
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum MarkerBoardParamsError {
    /// The embedded chessboard detector parameters were invalid.
    #[error(transparent)]
    Chessboard(#[from] ChessboardParamsError),
    /// Board dimensions must have at least 2×2 inner corners.
    #[error("invalid marker board dimensions {rows}×{cols}: both must be >= 2")]
    InvalidBoardDimensions {
        /// Configured inner-corner row count.
        rows: u32,
        /// Configured inner-corner column count.
        cols: u32,
    },
    /// Optional square size must be finite and strictly positive when provided.
    #[error("invalid marker board cell_size: must be finite and > 0")]
    InvalidCellSize,
    /// Circle diameter relative to square side must lie in `(0, 1]`.
    #[error("invalid circle_diameter_rel: must be finite and in (0, 1]")]
    InvalidCircleDiameter,
    /// One of the expected circle markers lies outside the `0..=cols × 0..=rows` square grid.
    #[error("circle marker at ({i}, {j}) lies outside {cols}×{rows} inner-corner board squares (0..={cols} × 0..={rows})")]
    CircleOutOfBounds {
        /// Circle column index.
        i: i32,
        /// Circle row index.
        j: i32,
        /// Board inner-corner column count.
        cols: u32,
        /// Board inner-corner row count.
        rows: u32,
    },
    /// Two expected circle markers occupy the same board square.
    #[error("duplicate circle marker cell coordinates in board spec")]
    DuplicateCircleCells,
    /// One or more [`crate::CircleScoreParams`] fields are invalid or non-finite.
    #[error("invalid circle_score parameters")]
    InvalidCircleScoreParams,
}

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

impl MarkerBoardDetectError {
    /// Whether this is a miss — no marker board in the image can be placed in
    /// its frame — rather than a misuse. Every variant today is a miss.
    pub fn is_not_found(&self) -> bool {
        match self {
            Self::ChessboardNotDetected
            | Self::AlignmentFailed { .. }
            | Self::AlignmentAmbiguous { .. } => true,
        }
    }
}
