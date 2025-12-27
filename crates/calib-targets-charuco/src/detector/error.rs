use calib_targets_chessboard::MeshWarpError;

/// Errors returned by the ChArUco detector.
#[derive(thiserror::Error, Debug)]
pub enum CharucoDetectError {
    #[error("chessboard not detected")]
    ChessboardNotDetected,
    #[error(transparent)]
    MeshWarp(#[from] MeshWarpError),
    #[error("no markers decoded")]
    NoMarkers,
    #[error("marker-to-board alignment failed (inliers={inliers})")]
    AlignmentFailed { inliers: usize },
}
