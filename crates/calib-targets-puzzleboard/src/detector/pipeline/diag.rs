//! Compact component rank and zero-cost diagnostics sink for PuzzleBoard and
//! PuzzlePole detection pipelines.

use crate::code_maps::PuzzleBoardObservedEdge;
use crate::detector::params::PuzzleBoardScoringMode;
#[cfg(feature = "diagnostics")]
use crate::diagnostics::{PuzzleBoardDecodeDiagnostics, PuzzleBoardDiagnostics};

/// Compact per-component rank carried through best-component selection on the
/// hot path without allocating or storing [`PuzzleBoardDiagnostics`].
#[derive(Clone, Copy)]
pub(super) struct ComponentRank {
    pub(super) edges_observed: usize,
    pub(super) edges_matched: usize,
    pub(super) bit_error_rate: f32,
    pub(super) mean_confidence: f32,
    pub(super) score_best: Option<f32>,
    pub(super) score_margin: Option<f32>,
}

/// Scalar winner-vs-runner-up decode summary passed to [`PuzzleDiagSink`] so
/// [`NoPuzzleDiag`] pays zero allocation overhead.
#[cfg(feature = "diagnostics")]
#[derive(Clone, Copy)]
pub(crate) struct DecodeDiagSummary {
    pub score_best: Option<f32>,
    pub score_runner_up: Option<f32>,
    pub score_margin: Option<f32>,
    pub runner_up_origin_row: Option<i32>,
    pub runner_up_origin_col: Option<i32>,
    pub runner_up_transform: Option<calib_targets_core::GridTransform>,
    pub scoring_mode: Option<PuzzleBoardScoringMode>,
}

#[cfg(not(feature = "diagnostics"))]
#[derive(Clone, Copy)]
pub(crate) struct DecodeDiagSummary;

#[cfg(feature = "diagnostics")]
impl DecodeDiagSummary {
    pub(crate) fn from_decode(
        score_best: Option<f32>,
        score_margin: Option<f32>,
        decoded: &crate::detector::decode::DecodeOutcome,
        scoring_mode: PuzzleBoardScoringMode,
    ) -> Self {
        Self {
            score_best,
            score_runner_up: decoded.score_runner_up,
            score_margin,
            runner_up_origin_row: decoded.runner_up_origin_row,
            runner_up_origin_col: decoded.runner_up_origin_col,
            runner_up_transform: decoded.runner_up_transform,
            scoring_mode: Some(scoring_mode),
        }
    }

    pub(crate) fn into_diag(self) -> PuzzleBoardDecodeDiagnostics {
        PuzzleBoardDecodeDiagnostics {
            score_best: self.score_best,
            score_runner_up: self.score_runner_up,
            score_margin: self.score_margin,
            runner_up_origin_row: self.runner_up_origin_row,
            runner_up_origin_col: self.runner_up_origin_col,
            runner_up_transform: self.runner_up_transform,
            scoring_mode: self.scoring_mode,
        }
    }
}

#[cfg(not(feature = "diagnostics"))]
impl DecodeDiagSummary {
    #[inline]
    pub(crate) fn from_decode(
        _score_best: Option<f32>,
        _score_margin: Option<f32>,
        _decoded: &crate::detector::decode::DecodeOutcome,
        _scoring_mode: PuzzleBoardScoringMode,
    ) -> Self {
        Self
    }
}

pub(crate) trait PuzzleDiagSink {
    fn record_failed_edges(&mut self, _observed: &[PuzzleBoardObservedEdge]) {}
    fn record_decoded_component(
        &mut self,
        _is_new_best: bool,
        _observed: &[PuzzleBoardObservedEdge],
        _summary: DecodeDiagSummary,
    ) {
    }
    fn record_conflict_component(
        &mut self,
        _observed: &[PuzzleBoardObservedEdge],
        _summary: DecodeDiagSummary,
    ) {
    }
}

pub(crate) struct NoPuzzleDiag;

impl PuzzleDiagSink for NoPuzzleDiag {}

#[cfg(feature = "diagnostics")]
#[derive(Default)]
pub(crate) struct PuzzleDiagCollector {
    best: Option<PuzzleBoardDiagnostics>,
    conflict: Option<PuzzleBoardDiagnostics>,
    last_failed: PuzzleBoardDiagnostics,
}

#[cfg(feature = "diagnostics")]
impl PuzzleDiagCollector {
    pub(crate) fn into_diagnostics(self) -> PuzzleBoardDiagnostics {
        self.conflict.or(self.best).unwrap_or(self.last_failed)
    }
}

#[cfg(feature = "diagnostics")]
impl PuzzleDiagSink for PuzzleDiagCollector {
    fn record_failed_edges(&mut self, observed: &[PuzzleBoardObservedEdge]) {
        self.last_failed = PuzzleBoardDiagnostics {
            observed_edges: observed.to_vec(),
            decode: PuzzleBoardDecodeDiagnostics::default(),
        };
    }

    fn record_decoded_component(
        &mut self,
        is_new_best: bool,
        observed: &[PuzzleBoardObservedEdge],
        summary: DecodeDiagSummary,
    ) {
        if is_new_best {
            self.best = Some(PuzzleBoardDiagnostics {
                observed_edges: observed.to_vec(),
                decode: summary.into_diag(),
            });
        }
    }

    fn record_conflict_component(
        &mut self,
        observed: &[PuzzleBoardObservedEdge],
        summary: DecodeDiagSummary,
    ) {
        self.conflict = Some(PuzzleBoardDiagnostics {
            observed_edges: observed.to_vec(),
            decode: summary.into_diag(),
        });
    }
}
