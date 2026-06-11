//! Checkerboard marker target detector (checkerboard + 3 circles in the middle).
//!
//! Pipeline overview:
//! - Detect a chessboard grid from ChESS corners (partial boards are allowed).
//! - Score circular markers per-cell in image space.
//! - Match circle candidates to the known layout and estimate the grid offset.
//! - Return typed marker-board corners with optional board alignment.
//!
//! ## Quickstart
//!
//! ```
//! use calib_targets_chessboard::ChessCorner;
//! use calib_targets_core::GrayImageView;
//! use calib_targets_marker::{
//!     CellCoords, CirclePolarity, MarkerBoardDetector, MarkerBoardSpec, MarkerBoardParams,
//!     MarkerCircleSpec,
//! };
//!
//! let layout = MarkerBoardSpec::new(
//!     6,
//!     8,
//!     [
//!         MarkerCircleSpec::new(CellCoords { i: 2, j: 2 }, CirclePolarity::White),
//!         MarkerCircleSpec::new(CellCoords { i: 3, j: 2 }, CirclePolarity::Black),
//!         MarkerCircleSpec::new(CellCoords { i: 2, j: 3 }, CirclePolarity::White),
//!     ],
//! )
//! .with_cell_size(1.0);
//!
//! let params = MarkerBoardParams::new(layout);
//! let detector = MarkerBoardDetector::new(params).expect("valid chessboard config");
//!
//! let pixels = vec![0u8; 32 * 32];
//! let view = GrayImageView {
//!     width: 32,
//!     height: 32,
//!     data: &pixels,
//! };
//! let corners: Vec<ChessCorner> = Vec::new();
//!
//! let _ = detector.detect_from_image_and_corners(&view, &corners);
//! ```
#![deny(missing_docs)]

mod circle_score;
mod coords;
mod detect;
mod io;
mod match_circles;
mod types;

mod detector;

pub mod diagnostics;

pub use circle_score::{CircleCandidate, CirclePolarity, CircleScoreParams};
pub use coords::{CellCoords, CellOffset};
pub use detector::MarkerBoardDetector;
pub use diagnostics::MarkerBoardDiagnostics;
pub use io::{MarkerBoardDetectConfig, MarkerBoardDetectReport, MarkerBoardIoError};
pub use types::{
    CircleMatch, CircleMatchParams, MarkerBoardCorner, MarkerBoardDetectionResult,
    MarkerBoardParams, MarkerBoardSpec, MarkerCircleSpec,
};
