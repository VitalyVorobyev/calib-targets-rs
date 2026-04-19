/// Errors returned by the ChArUco detector.
#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
pub enum CharucoDetectError {
    #[error("chessboard not detected")]
    ChessboardNotDetected,
    #[error("no markers decoded")]
    NoMarkers,
    #[error("marker-to-board alignment failed (inliers={inliers})")]
    AlignmentFailed { inliers: usize },
}
