//! ChArUco-related utilities.
//!
//! Current focus:
//! - chessboard detection from ChESS corners,
//! - per-cell marker decoding (no full-image warp by default),
//! - alignment to a known board definition and corner ID assignment.
//!
//! Marker dictionaries and decoding live in `calib-targets-aruco`.

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
