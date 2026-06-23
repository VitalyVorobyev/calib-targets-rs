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
pub mod link_check;

// Opt-in introspection surface, gated behind the `diagnostics` feature (default
// off), consistent with `calib-targets-chessboard`.
#[cfg(feature = "diagnostics")]
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
pub use link_check::{
    validate_marker_corner_links, CharucoMarkerCornerLinks, LinkCheckMode, LinkViolation,
    LinkViolationKind, MarkerCornerLink,
};

/// Deprecated alias for the [`link_check`] module.
///
/// The marker-corner linkage check module was renamed from `validation` to
/// `link_check` to reflect its role (it checks marker↔corner *links*, distinct
/// from the in-`detector` homography corner *refit*). The old
/// `calib_targets_charuco::validation::*` path still resolves through this
/// re-export; migrate to `calib_targets_charuco::link_check` (or the
/// crate-root re-exports, which are unchanged).
#[deprecated(since = "0.10.0", note = "renamed to `link_check`")]
pub use crate::link_check as validation;

pub use calib_targets_core::{GridAlignment, GridTransform, GRID_TRANSFORMS_D4};
