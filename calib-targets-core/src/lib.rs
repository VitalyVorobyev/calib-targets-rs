//! Core types and utilities for calibration target detection.
//!
//! This crate is intentionally small and purely geometric. It does *not*
//! depend on any concrete corner detector or image type.

use nalgebra::{Point2, Vector2};

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
