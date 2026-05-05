//! ChESS detector configuration types.
//!
//! In 0.8 these were thin wrappers over `chess-corners`. The wrappers are
//! gone — every type below is a direct re-export of the upstream type, so
//! workspace-level deltas can no longer drift away from upstream defaults
//! and `chess-corners` field additions cannot be silently dropped by a
//! manual conversion routine.
//!
//! Workspace-only preprocessing (the optional same-size Gaussian pre-blur
//! that used to live on `ChessConfig::pre_blur_sigma_px`) is now a sibling
//! field on each detector's `*Params` struct, applied by the corresponding
//! detector entry point before the upstream `find_chess_corners_image`
//! call.

/// Low-level per-image ChESS params (single scale).
pub use chess_corners::ChessParams as ChessCornerParams;
/// Upstream `UpscaleError` re-exported under the workspace legacy name.
pub use chess_corners::UpscaleError as UpscaleConfigError;
pub use chess_corners::{
    CenterOfMassConfig, ChessConfig, CoarseToFineParams, DescriptorMode, DetectorMode,
    ForstnerConfig, PyramidParams, RadonDetectorParams, RefinementMethod, RefinerConfig,
    RefinerKind, SaddlePointConfig, ThresholdMode, UpscaleConfig, UpscaleMode,
};
