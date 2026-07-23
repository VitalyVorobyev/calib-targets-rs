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
