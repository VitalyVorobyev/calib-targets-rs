//! ChArUco detection pipeline.
//!
//! This module wires together chessboard detection, per-cell marker decoding,
//! alignment to a known board definition, and ChArUco corner ID assignment.

mod alignment_select;
mod board_match;
mod corner_mapping;
mod corner_validation;
mod error;
mod grid_smoothness;
mod marker_sampling;
mod merge;
mod params;
mod pipeline;
mod result;

pub use board_match::{
    BoardMatchDiagnostics, CellBestMatch, CellDiag, DiagHypothesis, RejectReason,
};
pub use error::CharucoDetectError;
pub use params::CharucoParams;
pub use pipeline::{
    CharucoDetectDiagnostics, CharucoDetector, ComponentDiagnostics, ComponentOutcome,
    MatcherDiagKind,
};
pub use result::CharucoDetectionResult;
