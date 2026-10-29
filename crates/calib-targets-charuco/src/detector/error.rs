/// Errors returned by the ChArUco detector.
#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
pub enum CharucoDetectError {
    /// No chessboard grid could be recovered from the input corners.
    #[error("chessboard not detected")]
    ChessboardNotDetected,
    /// No ArUco markers could be decoded from the board cells.
    #[error("no markers decoded")]
    NoMarkers,
    /// The decoded markers could not be aligned to the board spec.
    #[error("marker-to-board alignment failed (inliers={inliers})")]
    AlignmentFailed {
        /// Number of markers that agreed with the best alignment found.
        inliers: usize,
    },
}

impl CharucoDetectError {
    /// Whether this is a miss — the image holds no ChArUco board this detector
    /// can identify — rather than a misuse. Every variant today is a miss; a
    /// bad board spec is reported by `CharucoDetector::new` instead.
    pub fn is_not_found(&self) -> bool {
        match self {
            Self::ChessboardNotDetected | Self::NoMarkers | Self::AlignmentFailed { .. } => true,
        }
    }
}
