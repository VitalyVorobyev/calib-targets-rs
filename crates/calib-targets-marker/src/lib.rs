//! Checkerboard marker target detector (checkerboard + 3 circles in the middle).
//!
//! Pipeline overview:
//! - Detect a chessboard grid from ChESS corners (partial boards are allowed).
//! - Score circular markers per-cell in image space.
//! - Match circle candidates to the known layout and estimate the grid offset.
//! - Return a `TargetDetection` with `TargetKind::CheckerboardMarker`.

pub mod circle_score;
pub mod coords;
pub mod detect;
pub mod match_circles;
pub mod types;

mod detector;

pub use circle_score::{CircleCandidate, CirclePolarity, CircleScoreParams};
pub use coords::{CellCoords, CellOffset};
pub use detector::MarkerBoardDetector;
pub use match_circles::{estimate_grid_offset, match_expected_circles};
pub use types::{
    CircleMatch, CircleMatchParams, MarkerBoardDetectionResult, MarkerBoardLayout,
    MarkerBoardParams, MarkerCircleSpec,
};
