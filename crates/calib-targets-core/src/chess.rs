//! ChESS detector configuration types.
//!
//! In 0.8, types that were previously mirrored verbatim from `chess-corners`
//! are now direct re-exports. Only [`ChessConfig`] is kept as a
//! workspace-owned type, because its `Default` differs from upstream:
//! the workspace uses `Relative 0.2` whereas upstream uses `Absolute 0.0`.
//!
//! All other types — including [`DetectorMode`], [`RefinementMethod`],
//! [`RefinerConfig`], [`RefinerKind`], [`DescriptorMode`], [`ThresholdMode`],
//! [`CenterOfMassConfig`], [`ForstnerConfig`], [`SaddlePointConfig`],
//! [`UpscaleConfig`], [`UpscaleMode`], [`ChessCornerParams`],
//! [`PyramidParams`], [`CoarseToFineParams`], [`RadonDetectorParams`] —
//! are re-exported from `chess-corners` directly.
//!
//! ## Radon support
//!
//! [`DetectorMode::Radon`], [`RefinementMethod::RadonPeak`], and
//! [`ChessConfig::radon_detector`] are now exposed workspace-wide.
//! Use [`ChessConfig::radon()`] for a ready-made Radon preset.

// Re-export upstream types verbatim.
/// Low-level per-image ChESS params (single scale).
pub use chess_corners::ChessParams as ChessCornerParams;
/// Upstream `UpscaleError` re-exported under the workspace legacy name.
pub use chess_corners::UpscaleError as UpscaleConfigError;
pub use chess_corners::{
    CenterOfMassConfig, CoarseToFineParams, DescriptorMode, DetectorMode, ForstnerConfig,
    PyramidParams, RadonDetectorParams, RefinementMethod, RefinerConfig, RefinerKind,
    SaddlePointConfig, ThresholdMode, UpscaleConfig, UpscaleMode,
};

use serde::{Deserialize, Serialize};

/// Workspace-owned high-level ChESS detector configuration.
///
/// Use [`ChessConfig::to_chess_corners_config`] to convert to the upstream
/// `chess_corners::ChessConfig` for passing to `find_chess_corners_image`.
///
/// The workspace default applies an adaptive fraction-of-max threshold
/// (`Relative 0.2`) rather than the upstream paper-faithful `Absolute 0.0`.
/// Downstream detectors rely on this mild pre-filter to keep candidate counts
/// manageable; callers wanting the raw upstream behaviour set `Absolute 0.0`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChessConfig {
    pub detector_mode: DetectorMode,
    pub descriptor_mode: DescriptorMode,
    pub threshold_mode: ThresholdMode,
    pub threshold_value: f32,
    pub nms_radius: u32,
    pub min_cluster_size: u32,
    pub refiner: RefinerConfig,
    pub pyramid_levels: u8,
    pub pyramid_min_size: usize,
    pub refinement_radius: u32,
    pub merge_radius: f32,
    /// Optional same-size Gaussian pre-blur applied before ChESS corner
    /// extraction. `0.0` disables preprocessing. Corner positions remain in
    /// the original image frame.
    pub pre_blur_sigma_px: f32,
    pub upscale: UpscaleConfig,
    /// Parameters for the whole-image Radon detector. Only consulted when
    /// [`detector_mode`](Self::detector_mode) is [`DetectorMode::Radon`];
    /// otherwise left at its default.
    pub radon_detector: RadonDetectorParams,
}

impl Default for ChessConfig {
    fn default() -> Self {
        // Workspace-level default: adaptive fraction-of-max threshold.
        // The upstream paper-faithful default is `Absolute 0.0`.
        Self {
            detector_mode: DetectorMode::default(),
            descriptor_mode: DescriptorMode::default(),
            threshold_mode: ThresholdMode::Relative,
            threshold_value: 0.2,
            nms_radius: 2,
            min_cluster_size: 2,
            refiner: RefinerConfig::default(),
            pyramid_levels: 1,
            pyramid_min_size: 128,
            refinement_radius: 3,
            merge_radius: 3.0,
            pre_blur_sigma_px: 0.0,
            upscale: UpscaleConfig::default(),
            radon_detector: RadonDetectorParams::default(),
        }
    }
}

impl ChessConfig {
    pub fn single_scale() -> Self {
        Self::default()
    }

    pub fn multiscale() -> Self {
        Self {
            pyramid_levels: 3,
            pyramid_min_size: 128,
            ..Self::default()
        }
    }

    /// Preset for the whole-image Radon detector. Single-scale by construction.
    pub fn radon() -> Self {
        Self {
            detector_mode: DetectorMode::Radon,
            pyramid_levels: 1,
            ..Self::default()
        }
    }

    /// Convert this workspace config to the upstream `chess_corners::ChessConfig`.
    ///
    /// This is the single conversion point; all detector code calls this method
    /// and passes the result to `find_chess_corners_image`.
    pub fn to_chess_corners_config(&self) -> chess_corners::ChessConfig {
        // `chess_corners::ChessConfig` is `#[non_exhaustive]`, so we must
        // start from `default()` and patch individual fields.
        let mut out = chess_corners::ChessConfig::default();
        out.detector_mode = self.detector_mode;
        out.descriptor_mode = self.descriptor_mode;
        out.threshold_mode = self.threshold_mode;
        out.threshold_value = self.threshold_value;
        out.nms_radius = self.nms_radius;
        out.min_cluster_size = self.min_cluster_size;
        out.refiner = self.refiner.clone();
        out.pyramid_levels = self.pyramid_levels;
        out.pyramid_min_size = self.pyramid_min_size;
        out.refinement_radius = self.refinement_radius;
        out.merge_radius = self.merge_radius;
        out.upscale = self.upscale;
        out.radon_detector = self.radon_detector;
        out
    }

    /// Produce the `chess_corners::ChessParams` (single-scale per-image params).
    #[doc(hidden)]
    pub fn to_chess_params(&self) -> ChessCornerParams {
        self.to_chess_corners_config().to_chess_params()
    }

    /// Produce the `chess_corners::CoarseToFineParams` for multiscale usage.
    #[doc(hidden)]
    pub fn to_coarse_to_fine_params(&self) -> CoarseToFineParams {
        self.to_chess_corners_config().to_coarse_to_fine_params()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_relative_threshold() {
        let cfg = ChessConfig::default();
        assert_eq!(cfg.threshold_mode, ThresholdMode::Relative);
        assert!((cfg.threshold_value - 0.2).abs() < 1e-6);
        assert_eq!(cfg.pre_blur_sigma_px, 0.0);
    }

    #[test]
    fn to_chess_corners_config_roundtrip_non_threshold_fields() {
        let cfg = ChessConfig {
            detector_mode: DetectorMode::Broad,
            descriptor_mode: DescriptorMode::Canonical,
            nms_radius: 3,
            ..ChessConfig::default()
        };
        let up = cfg.to_chess_corners_config();
        assert_eq!(up.detector_mode, chess_corners::DetectorMode::Broad);
        assert_eq!(up.descriptor_mode, chess_corners::DescriptorMode::Canonical);
        assert_eq!(up.nms_radius, 3);
    }

    #[test]
    fn radon_variant_maps_through() {
        let cfg = ChessConfig::radon();
        let up = cfg.to_chess_corners_config();
        assert_eq!(up.detector_mode, chess_corners::DetectorMode::Radon);
    }

    #[test]
    fn upscale_config_roundtrips_json() {
        let cfg = UpscaleConfig::fixed(3);
        let json = serde_json::to_string(&cfg).unwrap();
        let roundtrip: UpscaleConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip, cfg);
    }

    #[test]
    fn chess_config_missing_upscale_deserializes_to_disabled() {
        let json = r#"{"threshold_mode":"relative","threshold_value":0.08}"#;
        let cfg: ChessConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.upscale, UpscaleConfig::disabled());
        assert_eq!(cfg.pre_blur_sigma_px, 0.0);
    }

    #[test]
    fn chess_config_pre_blur_roundtrips_json() {
        let cfg = ChessConfig {
            pre_blur_sigma_px: 1.5,
            ..ChessConfig::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let roundtrip: ChessConfig = serde_json::from_str(&json).unwrap();
        assert!((roundtrip.pre_blur_sigma_px - 1.5).abs() < 1e-6);
    }
}
