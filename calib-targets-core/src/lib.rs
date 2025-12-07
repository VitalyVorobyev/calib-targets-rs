//! Core types and utilities for calibration target detection.
//!
//! This crate is intentionally small and purely geometric. It does *not*
//! depend on any concrete corner detector or image type.

use nalgebra::{Point2, Unit, Vector2};

/// Canonical 2D corner used by all target detectors.
///
/// This is the thing you obtain by adapting the output of your ChESS crate.
#[derive(Clone, Debug)]
pub struct Corner {
    /// Corner position in pixel coordinates.
    pub position: Point2<f32>,

    /// Dominant grid orientation at the corner, in radians.
    ///
    /// Convention:
    /// - Defined modulo π (pi), not 2π, because chessboard axes are undirected.
    /// - Typically points along one local grid axis.
    pub orientation: f32,

    /// Strength / response of the corner detector.
    pub strength: f32,

    /// Optional phase / parity (0..3) describing local black/white configuration.
    pub phase: u8,
}

impl Corner {
    /// Convenience accessor for (x, y) as a vector.
    pub fn as_vec2(&self) -> Vector2<f32> {
        Vector2::new(self.position.x, self.position.y)
    }
}

/// Integer grid coordinates (i, j) in board space.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct GridCoords {
    pub i: i32,
    pub j: i32,
}

/// The kind of target that a detection corresponds to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetKind {
    Chessboard,
    Charuco,
    CheckerboardMarker,
}

/// A corner that is part of a detected target, with optional ID info.
#[derive(Clone, Debug)]
pub struct LabeledCorner {
    /// Pixel position.
    pub position: Point2<f32>,

    /// Optional integer grid coordinates (i, j).
    pub grid: Option<GridCoords>,

    /// Optional logical ID (e.g. ChArUco or marker-board ID).
    pub id: Option<u32>,

    /// Optional detection confidence [0, 1].
    pub confidence: f32,
}

/// One detected target (board instance) in an image.
#[derive(Clone, Debug)]
pub struct TargetDetection {
    pub kind: TargetKind,
    pub corners: Vec<LabeledCorner>,
}

/// Shared parameters for grid search (tune per pattern if needed).
#[derive(Clone, Debug)]
pub struct GridSearchParams {
    /// Minimal corner strength to consider.
    pub min_strength: f32,

    /// Minimal number of corners in a detection to be considered valid.
    pub min_corners: usize,
}

impl Default for GridSearchParams {
    fn default() -> Self {
        Self {
            min_strength: 0.0,
            min_corners: 16,
        }
    }
}

/// Estimate two orthogonal grid axes from ChESS corner orientations.
///
/// This respects the fact that your orientations are defined modulo π.
/// It uses a "double-angle" trick to get a dominant direction, then
/// constructs the perpendicular as the second axis.
pub fn estimate_grid_orientations(corners: &[Corner]) -> Option<f32> {
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

    let u = Unit::new_normalize(Vector2::new(mean_theta.cos(), mean_theta.sin()));
    Some(u.angle(&Vector2::x_axis()))
}
