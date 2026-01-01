//! Checkerboard marker target detector (checkerboard + 3 circles in the middle).
//!
//! Pipeline overview:
//! - Detect a chessboard grid from ChESS corners (partial boards are allowed).
//! - Score circular markers per-cell in image space.
//! - Match circle candidates to the known layout and estimate the grid offset.
//! - Return a `TargetDetection` with `TargetKind::CheckerboardMarker`.
//!
//! ## Quickstart
//!
//! ```
//! use calib_targets_core::{Corner, GrayImageView};
//! use calib_targets_marker::{
//!     CellCoords, CirclePolarity, MarkerBoardDetector, MarkerBoardLayout, MarkerBoardParams,
//!     MarkerCircleSpec,
//! };
//!
//! let layout = MarkerBoardLayout {
//!     rows: 6,
//!     cols: 8,
//!     cell_size: Some(1.0),
//!     circles: [
//!         MarkerCircleSpec {
//!             cell: CellCoords { i: 2, j: 2 },
//!             polarity: CirclePolarity::White,
//!         },
//!         MarkerCircleSpec {
//!             cell: CellCoords { i: 3, j: 2 },
//!             polarity: CirclePolarity::Black,
//!         },
//!         MarkerCircleSpec {
//!             cell: CellCoords { i: 2, j: 3 },
//!             polarity: CirclePolarity::White,
//!         },
//!     ],
//! };
//!
//! let params = MarkerBoardParams::new(layout);
//! let detector = MarkerBoardDetector::new(params);
//!
//! let pixels = vec![0u8; 32 * 32];
//! let view = GrayImageView {
//!     width: 32,
//!     height: 32,
//!     data: &pixels,
//! };
//! let corners: Vec<Corner> = Vec::new();
//!
//! let _ = detector.detect_from_image_and_corners(&view, &corners);
//! ```

pub mod circle_score;
pub mod coords;
pub mod detect;
pub mod match_circles;
pub mod types;

mod detector;

pub use circle_score::{CircleCandidate, CirclePolarity, CircleScoreParams};
pub use coords::{CellCoords, CellOffset};
pub use detector::MarkerBoardDetector;
pub use match_circles::{estimate_grid_alignment, estimate_grid_offset, match_expected_circles};
pub use types::{
    CircleMatch, CircleMatchParams, MarkerBoardDetectionResult, MarkerBoardLayout,
    MarkerBoardParams, MarkerCircleSpec,
};
