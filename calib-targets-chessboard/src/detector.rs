use crate::params::ChessboardParams;
use calib_targets_core::{
    estimate_grid_axes_from_orientations, Corner, GridCoords, LabeledCorner, TargetDetection,
    TargetKind,
};
use log::info;
use nalgebra::Vector2;

/// Simple chessboard detector using ChESS orientations + grid fitting in (u, v) space.
pub struct ChessboardDetector {
    pub params: ChessboardParams,
}

impl ChessboardDetector {
    pub fn new(params: ChessboardParams) -> Self {
        Self { params }
    }

    /// Main entry point: find chessboard(s) in a cloud of ChESS corners.
    ///
    /// This function expects corners already computed by your ChESS crate.
    /// For now it returns at most one detection (the best grid).
    pub fn detect_from_corners(&self, corners: &[Corner]) -> Vec<TargetDetection> {
        // 1. Filter by strength.
        let strong: Vec<Corner> = corners
            .iter()
            .cloned()
            .filter(|c| c.strength >= self.params.grid_search.min_strength)
            .collect();

        info!(
            "found {} raw ChESS corners after strength filter",
            strong.len()
        );

        if strong.len() < self.params.grid_search.min_corners {
            return Vec::new();
        }

        // 2. Estimate grid axes from orientations.
        let Some((u_axis_unit, v_axis_unit)) = estimate_grid_axes_from_orientations(&strong) else {
            info!("failed to estimate grid axes from orientations");
            return Vec::new();
        };

        let theta_u = u_axis_unit.angle(&Vector2::x_axis());
        let theta_v = v_axis_unit.angle(&Vector2::x_axis());

        vec![TargetDetection {
            kind: TargetKind::Chessboard,
            corners: vec![],
        }]
    }
}
