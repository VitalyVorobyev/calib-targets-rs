use calib_targets_aruco::MarkerDetection;
use calib_targets_core::{
    Coord, DetectionQuality, GridAlignment, LabeledCorner, PositionCovariance, TargetDetection,
    TargetKind,
};
use nalgebra::Point2;
use serde::{Deserialize, Serialize};

/// A labelled ChArUco inner corner.
#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CharucoCorner {
    /// Sub-pixel image position.
    pub position: Point2<f32>,
    /// Covariance of [`Self::position`] in px², carried from the corner
    /// detector; `None` when it measured none. See [`PositionCovariance`].
    #[serde(default)]
    pub position_covariance: Option<PositionCovariance>,
    /// ChArUco board corner coordinate.
    ///
    /// `u` runs along grid `i` (rightward), `v` along grid `j` (downward).
    pub grid: Coord,
    /// ChArUco logical corner ID.
    pub id: u32,
    /// Physical board-space position in millimetres.
    pub target_position: Point2<f32>,
    /// Detector-specific corner score; higher is better.
    pub score: f32,
}

impl CharucoCorner {
    /// Create a ChArUco corner from its required fields.
    pub fn new(
        position: Point2<f32>,
        grid: Coord,
        id: u32,
        target_position: Point2<f32>,
        score: f32,
    ) -> Self {
        Self {
            position,
            position_covariance: None,
            grid,
            id,
            target_position,
            score,
        }
    }

    /// Set the covariance of [`Self::position`] (px²). Accepts a
    /// [`PositionCovariance`] or an `Option` of one.
    #[must_use]
    pub fn with_position_covariance(
        mut self,
        position_covariance: impl Into<Option<PositionCovariance>>,
    ) -> Self {
        self.position_covariance = position_covariance.into();
        self
    }

    pub(crate) fn from_labeled(corner: LabeledCorner) -> Option<Self> {
        Some(Self {
            position: corner.position,
            position_covariance: corner.position_covariance,
            grid: corner.grid?,
            id: corner.id?,
            target_position: corner.target_position?,
            score: corner.score,
        })
    }

    /// Convert this typed corner to the shared carrier used by diagnostics and bindings.
    pub fn to_labeled(&self) -> LabeledCorner {
        LabeledCorner::new(self.position, self.score)
            .with_position_covariance(self.position_covariance)
            .with_grid(self.grid)
            .with_id(self.id)
            .with_target_position(self.target_position)
    }
}

/// Output of a ChArUco detection run.
///
/// `#[non_exhaustive]`: construct with [`CharucoDetection::new`].
#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CharucoDetection {
    /// Labelled ChArUco inner corners.
    pub corners: Vec<CharucoCorner>,
    /// Marker detections that are consistent with [`Self::alignment`] (inliers
    /// of the chosen hypothesis).
    pub markers: Vec<MarkerDetection>,
    /// Alignment from the rectified grid coordinate system into board coordinates.
    pub alignment: GridAlignment,
    /// How well [`Self::corners`] are localized, measured on the delivered
    /// corners (after re-detection and merging). See [`DetectionQuality`].
    /// [`noise_sigma`](DetectionQuality::noise_sigma) is set when the detector
    /// ran the corner pass itself ([`CharucoDetector::detect`](crate::CharucoDetector::detect)).
    #[serde(default)]
    pub quality: DetectionQuality,
}

impl CharucoDetection {
    /// Create a result from its typed corners, inlier markers, and alignment,
    /// measuring its [`quality`](Self::quality).
    pub fn new(
        corners: Vec<CharucoCorner>,
        markers: Vec<MarkerDetection>,
        alignment: GridAlignment,
    ) -> Self {
        let labeled: Vec<LabeledCorner> = corners.iter().map(CharucoCorner::to_labeled).collect();
        Self {
            quality: DetectionQuality::from_corners(&labeled),
            corners,
            markers,
            alignment,
        }
    }

    pub(crate) fn from_target_detection(
        detection: TargetDetection,
        markers: Vec<MarkerDetection>,
        alignment: GridAlignment,
    ) -> Self {
        debug_assert_eq!(detection.kind, TargetKind::Charuco);
        let input_len = detection.corners.len();
        let corners: Vec<CharucoCorner> = detection
            .corners
            .into_iter()
            .filter_map(CharucoCorner::from_labeled)
            .collect();
        debug_assert_eq!(corners.len(), input_len);
        Self::new(corners, markers, alignment)
    }

    /// Convert typed corners into the shared `TargetDetection` carrier.
    pub fn target_detection(&self) -> TargetDetection {
        TargetDetection::new(
            TargetKind::Charuco,
            self.corners.iter().map(CharucoCorner::to_labeled).collect(),
        )
    }
}
