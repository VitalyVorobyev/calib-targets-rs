use nalgebra::Point2;
use serde::{Deserialize, Serialize};

pub use projective_grid::{AxisEstimate, GridCoords};

/// The kind of target that a detection corresponds to.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    Chessboard,
    Charuco,
    CheckerboardMarker,
    PuzzleBoard,
}

/// A corner that is part of a detected target, with optional ID info.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LabeledCorner {
    /// Pixel position.
    pub position: Point2<f32>,

    /// Optional integer grid coordinates (i, j).
    pub grid: Option<GridCoords>,

    /// Optional logical ID (e.g. ChArUco or marker-board ID).
    pub id: Option<u32>,

    /// Optional target-space position in millimeters (paired with `id`).
    #[serde(default)]
    pub target_position: Option<Point2<f32>>,

    /// Detection score (higher is better).
    ///
    /// The meaning depends on the detector (it may be unnormalized).
    #[serde(alias = "confidence")]
    pub score: f32,
}

/// One detected target (board instance) in an image.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TargetDetection {
    pub kind: TargetKind,
    pub corners: Vec<LabeledCorner>,
}
