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

pub use alignment::{CharucoAlignment, GridTransform};
pub use board::{CharucoBoard, CharucoBoardError, CharucoBoardSpec, MarkerLayout};
pub use detector::{
    CharucoDetectError, CharucoDetectionResult, CharucoDetector, CharucoDetectorParams,
};
pub use io::{
    CharucoConfigError, CharucoDetectConfig, CharucoDetectReport, CharucoIoError,
    RectifiedImageInfo,
};
