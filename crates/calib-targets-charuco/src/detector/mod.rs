//! ChArUco detection pipeline.
//!
//! This module wires together chessboard detection, per-cell marker decoding,
//! alignment to a known board definition, and ChArUco corner ID assignment.

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
// feature (default off), consistent with `calib-targets-chessboard`. The
// feature now gates *computation* too: the diagnostic matcher
// (`match_board_diag`) and the diagnostics-collecting pipeline path are
// compiled only when the feature is on, so the production `detect` path
// allocates zero diagnostics. `DiagHypothesis` is the one exception — it is
// the core hypothesis-selection result the production matcher needs, so it is
// always compiled and only its re-export here is gated.
#[cfg(feature = "diagnostics")]
pub use board_match::{
    BoardMatchDiagnostics, CellBestMatch, CellDiag, DiagHypothesis, RejectReason,
};
pub use error::CharucoDetectError;
pub use params::{CharucoAdvancedTuning, CharucoParams};
pub use pipeline::CharucoDetector;
#[cfg(feature = "diagnostics")]
pub use pipeline::{CharucoDetectDiagnostics, ComponentDiagnostics, ComponentOutcome};
pub use result::{CharucoCorner, CharucoDetectionResult};
