use nalgebra::Point2;
use projective_grid::Coord;
use projective_grid::LocalAxis as NextLocalAxis;
use serde::{Deserialize, Serialize};

/// Local estimate of one undirected grid axis at a detected corner.
///
/// `angle` is in radians. `sigma` is the 1σ angular uncertainty in radians.
/// Default-constructed axes carry `sigma = π`, the workspace's no-information
/// sentinel for axis-aware grid builders.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AxisEstimate {
    /// Axis angle in radians.
    pub angle: f32,
    /// 1σ angular uncertainty in radians.
    pub sigma: f32,
}

impl Default for AxisEstimate {
    fn default() -> Self {
        Self {
            angle: 0.0,
            sigma: std::f32::consts::PI,
        }
    }
}

impl AxisEstimate {
    /// Construct an axis estimate from a bare angle with no uncertainty
    /// penalty (`sigma = 0.0`).
    pub fn from_angle(angle: f32) -> Self {
        Self { angle, sigma: 0.0 }
    }
}

// ---- Conversions to / from projective-grid ----

/// Promote [`AxisEstimate`] into the [`projective_grid`] crate's generic
/// local-axis shape.
#[inline]
pub fn axis_estimate_to_next(a: AxisEstimate) -> NextLocalAxis {
    NextLocalAxis::new(a.angle, Some(a.sigma))
}

/// Project a [`NextLocalAxis`] back into the legacy shape.
#[inline]
pub fn axis_estimate_from_next(a: NextLocalAxis) -> AxisEstimate {
    AxisEstimate {
        angle: a.angle_rad,
        sigma: a.sigma_rad.unwrap_or(std::f32::consts::PI),
    }
}

#[cfg(test)]
mod axis_tests {
    use super::*;

    #[test]
    fn default_axis_is_no_information_sentinel() {
        let axis = AxisEstimate::default();
        assert_eq!(axis.angle, 0.0);
        assert_eq!(axis.sigma, std::f32::consts::PI);
    }

    #[test]
    fn from_angle_sets_zero_sigma() {
        let axis = AxisEstimate::from_angle(1.25);
        assert_eq!(axis.angle, 1.25);
        assert_eq!(axis.sigma, 0.0);
    }

    #[test]
    fn axis_round_trips_through_next() {
        let axis = AxisEstimate {
            angle: 0.75,
            sigma: 0.02,
        };
        assert_eq!(axis_estimate_from_next(axis_estimate_to_next(axis)), axis);

        let no_sigma = NextLocalAxis::new(0.5, None);
        assert_eq!(
            axis_estimate_from_next(no_sigma),
            AxisEstimate {
                angle: 0.5,
                sigma: std::f32::consts::PI
            }
        );
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

    /// Optional integer grid coordinates `(u, v)`.
    pub grid: Option<Coord>,

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

    /// Attach integer grid coordinates `(u, v)`.
    #[must_use]
    pub fn with_grid(mut self, grid: Coord) -> Self {
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
