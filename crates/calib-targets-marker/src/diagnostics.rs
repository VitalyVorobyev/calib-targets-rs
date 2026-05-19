//! Opt-in diagnostics surface for the marker-board detector.
//!
//! These types carry evidence about *how* a marker board was found — every
//! circle hypothesis the image-space scorer produced, the expected-to-detected
//! circle pairings the matcher chose, the per-corner provenance back into the
//! input ChESS-corner slice, and the count of circles consistent with the
//! chosen grid alignment. They are produced by
//! [`crate::MarkerBoardDetector::detect_from_corners_with_diagnostics`] and
//! [`crate::MarkerBoardDetector::detect_from_image_and_corners_with_diagnostics`]
//! and are intentionally kept separate from the result API
//! ([`crate::MarkerBoardDetectionResult`]).
//!
//! A consumer that only needs to *use* a marker-board detection wants the
//! labelled corners ([`crate::MarkerBoardDetectionResult::corners`]) and the
//! grid alignment ([`crate::MarkerBoardDetectionResult::alignment`]) — never
//! the contents of this module. The fields here exist only to *understand* or
//! debug a detection.
//!
//! This module carries a **looser stability promise** than the result API:
//! diagnostic fields may be added or restructured in minor releases as the
//! detector's internal evidence model evolves.

use serde::Serialize;

use crate::circle_score::CircleCandidate;
use crate::types::CircleMatch;

/// Per-call diagnostics captured by the marker-board detector's
/// `*_with_diagnostics` entry points.
///
/// Returned alongside the [`crate::MarkerBoardDetectionResult`] on every
/// successful call. On the corners-only path
/// ([`crate::MarkerBoardDetector::detect_from_corners_with_diagnostics`])
/// there is no image to score circles against, so [`Self::circle_candidates`]
/// and [`Self::circle_matches`] are empty and [`Self::alignment_inliers`] is
/// `0`; [`Self::inliers`] is still populated from the chessboard stage.
#[non_exhaustive]
#[derive(Clone, Debug, Default, Serialize)]
pub struct MarkerBoardDiagnostics {
    /// Per-corner provenance: for labelled corner `k` in
    /// [`crate::MarkerBoardDetectionResult::corners`], `inliers[k]` is the
    /// index of the source ChESS corner in the detector's input slice.
    pub inliers: Vec<usize>,
    /// Every circle hypothesis scored in image space, before matching.
    /// Empty on the corners-only detection path.
    pub circle_candidates: Vec<CircleCandidate>,
    /// One entry per expected layout circle, recording the detected
    /// candidate it was paired with (if any) and the offset in cell units.
    /// Empty on the corners-only detection path.
    pub circle_matches: Vec<CircleMatch>,
    /// Number of circles consistent with the chosen grid alignment.
    /// `0` when no alignment was found or on the corners-only path.
    pub alignment_inliers: usize,
}
