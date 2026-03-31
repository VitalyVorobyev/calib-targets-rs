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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectorMode {
    #[default]
    Canonical,
    Broad,
}

/// Descriptor sampling mode for orientation/descriptor extraction.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DescriptorMode {
    #[default]
    FollowDetector,
    Canonical,
    Broad,
}

/// Threshold interpretation mode for ChESS corner detection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThresholdMode {
    #[default]
    Relative,
    Absolute,
}

/// User-facing refiner method selector for the high-level ChESS config.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefinementMethod {
    #[default]
    CenterOfMass,
    Forstner,
    SaddlePoint,
}

/// Workspace-owned selection of the low-level ChESS subpixel refiner.
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

/// Workspace-owned high-level ChESS detector configuration matching chess-corners 0.5.
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
}

impl Default for ChessConfig {
    fn default() -> Self {
        Self {
            detector_mode: DetectorMode::default(),
            descriptor_mode: DescriptorMode::default(),
            threshold_mode: ThresholdMode::default(),
            threshold_value: 0.2,
            nms_radius: 2,
            min_cluster_size: 2,
            refiner: RefinerConfig::default(),
            pyramid_levels: 1,
            pyramid_min_size: 128,
            refinement_radius: 3,
            merge_radius: 3.0,
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
        }
    }
}
