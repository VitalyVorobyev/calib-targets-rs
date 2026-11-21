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

// The diagnostics types live in the feature-gated `crate::diagnostics`
// module, consistent with `calib-targets-chessboard`. The feature gates
// *computation* too: the diagnostic matcher (`match_board_diag`) and the
// diagnostics-collecting pipeline path are compiled only when the feature is
// on, so the production `detect` path allocates zero diagnostics.
// `DiagHypothesis` is the one exception — it is the core hypothesis-selection
// result the production matcher needs, so it is always compiled here and only
// its public re-export (from `crate::diagnostics`) is gated.
#[cfg(feature = "diagnostics")]
pub use board_match::DiagHypothesis;
pub use error::CharucoDetectError;
pub use params::{CharucoAdvancedTuning, CharucoParams};
pub use pipeline::CharucoDetector;
pub use result::{CharucoCorner, CharucoDetection};
