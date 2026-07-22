//! Opt-in diagnostics surface for the marker-board detector.
//!
//! These types carry evidence about *how* a marker board was found — every
//! circle hypothesis the image-space scorer produced, the expected-to-detected
//! circle pairings the matcher chose, the per-corner provenance back into the
//! input ChESS-corner slice, and the count of circles consistent with the
//! chosen grid alignment. They are produced by
//! [`crate::MarkerBoardDetector::detect_with_diagnostics`] and are
//! intentionally kept separate from the result API
//! ([`crate::MarkerBoardDetection`]).
//!
//! A consumer that only needs to *use* a marker-board detection wants the
//! labelled corners ([`crate::MarkerBoardDetection::corners`]) and the
//! grid alignment ([`crate::MarkerBoardDetection::alignment`]) — never
//! the contents of this module. The fields here exist only to *understand* or
//! debug a detection.
//!
//! This module carries a **looser stability promise** than the result API:
//! diagnostic fields may be added or restructured in minor releases as the
//! detector's internal evidence model evolves.

use serde::Serialize;

use crate::circle_score::CircleCandidate;
use crate::types::CircleMatch;

/// Per-call diagnostics captured by
/// [`crate::MarkerBoardDetector::detect_with_diagnostics`].
///
/// Returned alongside the [`crate::MarkerBoardDetection`] on every
/// successful call.
#[non_exhaustive]
#[derive(Clone, Debug, Default, Serialize)]
pub struct MarkerBoardDiagnostics {
    /// Per-corner provenance: for labelled corner `k` in
    /// [`crate::MarkerBoardDetection::corners`], `inliers[k]` is the
    /// index of the source ChESS corner in the detector's input slice.
    pub inliers: Vec<usize>,
    /// Every circle hypothesis scored in image space, before matching.
    pub circle_candidates: Vec<CircleCandidate>,
    /// One entry per expected layout circle, recording the detected
    /// candidate it was paired with (if any) and the offset in cell units.
    pub circle_matches: Vec<CircleMatch>,
    /// Number of circles consistent with the chosen grid alignment.
    /// `0` when no alignment was found.
    pub alignment_inliers: usize,
}
