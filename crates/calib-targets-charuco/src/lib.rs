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
//! use calib_targets_charuco::{
//!     builtins, CharucoBoardSpec, CharucoDetector, CharucoParams, ChessCorner,
//!     GrayImageView, MarkerLayout,
//! };
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

// Opt-in marker↔corner linkage validation, gated behind the `link-check`
// feature (default off). It has no in-tree consumers, but is intentional,
// documented validation logic, so it is preserved behind a gate rather than
// removed — mirroring the `diagnostics` feature pattern.
#[cfg(feature = "link-check")]
pub mod link_check;

// Opt-in introspection surface, gated behind the `diagnostics` feature (default
// off), consistent with `calib-targets-chessboard`.
#[cfg(feature = "diagnostics")]
pub mod diagnostics;

pub use board::{CharucoBoard, CharucoBoardError, CharucoBoardSpec, MarkerLayout};
pub use detector::{
    CharucoAdvancedTuning, CharucoCorner, CharucoDetectError, CharucoDetectionResult,
    CharucoDetector, CharucoParams,
};
pub use io::{
    load_board_spec_any, resolve_dictionary, BoardSpecLoadError, CharucoConfigError,
    CharucoDetectConfig, CharucoDetectReport, CharucoIoError,
};
#[cfg(feature = "link-check")]
pub use link_check::{
    validate_marker_corner_links, CharucoMarkerCornerLinks, LinkCheckMode, LinkViolation,
    LinkViolationKind, MarkerCornerLink,
};

// A consumer of this crate alone must be able to name every foreign type our
// public API requires — the marker dictionary, the image view, and the corner
// input — without depending on calib-targets-aruco / -core / -chessboard
// directly.
pub use calib_targets_aruco::{builtins, Dictionary};
pub use calib_targets_chessboard::ChessCorner;
pub use calib_targets_core::{GrayImageView, GridAlignment, GridTransform};
