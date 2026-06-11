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
    /// The chessboard `graph_build_algorithm` requested via
    /// [`CharucoParams::chessboard`](crate::CharucoParams::chessboard) is not
    /// supported by the ChArUco detector.
    ///
    /// Only [`GraphBuildAlgorithm::SeedAndGrow`] is supported: the topological
    /// builder's axis-driven cell test assumes uniform black/white tiles, which
    /// marker squares break — a marker carries embedded bit features whose ChESS
    /// axes do not align with the global board directions, so triangle-pair
    /// merging into chessboard cells fails on every marker-bearing cell (see
    /// Gaps 8 + 10 in `docs/algorithmic_gaps.md`). Making the topological cell
    /// test marker-safe is out of scope; use
    /// [`GraphBuildAlgorithm::SeedAndGrow`] (the default) instead.
    ///
    /// [`GraphBuildAlgorithm::SeedAndGrow`]: calib_targets_chessboard::GraphBuildAlgorithm::SeedAndGrow
    #[error(
        "ChArUco does not support GraphBuildAlgorithm::Topological; \
         marker-internal corners defeat the topological cell test — \
         use GraphBuildAlgorithm::SeedAndGrow (the default)"
    )]
    UnsupportedAlgorithm,
}
