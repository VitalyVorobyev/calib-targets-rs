use nalgebra::{Point2, Vector2};
use serde::{Deserialize, Serialize};

pub use projective_grid::GridCoords;

/// One local grid-axis direction at a corner, with its 1σ angular uncertainty.
///
/// Mirrors the upstream `chess_corners::AxisEstimate` so that
/// `calib-targets-core`'s public API does not leak the external detector crate.
/// The conversion from upstream to this type happens in the single
/// `adapt_chess_corner` adapter per consumer crate.
///
/// Convention (matches chess-corners 0.6):
/// - `angle` is in radians, canonicalised to `[0, π)` for axis 0 and to
///   `(axes[0].angle, axes[0].angle + π)` for axis 1; the CCW sweep from axis 0
///   to axis 1 crosses a dark sector.
/// - `sigma` is the 1σ angular uncertainty from the Gauss–Newton covariance
///   of the two-axis intensity fit. Default-constructed axes carry `sigma = π`
///   (no information), so consumers that weight by sigma naturally ignore them.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AxisEstimate {
    pub angle: f32,
    pub sigma: f32,
}

impl Default for AxisEstimate {
    fn default() -> Self {
        // No-information default. Downstream code that weights by `sigma`
        // must treat `π` as "this axis is unusable".
        Self {
            angle: 0.0,
            sigma: std::f32::consts::PI,
        }
    }
}

/// Canonical 2D corner used by all target detectors.
///
/// Obtained by adapting the output of your ChESS crate. Carries the two-axis
/// descriptor (`axes`) emitted by the chess-corners 0.6 detector; all
/// orientation-driven graph / clustering code now reads the axes directly.
///
/// `Default::default()` yields a zero-origin corner with `axes` at the no-info
/// sentinel (sigma = π); test fixtures typically use `..Corner::default()` to
/// fill the new 0.6 fields without listing them explicitly.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Corner {
    /// Corner position in pixel coordinates.
    pub position: Point2<f32>,

    /// Orientation-cluster label assigned at runtime by the chessboard
    /// detector's clustering pass. `None` until clustering runs (or if the
    /// corner falls outside both cluster centers).
    pub orientation_cluster: Option<usize>, // Some(0 or 1) if clustered, None if outlier

    /// The two local grid-axis directions with per-axis 1σ precision.
    ///
    /// Populated from `chess_corners::CornerDescriptor::axes` in 0.6+; older
    /// consumers may leave this as `Default` (sigma = π, i.e. "no info"). All
    /// orientation-sensitive downstream code (clustering, neighbor validation,
    /// local-step estimation) reads these directly.
    #[serde(default)]
    pub axes: [AxisEstimate; 2],

    /// Bright/dark amplitude `|A|` (≥ 0, gray levels) recovered by the upstream
    /// two-axis tanh fit. Independent from [`Self::strength`] — do not compare
    /// against the same threshold.
    #[serde(default)]
    pub contrast: f32,

    /// RMS residual of the two-axis intensity fit (gray levels). Lower is a
    /// tighter match to an ideal chessboard corner; used by the forthcoming
    /// `contrast`/`fit_rms` pre-filter.
    #[serde(default)]
    pub fit_rms: f32,

    /// Strength / response of the corner detector (raw ChESS response at the
    /// detected peak). Positive values are corner candidates per the paper.
    pub strength: f32,
}

impl Corner {
    /// Convenience accessor for (x, y) as a vector.
    pub fn as_vec2(&self) -> Vector2<f32> {
        Vector2::new(self.position.x, self.position.y)
    }
}

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
