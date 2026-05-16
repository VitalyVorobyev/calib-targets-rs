//! ChESS detector configuration types.
//!
//! Re-exports the `chess-corners` 0.10 facade types unmodified so workspace
//! callers can refer to them through `calib_targets_core::*` without
//! depending on `chess-corners` directly. The 0.10 release split the
//! previous monolithic `ChessConfig` into a top-level [`DetectorConfig`]
//! (strategy + threshold + multiscale + upscale) and a narrower
//! strategy-specific [`ChessConfig`] / [`RadonConfig`] payload. The
//! workspace surfaces both: most code paths name [`DetectorConfig`] as
//! their high-level config, while the lower-level [`ChessConfig`] is only
//! reachable through [`DetectorConfig::with_chess`] callbacks or the
//! [`DetectionStrategy::Chess`] enum payload.
//!
//! The legacy `find_chess_corners_image` free function is gone — call
//! [`Detector::new(cfg)?.detect(&img)?`] instead.
//!
//! Workspace-only preprocessing (the optional same-size Gaussian pre-blur)
//! is exposed as a standalone helper at the facade level
//! (`calib_targets::preprocess`); detection entry points operate on the
//! image as supplied so the library no longer conflates preprocessing
//! with detection.

/// Low-level per-image ChESS params (single scale).
pub use chess_corners::ChessParams as ChessCornerParams;
/// Upstream `UpscaleError` re-exported under the workspace legacy name.
pub use chess_corners::UpscaleError as UpscaleConfigError;
pub use chess_corners::{
    CenterOfMassConfig, ChessConfig, ChessRefiner, ChessRing, CoarseToFineParams, DescriptorRing,
    DetectionStrategy, Detector, DetectorConfig, ForstnerConfig, MultiscaleConfig,
    OrientationMethod, PyramidParams, RadonConfig, RadonDetectorParams, RadonPeakConfig,
    RadonRefiner, RefinerKind, SaddlePointConfig, Threshold, UpscaleConfig,
};
