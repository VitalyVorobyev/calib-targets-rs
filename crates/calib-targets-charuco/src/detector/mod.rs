//! ChArUco detection pipeline.
//!
//! This module wires together chessboard detection, per-cell marker decoding,
//! alignment to a known board definition, and ChArUco corner ID assignment.

mod alignment_select;
mod board_match;
mod corner_mapping;
mod corner_refit;
mod error;
mod grid_smoothness;
mod marker_sampling;
mod merge;
mod params;
mod pipeline;
mod result;

// Diagnostics types reach the public surface only behind the `diagnostics`
// feature (default off), consistent with `calib-targets-chessboard`. They are
// always compiled — the matcher/pipeline capture them internally — but are
// re-exported (and namable in public signatures) only when the feature is on.
#[cfg(feature = "diagnostics")]
pub use board_match::{
    BoardMatchDiagnostics, CellBestMatch, CellDiag, DiagHypothesis, RejectReason,
};
pub use error::CharucoDetectError;
pub use params::CharucoParams;
pub use pipeline::CharucoDetector;
#[cfg(feature = "diagnostics")]
pub use pipeline::{
    CharucoDetectDiagnostics, ComponentDiagnostics, ComponentOutcome, MatcherDiagKind,
};
pub use result::{CharucoCorner, CharucoDetectionResult};
