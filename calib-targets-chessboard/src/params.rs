use calib_targets_core::GridSearchParams;

/// Parameters specific to the chessboard detector.
#[derive(Clone, Debug)]
pub struct ChessboardParams {
    pub grid_search: GridSearchParams,

    /// Expected number of *inner* corners in vertical direction (rows).
    pub expected_rows: Option<u32>,

    /// Expected number of *inner* corners in horizontal direction (cols).
    pub expected_cols: Option<u32>,

    /// Minimal completeness ratio (#detected corners / full grid size)
    /// when expected_rows/cols are provided.
    pub completeness_threshold: f32,
}

impl Default for ChessboardParams {
    fn default() -> Self {
        Self {
            grid_search: GridSearchParams::default(),
            expected_rows: None,
            expected_cols: None,
            completeness_threshold: 0.7,
        }
    }
}
