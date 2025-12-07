use crate::params::ChessboardParams;
use crate::gridgraph::{GridGraph, GridGraphParams};
use crate::geom::is_aligned_or_orthogonal;
use calib_targets_core::{
    Corner, TargetDetection,
    TargetKind,
};
use log::info;
use nalgebra::Vector2;

/// Estimate two orthogonal grid axes from ChESS corner orientations.
///
/// This respects the fact that your orientations are defined modulo π.
/// It uses a "double-angle" trick to get a dominant direction, then
/// constructs the perpendicular as the second axis.
///
/// Returns (u, v) unit vectors in image pixel space.
pub fn estimate_grid_axes_from_orientations(
    corners: &[Corner],
) -> Option<f32> {
    if corners.is_empty() {
        return None;
    }

    // Accumulate in double-angle space to handle θ ≡ θ + π
    let mut sum = Vector2::<f32>::zeros();
    let mut weight_sum = 0.0f32;

    for c in corners {
        let theta = c.orientation;
        // You can weight by strength to favor strong corners.
        let w = c.strength.max(0.0);
        if w <= 0.0 {
            continue;
        }

        let two_theta = 2.0 * theta;
        let v = Vector2::new(two_theta.cos(), two_theta.sin());
        sum += w * v;
        weight_sum += w;
    }

    if weight_sum <= 0.0 {
        return None;
    }

    let mean = sum / weight_sum;
    if mean.norm_squared() < 1e-6 {
        // No dominant orientation.
        return None;
    }

    // Back to single-angle space.
    let mean_two_angle = mean.y.atan2(mean.x);
    let mean_theta = 0.5 * mean_two_angle;
    Some(mean_theta)
}

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
        let Some(theta_u) = estimate_grid_axes_from_orientations(&strong) else {
            info!("failed to estimate grid axes from orientations");
            return Vec::new();
        };

        let aligned_corners: Vec<Corner> = strong
            .iter()
            .cloned()
            .filter(|c| {
                is_aligned_or_orthogonal(theta_u, c.orientation, self.params.orientation_tolerance_rad)
            })
            .collect();

        let graph_params = GridGraphParams {
            min_spacing_pix: 5.0,
            max_spacing_pix: 100.0,
            k_neighbors: 8,
            orientation_tolerance_rad: self.params.orientation_tolerance_rad,
        };
        let graph = GridGraph::new(&aligned_corners, graph_params);
        println!("{:?}", graph.neighbors);

        vec![TargetDetection {
            kind: TargetKind::Chessboard,
            corners: vec![],
        }]
    }
}
