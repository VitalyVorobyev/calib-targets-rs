//! ChArUco-related utilities.
//!
//! Current focus:
//! - chessboard detection from ChESS corners,
//! - per-cell marker decoding (no full-image warp by default),
//! - alignment to a known board definition and corner ID assignment.
//!
//! Marker dictionaries and decoding live in `calib-targets-aruco`.
//!
//! ## Quickstart
//!
//! ```no_run
//! use calib_targets_aruco::builtins;
//! use calib_targets_charuco::{CharucoBoardSpec, CharucoDetector, CharucoParams, MarkerLayout};
//! use calib_targets_chessboard::ChessCorner;
//! use calib_targets_core::GrayImageView;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let board = CharucoBoardSpec::new(5, 7, 1.0, 0.7, builtins::DICT_4X4_50)
//!     .with_marker_layout(MarkerLayout::OpenCvCharuco);
//!
//! let params = CharucoParams::for_board(&board);
//! let detector = CharucoDetector::new(params)?;
//!
//! let pixels = vec![0u8; 32 * 32];
//! let view = GrayImageView {
//!     width: 32,
//!     height: 32,
//!     data: &pixels,
//! };
//! let corners: Vec<ChessCorner> = Vec::new();
//!
//! let _ = detector.detect(&view, &corners)?;
//! # Ok(())
//! # }
//! ```
#![deny(missing_docs)]

mod alignment;
mod board;
mod detector;
mod io;
mod validation;

pub mod diagnostics;

pub use alignment::CharucoAlignment;
pub use board::{CharucoBoard, CharucoBoardError, CharucoBoardSpec, MarkerLayout};
pub use detector::{
    CharucoCorner, CharucoDetectError, CharucoDetectionResult, CharucoDetector, CharucoParams,
};
pub use io::{
    load_board_spec_any, resolve_dictionary, BoardSpecLoadError, CharucoConfigError,
    CharucoDetectConfig, CharucoDetectReport, CharucoIoError,
};
pub use validation::{
    validate_marker_corner_links, CharucoMarkerCornerLinks, LinkCheckMode, LinkViolation,
    LinkViolationKind, MarkerCornerLink,
};

pub use calib_targets_core::{GridAlignment, GridTransform, GRID_TRANSFORMS_D4};
