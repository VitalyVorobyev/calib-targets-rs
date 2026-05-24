use nalgebra::Point2;
use projective_grid_next::AxisEstimate as NextAxisEstimate;
use serde::{Deserialize, Serialize};

pub use projective_grid::AxisEstimate;

pub use crate::grid_alignment::GridCoords;

// ---- Conversions to / from projective-grid-next ----

/// Promote the legacy `f32`-only [`AxisEstimate`] into the
/// [`projective_grid_next`] crate's `Float`-generic
/// [`NextAxisEstimate<f32>`].
///
/// Implemented as a free function because both types live in foreign
/// crates from this module's POV (the orphan rules forbid an `impl From`).
#[inline]
pub fn axis_estimate_to_next(a: AxisEstimate) -> NextAxisEstimate<f32> {
    NextAxisEstimate::new(a.angle, a.sigma)
}

/// Project a [`NextAxisEstimate<f32>`] back into the legacy shape.
#[inline]
pub fn axis_estimate_from_next(a: NextAxisEstimate<f32>) -> AxisEstimate {
    AxisEstimate {
        angle: a.angle,
        sigma: a.sigma,
    }
}

/// The kind of target that a detection corresponds to.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    /// A plain chessboard: integer-labelled X-junction corners only.
    Chessboard,
    /// A ChArUco board: a chessboard fused with ArUco markers in the
    /// white cells, giving each corner an absolute ID.
    Charuco,
    /// A checkerboard marker board: a chessboard with a small set of
    /// circular markers identifying its orientation.
    CheckerboardMarker,
    /// A PuzzleBoard: a self-identifying chessboard whose edge dots give
    /// every corner an absolute `(I, J)` label.
    PuzzleBoard,
}

/// A corner that is part of a detected target, with optional ID info.
///
/// `#[non_exhaustive]`: this carrier accretes optional fields as detectors
/// gain capabilities. Construct it with [`LabeledCorner::new`] (position +
/// score) and attach grid / ID / target-space metadata with the `with_*`
/// setters.
#[non_exhaustive]
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

impl LabeledCorner {
    /// Create a corner from its required fields (`position`, `score`).
    ///
    /// `grid`, `id`, and `target_position` start unset; attach them with
    /// [`Self::with_grid`], [`Self::with_id`], and
    /// [`Self::with_target_position`].
    pub fn new(position: Point2<f32>, score: f32) -> Self {
        Self {
            position,
            grid: None,
            id: None,
            target_position: None,
            score,
        }
    }

    /// Attach integer grid coordinates `(i, j)`.
    #[must_use]
    pub fn with_grid(mut self, grid: GridCoords) -> Self {
        self.grid = Some(grid);
        self
    }

    /// Attach a logical ID (e.g. ChArUco or marker-board ID).
    #[must_use]
    pub fn with_id(mut self, id: u32) -> Self {
        self.id = Some(id);
        self
    }

    /// Attach a target-space position in millimeters (paired with `id`).
    #[must_use]
    pub fn with_target_position(mut self, target_position: Point2<f32>) -> Self {
        self.target_position = Some(target_position);
        self
    }
}

/// One detected target (board instance) in an image.
///
/// `#[non_exhaustive]`: construct with [`TargetDetection::new`].
#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TargetDetection {
    /// Which kind of calibration target this detection describes.
    pub kind: TargetKind,
    /// The detected corners. No ordering or completeness is promised by
    /// this generic carrier; each detector documents its own guarantees.
    pub corners: Vec<LabeledCorner>,
}

impl TargetDetection {
    /// Create a detection from its target kind and labelled corners.
    pub fn new(kind: TargetKind, corners: Vec<LabeledCorner>) -> Self {
        Self { kind, corners }
    }
}
