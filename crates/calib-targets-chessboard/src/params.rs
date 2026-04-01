use calib_targets_core::{ChessConfig, OrientationClusteringParams};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GridGraphParams {
    pub min_spacing_pix: f32,
    pub max_spacing_pix: f32,
    pub k_neighbors: usize,
    pub orientation_tolerance_deg: f32,
}

impl Default for GridGraphParams {
    fn default() -> Self {
        Self {
            min_spacing_pix: 5.0,
            max_spacing_pix: 50.0,
            k_neighbors: 8,
            orientation_tolerance_deg: 22.5,
        }
    }
}

/// Parameters specific to the chessboard detector.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChessboardParams {
    /// ChESS corner detector configuration.
    #[serde(default)]
    pub chess: ChessConfig,

    /// Minimal corner strength to consider.
    pub min_corner_strength: f32,

    /// Minimal number of corners in a detection to be considered valid.
    pub min_corners: usize,

    /// Expected number of *inner* corners in vertical direction (rows).
    pub expected_rows: Option<u32>,

    /// Expected number of *inner* corners in horizontal direction (cols).
    pub expected_cols: Option<u32>,

    /// Minimal completeness ratio (#detected corners / full grid size)
    /// when expected_rows/cols are provided.
    pub completeness_threshold: f32,

    pub use_orientation_clustering: bool,
    pub orientation_clustering_params: OrientationClusteringParams,

    /// Grid graph construction parameters.
    #[serde(default)]
    pub graph: GridGraphParams,
}

impl ChessboardParams {
    /// Three-config sweep preset: default + high-threshold + low-threshold.
    ///
    /// Useful for challenging images where a single threshold may miss corners.
    pub fn sweep_default() -> Vec<Self> {
        let base = Self::default();
        let mut high = base.clone();
        high.chess.threshold_value = 0.15;
        let mut low = base.clone();
        low.chess.threshold_value = 0.08;
        vec![base, high, low]
    }
}

impl Default for ChessboardParams {
    fn default() -> Self {
        Self {
            chess: ChessConfig::default(),
            min_corner_strength: 0.0,
            min_corners: 16,
            expected_rows: None,
            expected_cols: None,
            completeness_threshold: 0.7,
            use_orientation_clustering: true,
            orientation_clustering_params: OrientationClusteringParams::default(),
            graph: GridGraphParams::default(),
        }
    }
}
