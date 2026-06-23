//! Opt-in diagnostics surface for the ChArUco detector.
//!
//! These types carry evidence about how a detection was reached — which
//! hypotheses were tried, why components were accepted or rejected, and
//! what the per-cell marker match looked like. They are produced by
//! [`crate::CharucoDetector::detect_with_diagnostics`] and are intentionally
//! kept separate from the result API ([`crate::CharucoDetector`],
//! [`crate::CharucoDetectionResult`], [`crate::CharucoParams`]).
//!
//! This module carries a **looser stability promise** than the result API:
//! diagnostic fields may be added or restructured in minor releases as the
//! detector's internal evidence model evolves.

pub use crate::detector::{
    BoardMatchDiagnostics, CellBestMatch, CellDiag, CharucoDetectDiagnostics, ComponentDiagnostics,
    ComponentOutcome, DiagHypothesis, RejectReason,
};
