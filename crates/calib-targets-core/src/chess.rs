use serde::{Deserialize, Serialize};

/// Center-of-mass subpixel refinement on the ChESS response map.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CenterOfMassConfig {
    pub radius: i32,
}

impl Default for CenterOfMassConfig {
    fn default() -> Self {
        Self { radius: 2 }
    }
}

/// Forstner-style gradient-based subpixel refinement.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
            max_condition_number: 1e3,
            max_offset: 1.5,
        }
    }
}

/// Saddle-point subpixel refinement on the source image.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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

/// Workspace-owned selection of the ChESS subpixel refiner.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RefinerConfig {
    CenterOfMass(CenterOfMassConfig),
    Forstner(ForstnerConfig),
    SaddlePoint(SaddlePointConfig),
}

impl Default for RefinerConfig {
    fn default() -> Self {
        Self::CenterOfMass(CenterOfMassConfig::default())
    }
}

/// Tunable parameters for ChESS response computation and local corner detection.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChessCornerParams {
    pub use_radius10: bool,
    pub descriptor_use_radius10: Option<bool>,
    pub threshold_rel: f32,
    pub threshold_abs: Option<f32>,
    pub nms_radius: u32,
    pub min_cluster_size: u32,
    pub refiner: RefinerConfig,
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
            refiner: RefinerConfig::default(),
        }
    }
}

/// Parameters for image pyramid construction.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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

/// Workspace-owned ChESS detector configuration used by facade helpers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct ChessConfig {
    pub params: ChessCornerParams,
    pub multiscale: CoarseToFineParams,
}

impl ChessConfig {
    /// Recommended coarse-to-fine starting point.
    pub fn multiscale() -> Self {
        Self {
            multiscale: CoarseToFineParams {
                pyramid: PyramidParams {
                    num_levels: 3,
                    min_size: 128,
                },
                ..CoarseToFineParams::default()
            },
            ..Self::default()
        }
    }

    /// Convenience helper for single-scale detection.
    pub fn single_scale() -> Self {
        Self::default()
    }
}
