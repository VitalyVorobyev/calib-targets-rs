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
    /// The chessboard parameters supplied via
    /// [`CharucoParams::chessboard`](crate::CharucoParams::chessboard) were
    /// rejected by the chessboard detector's own configuration validator
    /// (`calib_targets_chessboard::Detector::new`).
    ///
    /// ChArUco runs on the topological grid builder; the only configuration the
    /// chessboard validator currently rejects is an orientation-source /
    /// graph-builder mismatch (a combination ChArUco never sets itself but a
    /// caller could construct on the embedded `chessboard` field). The detector
    /// surfaces it here rather than panicking.
    #[error("chessboard configuration rejected by the chessboard detector validator")]
    UnsupportedAlgorithm,
}
