//! ChArUco detection pipeline.
//!
//! This module wires together chessboard detection, per-cell marker decoding,
//! alignment to a known board definition, and ChArUco corner ID assignment.

mod alignment_select;
mod corner_mapping;
mod error;
mod marker_sampling;
mod params;
mod pipeline;
mod result;

pub use error::CharucoDetectError;
pub use params::CharucoDetectorParams;
pub use pipeline::CharucoDetector;
pub use result::CharucoDetectionResult;
