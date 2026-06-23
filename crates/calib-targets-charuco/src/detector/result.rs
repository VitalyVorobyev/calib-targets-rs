use calib_targets_aruco::MarkerDetection;
use calib_targets_core::{Coord, GridAlignment, LabeledCorner, TargetDetection, TargetKind};
use nalgebra::Point2;
use serde::Serialize;

/// A labelled ChArUco inner corner.
#[non_exhaustive]
#[derive(Clone, Debug, Serialize)]
pub struct CharucoCorner {
    /// Sub-pixel image position.
    pub position: Point2<f32>,
    /// ChArUco board corner coordinate.
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
            grid,
            id,
            target_position,
            score,
        }
    }

    pub(crate) fn from_labeled(corner: LabeledCorner) -> Option<Self> {
        Some(Self {
            position: corner.position,
            grid: corner.grid?,
            id: corner.id?,
            target_position: corner.target_position?,
            score: corner.score,
        })
    }

    /// Convert this typed corner to the shared carrier used by diagnostics and bindings.
    pub fn to_labeled(&self) -> LabeledCorner {
        LabeledCorner::new(self.position, self.score)
            .with_grid(self.grid)
            .with_id(self.id)
            .with_target_position(self.target_position)
    }
}

/// Output of a ChArUco detection run.
///
/// `#[non_exhaustive]`: construct with [`CharucoDetectionResult::new`].
#[non_exhaustive]
#[derive(Clone, Debug, Serialize)]
pub struct CharucoDetectionResult {
    /// Labelled ChArUco inner corners.
    pub corners: Vec<CharucoCorner>,
    /// Marker detections that are consistent with [`Self::alignment`] (inliers
    /// of the chosen hypothesis).
    pub markers: Vec<MarkerDetection>,
    /// Alignment from the rectified grid coordinate system into board coordinates.
    pub alignment: GridAlignment,
}

impl CharucoDetectionResult {
    /// Create a result from its typed corners, inlier markers, and alignment.
    pub fn new(
        corners: Vec<CharucoCorner>,
        markers: Vec<MarkerDetection>,
        alignment: GridAlignment,
    ) -> Self {
        Self {
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
