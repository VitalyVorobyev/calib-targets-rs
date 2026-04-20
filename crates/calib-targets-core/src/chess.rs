use serde::{Deserialize, Serialize};

/// Center-of-mass subpixel refinement on the ChESS response map.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CenterOfMassConfig {
    pub radius: i32,
}

impl Default for CenterOfMassConfig {
    fn default() -> Self {
        Self { radius: 2 }
    }
}

/// Forstner-style gradient-based subpixel refinement.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ForstnerConfig {
    pub radius: i32,
    pub min_trace: f32,
    pub min_det: f32,
    pub max_condition_number: f32,
    pub max_offset: f32,
}

impl Default for ForstnerConfig {
    fn default() -> Self {
        Self {
            radius: 2,
            min_trace: 25.0,
            min_det: 1e-3,
            max_condition_number: 50.0,
            max_offset: 1.5,
        }
    }
}

/// Saddle-point subpixel refinement on the source image.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SaddlePointConfig {
    pub radius: i32,
    pub det_margin: f32,
    pub max_offset: f32,
    pub min_abs_det: f32,
}

impl Default for SaddlePointConfig {
    fn default() -> Self {
        Self {
            radius: 2,
            det_margin: 1e-3,
            max_offset: 1.5,
            min_abs_det: 1e-4,
        }
    }
}

/// Detector sampling mode for the ChESS response kernel.
///
/// # Adding a variant
///
/// `#[non_exhaustive]` forces external matchers to use a `_` arm, so adding
/// a variant here will not surface as a compile error in downstream crates.
/// When you add a variant you MUST also update every adapter site in lockstep
/// (each guarded by the workspace-internal exhaustive match in this crate's
/// tests below):
/// - `crates/calib-targets/src/detect.rs::to_detector_mode`
/// - `crates/calib-targets-wasm/src/convert.rs::to_detector_mode`
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectorMode {
    #[default]
    Canonical,
    Broad,
}

/// Descriptor sampling mode for orientation/descriptor extraction.
///
/// # Adding a variant
///
/// See [`DetectorMode`]. Adapter sites:
/// - `crates/calib-targets/src/detect.rs::to_descriptor_mode`
/// - `crates/calib-targets-wasm/src/convert.rs::to_descriptor_mode`
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DescriptorMode {
    #[default]
    FollowDetector,
    Canonical,
    Broad,
}

/// Threshold interpretation mode for ChESS corner detection.
///
/// # Adding a variant
///
/// See [`DetectorMode`]. Adapter sites:
/// - `crates/calib-targets/src/detect.rs::to_threshold_mode`
/// - `crates/calib-targets-wasm/src/convert.rs::to_threshold_mode`
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThresholdMode {
    #[default]
    Relative,
    Absolute,
}

/// User-facing refiner method selector for the high-level ChESS config.
///
/// # Adding a variant
///
/// See [`DetectorMode`]. Adapter sites:
/// - `crates/calib-targets/src/detect.rs::to_refinement_method`
/// - `crates/calib-targets-wasm/src/convert.rs::to_refinement_method`
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefinementMethod {
    #[default]
    CenterOfMass,
    Forstner,
    SaddlePoint,
}

/// Workspace-owned selection of the low-level ChESS subpixel refiner.
///
/// # Adding a variant
///
/// See [`DetectorMode`]. Adapter sites:
/// - `crates/calib-targets-charuco/src/detector/params.rs::to_refiner_kind`
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RefinerKindConfig {
    CenterOfMass(CenterOfMassConfig),
    Forstner(ForstnerConfig),
    SaddlePoint(SaddlePointConfig),
}

impl Default for RefinerKindConfig {
    fn default() -> Self {
        Self::CenterOfMass(CenterOfMassConfig::default())
    }
}

impl RefinerKindConfig {
    pub fn as_refiner_config(&self) -> RefinerConfig {
        match self {
            Self::CenterOfMass(cfg) => RefinerConfig {
                kind: RefinementMethod::CenterOfMass,
                center_of_mass: *cfg,
                ..RefinerConfig::default()
            },
            Self::Forstner(cfg) => RefinerConfig {
                kind: RefinementMethod::Forstner,
                forstner: *cfg,
                ..RefinerConfig::default()
            },
            Self::SaddlePoint(cfg) => RefinerConfig {
                kind: RefinementMethod::SaddlePoint,
                saddle_point: *cfg,
                ..RefinerConfig::default()
            },
        }
    }
}

/// User-facing high-level ChESS refiner configuration.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RefinerConfig {
    pub kind: RefinementMethod,
    pub center_of_mass: CenterOfMassConfig,
    pub forstner: ForstnerConfig,
    pub saddle_point: SaddlePointConfig,
}

impl RefinerConfig {
    pub fn center_of_mass() -> Self {
        Self {
            kind: RefinementMethod::CenterOfMass,
            ..Self::default()
        }
    }

    pub fn forstner() -> Self {
        Self {
            kind: RefinementMethod::Forstner,
            ..Self::default()
        }
    }

    pub fn saddle_point() -> Self {
        Self {
            kind: RefinementMethod::SaddlePoint,
            ..Self::default()
        }
    }

    pub fn to_refiner_kind_config(&self) -> RefinerKindConfig {
        match self.kind {
            RefinementMethod::CenterOfMass => RefinerKindConfig::CenterOfMass(self.center_of_mass),
            RefinementMethod::Forstner => RefinerKindConfig::Forstner(self.forstner),
            RefinementMethod::SaddlePoint => RefinerKindConfig::SaddlePoint(self.saddle_point),
        }
    }
}

/// Tunable parameters for ChESS response computation and local corner detection.
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChessCornerParams {
    pub use_radius10: bool,
    pub descriptor_use_radius10: Option<bool>,
    pub threshold_rel: f32,
    pub threshold_abs: Option<f32>,
    pub nms_radius: u32,
    pub min_cluster_size: u32,
    pub refiner: RefinerKindConfig,
}

impl Default for ChessCornerParams {
    fn default() -> Self {
        Self {
            use_radius10: false,
            descriptor_use_radius10: None,
            threshold_rel: 0.2,
            threshold_abs: None,
            nms_radius: 2,
            min_cluster_size: 2,
            refiner: RefinerKindConfig::default(),
        }
    }
}

/// Parameters for image pyramid construction.
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PyramidParams {
    pub num_levels: u8,
    pub min_size: usize,
}

impl Default for PyramidParams {
    fn default() -> Self {
        Self {
            num_levels: 1,
            min_size: 128,
        }
    }
}

/// Coarse-to-fine multiscale ChESS detector parameters.
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CoarseToFineParams {
    pub pyramid: PyramidParams,
    pub refinement_radius: u32,
    pub merge_radius: f32,
}

impl Default for CoarseToFineParams {
    fn default() -> Self {
        Self {
            pyramid: PyramidParams::default(),
            refinement_radius: 3,
            merge_radius: 3.0,
        }
    }
}

impl CoarseToFineParams {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Optional pre-detection integer upscaling.
///
/// This mirrors `chess-corners` 0.6 without making `calib-targets-core`
/// depend on the detector crate. When enabled, upstream returns corner
/// positions rescaled back into the original input image frame.
///
/// # Adding a variant
///
/// See [`DetectorMode`]. Adapter sites:
/// - `crates/calib-targets/src/detect.rs::to_upscale_config`
/// - `crates/calib-targets-wasm/src/convert.rs::to_upscale_config`
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpscaleMode {
    /// Do not upscale before ChESS detection.
    #[default]
    Disabled,
    /// Upscale by a fixed integer factor.
    Fixed,
}

/// Configuration for the optional pre-pipeline upscaling stage.
///
/// JSON shape:
/// - `{ "mode": "disabled" }`
/// - `{ "mode": "fixed", "factor": 2 }`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UpscaleConfig {
    pub mode: UpscaleMode,
    /// Integer factor used when `mode == Fixed`. Valid fixed factors are 2, 3,
    /// and 4; ignored when disabled.
    pub factor: u32,
}

impl Default for UpscaleConfig {
    fn default() -> Self {
        Self {
            mode: UpscaleMode::Disabled,
            factor: 2,
        }
    }
}

impl Serialize for UpscaleConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;

        match self.mode {
            UpscaleMode::Disabled => {
                let mut state = serializer.serialize_struct("UpscaleConfig", 1)?;
                state.serialize_field("mode", &self.mode)?;
                state.end()
            }
            UpscaleMode::Fixed => {
                let mut state = serializer.serialize_struct("UpscaleConfig", 2)?;
                state.serialize_field("mode", &self.mode)?;
                state.serialize_field("factor", &self.factor)?;
                state.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for UpscaleConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(default)]
        struct Helper {
            mode: UpscaleMode,
            factor: u32,
        }

        impl Default for Helper {
            fn default() -> Self {
                let cfg = UpscaleConfig::default();
                Self {
                    mode: cfg.mode,
                    factor: cfg.factor,
                }
            }
        }

        let helper = Helper::deserialize(deserializer)?;
        Ok(Self {
            mode: helper.mode,
            factor: helper.factor,
        })
    }
}

impl UpscaleConfig {
    pub fn disabled() -> Self {
        Self::default()
    }

    pub fn fixed(factor: u32) -> Self {
        Self {
            mode: UpscaleMode::Fixed,
            factor,
        }
    }

    pub fn effective_factor(&self) -> u32 {
        match self.mode {
            UpscaleMode::Disabled => 1,
            UpscaleMode::Fixed => self.factor,
        }
    }

    pub fn validate(&self) -> Result<(), UpscaleConfigError> {
        if matches!(self.mode, UpscaleMode::Fixed) && !matches!(self.factor, 2..=4) {
            return Err(UpscaleConfigError::InvalidFactor(self.factor));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpscaleConfigError {
    InvalidFactor(u32),
}

impl std::fmt::Display for UpscaleConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidFactor(factor) => {
                write!(
                    f,
                    "upscale factor {factor} not supported (expected 2, 3, or 4)"
                )
            }
        }
    }
}

impl std::error::Error for UpscaleConfigError {}

/// Workspace-owned high-level ChESS detector configuration.
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
    pub upscale: UpscaleConfig,
}

impl Default for ChessConfig {
    fn default() -> Self {
        // Workspace-level default intentionally layers an adaptive
        // fraction-of-max threshold (`Relative 0.2`) on top of the paper's
        // strict `R > 0` rule that chess-corners 0.6 exposes by default.
        // Downstream detectors (chessboard / charuco / puzzleboard) rely
        // on this mild pre-filter to keep candidate counts manageable;
        // callers wanting the raw upstream behavior set `Absolute 0.0`.
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
            upscale: UpscaleConfig::default(),
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

    #[doc(hidden)]
    pub fn to_chess_params(&self) -> ChessCornerParams {
        let mut params = ChessCornerParams {
            use_radius10: matches!(self.detector_mode, DetectorMode::Broad),
            descriptor_use_radius10: match self.descriptor_mode {
                DescriptorMode::FollowDetector => None,
                DescriptorMode::Canonical => Some(false),
                DescriptorMode::Broad => Some(true),
            },
            nms_radius: self.nms_radius,
            min_cluster_size: self.min_cluster_size,
            refiner: self.refiner.to_refiner_kind_config(),
            ..ChessCornerParams::default()
        };
        match self.threshold_mode {
            ThresholdMode::Relative => {
                params.threshold_rel = self.threshold_value;
                params.threshold_abs = None;
            }
            ThresholdMode::Absolute => {
                params.threshold_abs = Some(self.threshold_value);
            }
        }
        params
    }

    #[doc(hidden)]
    pub fn to_coarse_to_fine_params(&self) -> CoarseToFineParams {
        CoarseToFineParams {
            pyramid: PyramidParams {
                num_levels: self.pyramid_levels,
                min_size: self.pyramid_min_size,
            },
            refinement_radius: self.refinement_radius,
            merge_radius: self.merge_radius,
        }
    }

    #[doc(hidden)]
    pub fn from_parts(params: &ChessCornerParams, multiscale: &CoarseToFineParams) -> Self {
        Self {
            detector_mode: if params.use_radius10 {
                DetectorMode::Broad
            } else {
                DetectorMode::Canonical
            },
            descriptor_mode: match params.descriptor_use_radius10 {
                None => DescriptorMode::FollowDetector,
                Some(false) => DescriptorMode::Canonical,
                Some(true) => DescriptorMode::Broad,
            },
            threshold_mode: if params.threshold_abs.is_some() {
                ThresholdMode::Absolute
            } else {
                ThresholdMode::Relative
            },
            threshold_value: params.threshold_abs.unwrap_or(params.threshold_rel),
            nms_radius: params.nms_radius,
            min_cluster_size: params.min_cluster_size,
            refiner: params.refiner.as_refiner_config(),
            pyramid_levels: multiscale.pyramid.num_levels,
            pyramid_min_size: multiscale.pyramid.min_size,
            refinement_radius: multiscale.refinement_radius,
            merge_radius: multiscale.merge_radius,
            upscale: UpscaleConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upscale_default_is_disabled_with_factor_two_hint() {
        let cfg = UpscaleConfig::default();
        assert_eq!(cfg.mode, UpscaleMode::Disabled);
        assert_eq!(cfg.factor, 2);
        assert_eq!(cfg.effective_factor(), 1);
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn upscale_fixed_validation_accepts_supported_factors() {
        for factor in [2, 3, 4] {
            let cfg = UpscaleConfig::fixed(factor);
            assert_eq!(cfg.effective_factor(), factor);
            assert!(cfg.validate().is_ok());
        }
    }

    #[test]
    fn upscale_fixed_validation_rejects_unsupported_factors() {
        for factor in [0, 1, 5] {
            assert_eq!(
                UpscaleConfig::fixed(factor).validate(),
                Err(UpscaleConfigError::InvalidFactor(factor))
            );
        }
    }

    #[test]
    fn upscale_config_roundtrips_json() {
        let cfg = UpscaleConfig::fixed(3);
        let json = serde_json::to_string(&cfg).unwrap();
        assert_eq!(json, r#"{"mode":"fixed","factor":3}"#);
        let roundtrip: UpscaleConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip, cfg);
    }

    #[test]
    fn upscale_disabled_serializes_without_factor() {
        let cfg = UpscaleConfig::disabled();
        let json = serde_json::to_string(&cfg).unwrap();
        assert_eq!(json, r#"{"mode":"disabled"}"#);
        let roundtrip: UpscaleConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip, cfg);
    }

    #[test]
    fn chess_config_missing_upscale_deserializes_to_disabled() {
        let json = r#"{"threshold_mode":"relative","threshold_value":0.08}"#;
        let cfg: ChessConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.upscale, UpscaleConfig::disabled());
    }

    // -----------------------------------------------------------------------
    // Variant-guard tests for `#[non_exhaustive]` enums consumed by external
    // adapters.
    //
    // Inside the defining crate, `#[non_exhaustive]` does not suppress the
    // exhaustiveness check, so the matches below WILL fail to compile when a
    // new variant is added. That compile error is the workspace's stable-Rust
    // substitute for the unstable `non_exhaustive_omitted_patterns` lint:
    // when it triggers, update the listed adapter sites in lockstep.
    // -----------------------------------------------------------------------

    #[test]
    fn upscale_mode_variant_guard() {
        // ADAPTERS:
        //   crates/calib-targets/src/detect.rs::to_upscale_config
        //   crates/calib-targets-wasm/src/convert.rs::to_upscale_config
        for mode in [UpscaleMode::Disabled, UpscaleMode::Fixed] {
            match mode {
                UpscaleMode::Disabled => (),
                UpscaleMode::Fixed => (),
            }
        }
    }

    #[test]
    fn detector_mode_variant_guard() {
        // ADAPTERS:
        //   crates/calib-targets/src/detect.rs::to_detector_mode
        //   crates/calib-targets-wasm/src/convert.rs::to_detector_mode
        for mode in [DetectorMode::Canonical, DetectorMode::Broad] {
            match mode {
                DetectorMode::Canonical => (),
                DetectorMode::Broad => (),
            }
        }
    }

    #[test]
    fn descriptor_mode_variant_guard() {
        // ADAPTERS:
        //   crates/calib-targets/src/detect.rs::to_descriptor_mode
        //   crates/calib-targets-wasm/src/convert.rs::to_descriptor_mode
        for mode in [
            DescriptorMode::FollowDetector,
            DescriptorMode::Canonical,
            DescriptorMode::Broad,
        ] {
            match mode {
                DescriptorMode::FollowDetector => (),
                DescriptorMode::Canonical => (),
                DescriptorMode::Broad => (),
            }
        }
    }

    #[test]
    fn threshold_mode_variant_guard() {
        // ADAPTERS:
        //   crates/calib-targets/src/detect.rs::to_threshold_mode
        //   crates/calib-targets-wasm/src/convert.rs::to_threshold_mode
        for mode in [ThresholdMode::Relative, ThresholdMode::Absolute] {
            match mode {
                ThresholdMode::Relative => (),
                ThresholdMode::Absolute => (),
            }
        }
    }

    #[test]
    fn refinement_method_variant_guard() {
        // ADAPTERS:
        //   crates/calib-targets/src/detect.rs::to_refinement_method
        //   crates/calib-targets-wasm/src/convert.rs::to_refinement_method
        for method in [
            RefinementMethod::CenterOfMass,
            RefinementMethod::Forstner,
            RefinementMethod::SaddlePoint,
        ] {
            match method {
                RefinementMethod::CenterOfMass => (),
                RefinementMethod::Forstner => (),
                RefinementMethod::SaddlePoint => (),
            }
        }
    }

    #[test]
    fn refiner_kind_config_variant_guard() {
        // ADAPTER:
        //   crates/calib-targets-charuco/src/detector/params.rs::to_refiner_kind
        let configs = [
            RefinerKindConfig::CenterOfMass(CenterOfMassConfig::default()),
            RefinerKindConfig::Forstner(ForstnerConfig::default()),
            RefinerKindConfig::SaddlePoint(SaddlePointConfig::default()),
        ];
        for cfg in configs {
            match cfg {
                RefinerKindConfig::CenterOfMass(_) => (),
                RefinerKindConfig::Forstner(_) => (),
                RefinerKindConfig::SaddlePoint(_) => (),
            }
        }
    }
}
