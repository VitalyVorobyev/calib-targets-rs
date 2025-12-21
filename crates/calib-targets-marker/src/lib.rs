//! Checkerboard marker target detector (checkerboard + 3 circles in the middle).
//!
//! Design idea:
//! - Use your ChESS-based chessboard detector to get a grid model (even partial).
//! - Detect 3 circles in the image.
//! - Match circle centers to known grid coordinates.
//! - Output a TargetDetection with TargetKind::CheckerboardMarker.

pub mod circle_score;
pub mod detect;

use calib_targets_chessboard::{ChessboardDetector, ChessboardParams};
use calib_targets_core::{Corner, LabeledCorner, TargetDetection, TargetKind};
use nalgebra::Point2;

#[derive(Clone, Debug)]
pub struct MarkerBoardLayout {
    /// Full checkerboard dimensions (e.g., inner corners).
    pub rows: u32,
    pub cols: u32,

    /// Grid coordinates of the three marker circles,
    /// e.g. (i, j) for each circle center in board coordinates.
    pub circle_positions: [(i32, i32); 3],
}

#[derive(Clone, Debug)]
pub struct MarkerBoardParams {
    pub layout: MarkerBoardLayout,
    pub chessboard: ChessboardParams,
    // later: circle detector thresholds, etc.
}

pub struct MarkerBoardDetector {
    params: MarkerBoardParams,
    chessboard_detector: ChessboardDetector,
}

impl MarkerBoardDetector {
    pub fn new(params: MarkerBoardParams) -> Self {
        let chessboard_detector = ChessboardDetector::new(params.chessboard.clone());
        Self {
            params,
            chessboard_detector,
        }
    }

    pub fn params(&self) -> &MarkerBoardParams {
        &self.params
    }

    /// Main entry: detect marker-board from ChESS corners + (eventually) image.
    ///
    /// For v0 we just re-use the chessboard detector and relabel the result
    /// as CheckerboardMarker, so you can plug this into your tests.
    ///
    /// Later:
    /// - add `image: &GrayImage` parameter,
    /// - run circle detector,
    /// - match circles to grid coords.
    pub fn detect_from_corners(&self, corners: &[Corner]) -> Option<TargetDetection> {
        let chess_detection = self.chessboard_detector.detect_from_corners(corners)?;

        // TODO: circle detection + matching logic.
        // For now, simply convert the chessboard detection into a
        // CheckerboardMarker detection with the same corners.
        let mut det = chess_detection.detection;
        det.kind = TargetKind::CheckerboardMarker;
        Some(det)
    }

    /// Placeholder for the future: detect circles (in image coords).
    #[allow(dead_code)]
    fn detect_circles(&self /*, image: &GrayImage */) -> Vec<Point2<f32>> {
        Vec::new()
    }

    /// Placeholder for the future: match circle centers to grid positions.
    #[allow(dead_code)]
    fn assign_circle_ids(
        &self,
        _circle_centers: &[Point2<f32>],
        _corners: &[LabeledCorner],
    ) -> Option<[usize; 3]> {
        // Should return indices into `corners` for each circle,
        // or some mapping to the known layout.circle_positions.
        None
    }
}
