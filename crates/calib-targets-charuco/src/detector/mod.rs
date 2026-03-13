//! ChArUco detection pipeline.
//!
//! This module wires together chessboard detection, per-cell marker decoding,
//! alignment to a known board definition, and ChArUco corner ID assignment.

mod alignment_select;
mod candidate_eval;
mod corner_mapping;
mod corner_validation;
mod error;
mod marker_decode;
mod marker_sampling;
mod params;
mod patch_placement;
mod pipeline;
mod rectified_recovery;
mod result;

pub use corner_validation::{CornerValidationDiagnostics, CornerValidationSkippedReason};
pub use error::CharucoDetectError;
pub use params::{CharucoAugmentationParams, CharucoDetectorParams};
pub use pipeline::CharucoDetector;
pub use result::{
    CharucoDetectionResult, CharucoDetectionRun, CharucoDiagnostics, CharucoStageTimings,
    MarkerHammingSummary, MarkerPathDiagnostics, MarkerPathSourceDiagnostics, MarkerScoreSummary,
    PatchPlacementCandidateDiagnostics, PatchPlacementDiagnostics, PatchPlacementSourceDiagnostics,
};
