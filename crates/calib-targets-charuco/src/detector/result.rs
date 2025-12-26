use calib_targets_aruco::MarkerDetection;
use calib_targets_core::{GridAlignment, TargetDetection};
use serde::{Deserialize, Serialize};

/// Marker detection with explicit coordinate spaces.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CharucoAlignedMarker {
    pub id: u32,

    /// Square cell coordinates in the *rectified grid* coordinate system.
    ///
    /// This is the coordinate system produced by the chessboard detector (then used to build
    /// per-cell marker sampling quads).
    pub rectified_sx: i32,
    pub rectified_sy: i32,

    /// Square cell coordinates in the *board* coordinate system.
    ///
    /// This is obtained by applying `CharucoDetectionResult.alignment` to `(rectified_sx, rectified_sy)`.
    pub board_sx: i32,
    pub board_sy: i32,

    pub rotation: u8,
    pub hamming: u8,
    pub score: f32,
    pub border_score: f32,
    pub code: u64,
    pub inverted: bool,
}

/// Output of a ChArUco detection run.
#[derive(Clone, Debug)]
pub struct CharucoDetectionResult {
    pub detection: TargetDetection,
    /// Raw marker detections in the rectified grid coordinate system.
    pub markers: Vec<MarkerDetection>,
    /// Alignment from the rectified grid coordinate system into board coordinates.
    pub alignment: GridAlignment,
}

impl CharucoDetectionResult {
    /// Return marker detections with their cell coordinates also expressed in board space.
    pub fn aligned_markers(&self) -> Vec<CharucoAlignedMarker> {
        self.markers
            .iter()
            .map(|m| {
                let [board_sx, board_sy] = self.alignment.map(m.sx, m.sy);
                CharucoAlignedMarker {
                    id: m.id,
                    rectified_sx: m.sx,
                    rectified_sy: m.sy,
                    board_sx,
                    board_sy,
                    rotation: m.rotation,
                    hamming: m.hamming,
                    score: m.score,
                    border_score: m.border_score,
                    code: m.code,
                    inverted: m.inverted,
                }
            })
            .collect()
    }
}
