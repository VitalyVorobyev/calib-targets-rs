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
//! use calib_targets_charuco::{CharucoBoardSpec, CharucoDetector, CharucoDetectorParams, MarkerLayout};
//! use calib_targets_core::{Corner, GrayImageView};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let board = CharucoBoardSpec {
//!     rows: 5,
//!     cols: 7,
//!     cell_size: 1.0,
//!     marker_size_rel: 0.7,
//!     dictionary: builtins::DICT_4X4_50,
//!     marker_layout: MarkerLayout::OpenCvCharuco,
//! };
//!
//! let params = CharucoDetectorParams::for_board(&board);
//! let detector = CharucoDetector::new(board, params)?;
//!
//! let pixels = vec![0u8; 32 * 32];
//! let view = GrayImageView {
//!     width: 32,
//!     height: 32,
//!     data: &pixels,
//! };
//! let corners: Vec<Corner> = Vec::new();
//!
//! let _ = detector.detect(&view, &corners)?;
//! # Ok(())
//! # }
//! ```

mod alignment;
mod board;
mod detector;
mod io;
mod validation;

pub use alignment::CharucoAlignment;
pub use board::{CharucoBoard, CharucoBoardError, CharucoBoardSpec, MarkerLayout};
pub use detector::{
    CharucoDetectError, CharucoDetectionResult, CharucoDetector, CharucoDetectorParams,
};
pub use io::{CharucoConfigError, CharucoDetectConfig, CharucoDetectReport, CharucoIoError};
pub use validation::{
    validate_marker_corner_links, CharucoMarkerCornerLinks, LinkCheckMode, LinkViolation,
    LinkViolationKind, MarkerCornerLink,
};

pub use calib_targets_core::{GridAlignment, GridTransform, GRID_TRANSFORMS_D4};
