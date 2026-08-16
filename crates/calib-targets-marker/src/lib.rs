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
//! use calib_targets_marker::{
//!     CellCoords, CirclePolarity, GrayImageView, MarkerBoardDetector, MarkerBoardSpec,
//!     MarkerBoardParams, MarkerCircleSpec,
//! };
//!
//! let board = MarkerBoardSpec::new(
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
//! let params = MarkerBoardParams::for_board(board);
//! let detector = MarkerBoardDetector::new(params).expect("valid chessboard config");
//!
//! let pixels = vec![0u8; 32 * 32];
//! let view = GrayImageView {
//!     width: 32,
//!     height: 32,
//!     data: &pixels,
//! };
//!
//! // `detect` runs the ChESS corner front-end from `params.chess` itself.
//! // Feed a corner cloud you already have to `detect_with_corners` instead.
//! let _ = detector.detect(&view);
//! ```
#![deny(missing_docs)]

mod circle_score;
mod coords;
mod detect;
mod error;
mod match_circles;
mod types;

mod detector;

// Opt-in introspection surface, gated behind the `diagnostics` feature (default
// off), mirroring `calib-targets-chessboard` / `-charuco` / `-puzzleboard`.
// The diagnostics types are always compiled (the detector captures them
// internally) but only reach the public surface — the `diagnostics` module,
// the type re-export, and `MarkerBoardDetector::diagnose` /
// `diagnose_with_corners` — when the feature is enabled.
#[cfg(feature = "diagnostics")]
pub mod diagnostics;
#[cfg(not(feature = "diagnostics"))]
pub(crate) mod diagnostics;

pub use circle_score::{CircleCandidate, CirclePolarity, CircleScoreParams};
pub use coords::{CellCoords, CellOffset};
pub use detector::MarkerBoardDetector;
#[cfg(feature = "diagnostics")]
pub use diagnostics::MarkerBoardDiagnostics;
pub use error::MarkerBoardDetectError;
pub use types::{
    CircleMatch, CircleMatchParams, MarkerBoardCorner, MarkerBoardDetection, MarkerBoardParams,
    MarkerBoardSpec, MarkerCircleSpec,
};

// Re-export the foreign types this crate's public API requires — the corner
// input, the image view, and the alignment result — so depending on
// calib-targets-marker alone is sufficient (no direct calib-targets-chessboard
// / -core dependency needed).
pub use calib_targets_chessboard::ChessCorner;
pub use calib_targets_core::{GrayImageView, GridAlignment, GridTransform};
