//! Opt-in diagnostics surface for the ChArUco detector.
//!
//! These types carry evidence about how a detection was reached — which
//! hypotheses were tried, why components were accepted or rejected, and
//! what the per-cell marker match looked like. They are produced by
//! [`crate::CharucoDetector::diagnose`] /
//! [`crate::CharucoDetector::diagnose_with_corners`] and are intentionally
//! kept separate from the result API ([`crate::CharucoDetector`],
//! [`crate::CharucoDetection`], [`crate::CharucoParams`]).
//!
//! This module carries a **looser stability promise** than the result API:
//! diagnostic fields may be added or restructured in minor releases as the
//! detector's internal evidence model evolves.

use calib_targets_core::Coord;
use serde::Serialize;

// The board-placement hypothesis is always compiled (the production matcher
// selects on it); it is part of this surface through
// [`BoardMatchDiagnostics::chosen`] / [`BoardMatchDiagnostics::runner_up`].
pub use crate::detector::DiagHypothesis;

/// Rich per-frame diagnostics captured by [`crate::CharucoDetector::diagnose`] /
/// [`crate::CharucoDetector::diagnose_with_corners`].
///
/// One entry per chessboard connected component the detector tried to
/// match; fail-early stages (no chessboard) produce an empty list.
#[derive(Clone, Debug, Default, Serialize)]
#[non_exhaustive]
pub struct CharucoDetectDiagnostics {
    /// One entry per chessboard connected component the detector tried
    /// to match.
    pub components: Vec<ComponentDiagnostics>,
    /// Total number of markers emitted by the board-level matcher across the
    /// components that contributed to the returned [`crate::CharucoDetection`].
    pub raw_marker_count: usize,
    /// Emitted markers whose ID disagreed with the marker expected by the
    /// winning board alignment at that cell — a self-consistency check over
    /// the emitted markers.
    pub raw_marker_wrong_id_count: usize,
}

/// Per-component diagnostics for one chessboard connected component.
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct ComponentDiagnostics {
    /// Zero-based index of this component.
    pub index: usize,
    /// Number of chessboard corners in this component.
    pub chess_corner_count: usize,
    /// Number of candidate marker cells extracted from this component.
    pub candidate_cell_count: usize,
    /// Board-level matcher diagnostics (per-cell scores, chosen/runner-up
    /// hypotheses, margin, rejection reason).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub board: Option<BoardMatchDiagnostics>,
    /// Final detection outcome for this component.
    pub outcome: ComponentOutcome,
}

/// Final outcome of detecting one chessboard component.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ComponentOutcome {
    /// The component yielded a detection.
    Ok {
        /// Number of markers in the component's detection.
        markers: usize,
        /// Number of ChArUco corners in the component's detection.
        charuco_corners: usize,
        /// Number of markers emitted for this component.
        raw_marker_count: usize,
        /// Emitted markers for this component whose ID disagreed with the
        /// chosen alignment.
        raw_marker_wrong_id_count: usize,
    },
    /// The component produced no detection.
    Failed {
        /// Human-readable reason the component failed.
        reason: String,
    },
}

/// Structured diagnostics produced by the board-level matcher: per-cell and
/// per-hypothesis matcher evidence for debugging and overlay rendering.
#[derive(Clone, Debug, Serialize, Default)]
#[non_exhaustive]
pub struct BoardMatchDiagnostics {
    /// Per-cell diagnostic record for every cell the matcher considered.
    pub cells: Vec<CellDiag>,
    /// The board-placement hypothesis the matcher chose; `None` when the
    /// match was rejected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chosen: Option<DiagHypothesis>,
    /// The second-best hypothesis, for margin inspection; `None` when
    /// fewer than two hypotheses were scored.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner_up: Option<DiagHypothesis>,
    /// Score margin between the chosen and runner-up hypotheses.
    pub margin: f32,
    /// Total number of board-placement hypotheses scored.
    pub total_hypotheses: usize,
    /// Reason the match was rejected; `None` when the match succeeded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejection: Option<RejectReason>,
    /// Board width in squares.
    pub board_cols: u32,
    /// Board height in squares.
    pub board_rows: u32,
    /// Number of marker bits per side (the marker's grid resolution).
    pub bits_per_side: usize,
}

/// Per-cell diagnostic record produced by the board matcher.
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct CellDiag {
    /// The cell's `(i, j)` grid coordinate.
    pub gc: Coord,
    /// The cell's four corners `[TL, TR, BR, BL]` in image pixels.
    pub corners_img: [[f32; 2]; 4],
    /// `true` when the cell was sampled (it had all four corners).
    pub sampled: bool,
    /// Otsu binarization threshold computed for this cell.
    pub otsu: u8,
    /// Fraction of the cell border that read as black.
    pub border_black: f32,
    /// Weight this cell contributed to hypothesis scoring.
    pub weight: f32,
    /// The board-cell `[col, row]` this cell mapped to under the chosen
    /// hypothesis; `None` when unmapped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mapped_bc: Option<[i32; 2]>,
    /// The marker ID expected at the mapped board cell; `None` when the
    /// cell carries no marker.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_id: Option<u32>,
    /// Score of the observed bits against the expected marker.
    pub expected_score: f32,
    /// The best-matching marker for this cell over all IDs and rotations;
    /// `None` when no marker scored.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best: Option<CellBestMatch>,
    /// Per-bit log-likelihoods of the observed bits under the expected
    /// marker; empty when not computed.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub expected_bit_ll: Vec<f32>,
    /// Mean interior-bit intensities sampled from the cell.
    pub interior_means: Vec<u8>,
}

/// The best marker match found for a single cell.
#[derive(Clone, Copy, Debug, Serialize)]
#[non_exhaustive]
pub struct CellBestMatch {
    /// Dictionary ID of the best-matching marker.
    pub marker_id: u32,
    /// Rotation, in 90° steps, that produced the best match.
    pub rotation: u8,
    /// Match score (higher is better).
    pub score: f32,
}

/// Reason the board matcher rejected a frame.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum RejectReason {
    /// No grid cells were available to sample.
    NoCells,
    /// The board specification has zero squares.
    EmptyBoard,
    /// No valid board translation could be enumerated.
    TranslationWindowEmpty,
    /// The chosen hypothesis's evidence did not exceed the chance level: the
    /// evidence the best of the competing hypotheses reaches on cells that say
    /// nothing about the board (`ln` of their number).
    EvidenceAtChance {
        /// The chosen hypothesis's evidence (nats).
        evidence: f32,
        /// The chance level it had to exceed (nats).
        chance_level: f32,
    },
    /// The chosen-vs-runner-up margin fell below the acceptance gate.
    MarginBelowGate {
        /// The observed score margin.
        margin: f32,
        /// The minimum margin required to accept.
        required: f32,
    },
    /// The match produced no markers to emit.
    NoEmittedMarkers,
}
