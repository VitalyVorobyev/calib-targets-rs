use calib_targets_core::OrientationClusteringParams;
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
    /// Minimal corner strength to consider.
    pub min_strength: f32,

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
}

impl Default for ChessboardParams {
    fn default() -> Self {
        Self {
            min_strength: 0.0,
            min_corners: 16,
            expected_rows: None,
            expected_cols: None,
            completeness_threshold: 0.7,
            use_orientation_clustering: true,
            orientation_clustering_params: OrientationClusteringParams::default(),
        }
    }
}
