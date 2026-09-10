//! Detector parameters for a PuzzlePole.
//!
//! Mostly the planar knobs — the pipeline is the planar pipeline — with one
//! genuine difference. A planar fragment is gated by a single `min_window`
//! applied to both axes, because on a flat board both axes carry the same
//! 167-long code. A pole's two axes do not: the circumference wraps at `p`
//! while the axial axis runs the master's 501. So the window floor is a pair,
//! and the pair is measured rather than chosen.

use super::PuzzlePoleSpec;
use crate::detector::{PuzzleBoardAdvancedTuning, PuzzleBoardScoringMode, PuzzleBoardSymmetryMode};
use calib_targets_chessboard::ChessboardParams;
use calib_targets_core::{default_chess_config, DetectorConfig};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// Corner rows around the circumference a fragment must span.
///
/// Measured exhaustively over every supported `(period, start row)` and every
/// placement — see `research/puzzleboard-rings/report/puzzlepole-floor.md`.
/// Under the rotations-only search a `5 × 8` corner window is unambiguous on
/// every supported pole at any length, and one corner less is not.
///
/// This is the output of a measurement, not a tuned value: raising it costs
/// recall for nothing, and lowering it admits placements that provably alias.
pub const MIN_CIRCUMFERENCE_SPAN: u32 = 5;

/// Corner columns along the axis a fragment must span.
///
/// The other half of the measured floor. Note it cannot go below **5** on any
/// pole for a structural reason rather than a statistical one: the interior
/// readout of a 4-corner axial extent yields two columns of `map_a`, and a
/// 3 × 2 block of a sub-perfect map is not unique — sub-perfection is a 3 × 3
/// property — so the axial position is never pinned, at any circumference span.
pub const MIN_AXIAL_SPAN: u32 = 8;

/// Configuration for the PuzzlePole detector.
#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PuzzlePoleParams {
    /// ChESS corner front-end configuration, as for a planar board.
    #[serde(default = "default_chess_config")]
    pub chess: DetectorConfig,
    /// Chessboard grid-assembly parameters.
    #[serde(default)]
    pub chessboard: ChessboardParams,
    /// The pole being looked for.
    pub pole: PuzzlePoleSpec,
    /// Decoding knobs.
    #[serde(default)]
    pub decode: PuzzlePoleDecodeConfig,
}

impl PuzzlePoleParams {
    /// Reasonable defaults for a given pole.
    #[must_use]
    pub fn for_pole(pole: PuzzlePoleSpec) -> Self {
        let mut chessboard = ChessboardParams::default();
        // Same floor the planar PuzzleBoard detector uses, and for the same
        // reason: a weakly-firing corner is grid-consistent in position but
        // pollutes the frontier with false labels, and the decoder tolerates a
        // missing corner far better than a wrong one.
        chessboard.min_corner_strength = 33.0;
        Self {
            chess: default_chess_config(),
            chessboard,
            pole,
            decode: PuzzlePoleDecodeConfig::default(),
        }
    }
}

impl PuzzlePoleParams {
    /// Multi-config sweep preset, for `detect_puzzlepole_best`.
    ///
    /// The same shape as the planar sweep: the chessboard front-end sweep
    /// crossed with two scoring settings — the default soft scorer at the
    /// default bit-error budget, and the legacy hard-weighted scorer at the
    /// paper's 40 % allowance, which recovers heavily foreshortened fragments
    /// the soft path's margin gate declines.
    ///
    /// A pole makes the second pass matter more than a flat board does: near
    /// the limb every dot is compressed, so a view that reads cleanly across
    /// the middle of the sector can still carry a low-confidence tail.
    #[must_use]
    pub fn sweep_for_pole(pole: &PuzzlePoleSpec) -> Vec<Self> {
        let base = Self::for_pole(*pole);
        let sweep = ChessboardParams::sweep_default();
        let with_chessboard = |mut chessboard: ChessboardParams| {
            chessboard.min_corner_strength = base.chessboard.min_corner_strength;
            Self {
                chessboard,
                ..base.clone()
            }
        };
        let mut configs: Vec<Self> = sweep.iter().cloned().map(with_chessboard).collect();
        configs.extend(sweep.into_iter().map(|chessboard| {
            let mut params = with_chessboard(chessboard);
            params.decode.scoring_mode = PuzzleBoardScoringMode::HardWeighted;
            params.decode.max_bit_error_rate = 0.40;
            params
        }));
        configs
    }
}

/// Decode knobs for a PuzzlePole.
///
/// Deliberately not a reuse of `PuzzleBoardDecodeConfig`. Two of its fields
/// would be meaningless here — `min_window` is a single number where a pole
/// needs a pair, and `search_mode` has nothing to choose, because a pole's
/// origin space is always a restricted rectangle and never the full master.
/// Sharing the type would have meant documenting two fields as "ignored", and
/// a field that is silently ignored is worse than one that does not exist.
#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PuzzlePoleDecodeConfig {
    /// Corner rows around the circumference a fragment must span.
    #[serde(default = "default_min_circumference_span")]
    pub min_circumference_span: u32,
    /// Corner columns along the axis a fragment must span.
    #[serde(default = "default_min_axial_span")]
    pub min_axial_span: u32,
    /// Minimum per-bit confidence for a sampled dot to count.
    #[serde(default = "default_min_bit_confidence")]
    pub min_bit_confidence: f32,
    /// Maximum tolerated logical bit-error rate.
    #[serde(default = "default_max_bit_error_rate")]
    pub max_bit_error_rate: f32,
    /// Try every detected grid component, not just the largest.
    #[serde(default = "default_search_all_components")]
    pub search_all_components: bool,
    /// Dot sampling radius, as a fraction of the edge length.
    #[serde(default = "default_sample_radius_rel")]
    pub sample_radius_rel: f32,
    /// Hard-weighted or soft log-likelihood scoring.
    #[serde(default)]
    pub scoring_mode: PuzzleBoardScoringMode,
    /// Which orientations to search.
    ///
    /// Rotations only, as for a planar board: a camera sees the outside of an
    /// opaque cylinder, so it cannot observe a mirrored pattern. The four
    /// rotations are all reachable — a pole can be photographed lying on its
    /// side — and are what test the detected grid's axes against the pole's.
    #[serde(default)]
    pub symmetry_mode: PuzzleBoardSymmetryMode,
    /// Rarely-touched scoring internals.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub advanced: Option<Box<PuzzleBoardAdvancedTuning>>,
}

impl PuzzlePoleDecodeConfig {
    /// The advanced tuning in force, defaulted if unset.
    #[must_use]
    pub fn effective_tuning(&self) -> Cow<'_, PuzzleBoardAdvancedTuning> {
        match &self.advanced {
            Some(tuning) => Cow::Borrowed(tuning),
            None => Cow::Owned(PuzzleBoardAdvancedTuning::default()),
        }
    }
}

impl Default for PuzzlePoleDecodeConfig {
    fn default() -> Self {
        Self {
            min_circumference_span: MIN_CIRCUMFERENCE_SPAN,
            min_axial_span: MIN_AXIAL_SPAN,
            min_bit_confidence: default_min_bit_confidence(),
            max_bit_error_rate: default_max_bit_error_rate(),
            search_all_components: default_search_all_components(),
            sample_radius_rel: default_sample_radius_rel(),
            scoring_mode: PuzzleBoardScoringMode::default(),
            symmetry_mode: PuzzleBoardSymmetryMode::default(),
            advanced: None,
        }
    }
}

const fn default_min_circumference_span() -> u32 {
    MIN_CIRCUMFERENCE_SPAN
}
const fn default_min_axial_span() -> u32 {
    MIN_AXIAL_SPAN
}
const fn default_min_bit_confidence() -> f32 {
    0.15
}
const fn default_max_bit_error_rate() -> f32 {
    0.30
}
const fn default_search_all_components() -> bool {
    true
}
fn default_sample_radius_rel() -> f32 {
    1.0 / 6.0
}

/// A pole shorter than this can never be decoded whatever the circumference
/// span, so the spec's own minimum must not sit below the structural floor:
/// four axial corners give two columns of `map_a`, and a 3 × 2 block of a
/// sub-perfect map is not unique. Checked at compile time because both sides
/// are constants and a runtime assertion would be theatre.
const _: () = assert!(super::MIN_AXIAL_SQUARES + 1 >= 5);

/// The soft-log-likelihood knobs, in the form the decoder takes them.
pub(crate) fn soft_cfg_from(cfg: &PuzzlePoleDecodeConfig) -> crate::detector::decode::SoftLlConfig {
    let tuning = cfg.effective_tuning();
    crate::detector::decode::SoftLlConfig {
        kappa: tuning.bit_likelihood_slope,
        per_bit_floor: tuning.per_bit_floor,
        alignment_min_margin: tuning.alignment_min_margin,
    }
}

/// Interior edge dots a `span_a × span_b` corner window offers.
///
/// The planar `required_edges` assumes a square window because a flat board's
/// two axes carry the same code. A pole's do not, so the floor is a rectangle
/// and the count has to be one too. Reduces to `2(s-1)(s-2)` when `a == b == s`,
/// which is the planar formula.
#[must_use]
pub(crate) fn required_edges_rect(span_a: u32, span_b: u32) -> usize {
    let (a, b) = (span_a.max(2) as usize, span_b.max(2) as usize);
    (a - 2) * (b - 1) + (a - 1) * (b - 2)
}

/// Distinct code bits a `span_a × span_b` corner window can resolve.
///
/// Both maps repeat every three rows or columns on their short axis, so a
/// window's dots are not independent bits: the count is `3` per distinct long
/// class each family reads. Reduces to the planar `6(s-2)` when `a == b == s`.
#[must_use]
pub(crate) fn required_logical_bits_rect(span_a: u32, span_b: u32) -> usize {
    let (a, b) = (span_a.max(2) as usize, span_b.max(2) as usize);
    3 * (a - 2) + 3 * (b - 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_measured_floor() {
        let cfg = PuzzlePoleDecodeConfig::default();
        assert_eq!(cfg.min_circumference_span, 5);
        assert_eq!(cfg.min_axial_span, 8);
    }

    #[test]
    fn the_rectangular_counts_reduce_to_the_planar_ones() {
        for span in 4..=10u32 {
            assert_eq!(
                required_edges_rect(span, span),
                crate::detector::params::required_edges(span),
                "edge count disagrees at span {span}"
            );
            assert_eq!(
                required_logical_bits_rect(span, span),
                crate::detector::params::required_logical_bits(span),
                "logical bit count disagrees at span {span}"
            );
        }
    }

    #[test]
    fn the_measured_floor_offers_enough_to_decode() {
        // 5 x 8 corners: 45 interior dots carrying 27 distinct bits.
        assert_eq!(required_edges_rect(5, 8), 45);
        assert_eq!(required_logical_bits_rect(5, 8), 27);
    }

    #[test]
    fn the_sweep_covers_both_scorers() {
        let pole = PuzzlePoleSpec::new(24, 12, 20.0).expect("period 24 is supported");
        let configs = PuzzlePoleParams::sweep_for_pole(&pole);
        assert!(configs.len() >= 2);
        assert!(configs.iter().any(|c| matches!(
            c.decode.scoring_mode,
            PuzzleBoardScoringMode::SoftLogLikelihood
        )));
        assert!(configs
            .iter()
            .any(|c| matches!(c.decode.scoring_mode, PuzzleBoardScoringMode::HardWeighted)));
        assert!(configs.iter().all(|c| c.pole == pole));
    }

    #[test]
    fn params_round_trip_through_serde() {
        let pole = PuzzlePoleSpec::new(18, 8, 13.0).expect("period 18 is supported");
        let params = PuzzlePoleParams::for_pole(pole);
        let json = serde_json::to_string(&params).expect("serialize");
        let back: PuzzlePoleParams = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.pole, params.pole);
        assert_eq!(
            back.decode.min_circumference_span,
            params.decode.min_circumference_span
        );
    }

    /// A config that omits every optional field must still deserialise to the
    /// measured floor — the defaults are the contract, not the struct literal.
    #[test]
    fn a_minimal_config_defaults_to_the_measured_floor() {
        let json = r#"{"pole":{"circumference_squares":18,"start_row":7,
            "axial_start_col":0,"axial_squares":8,"cell_size_mm":13.0}}"#;
        let params: PuzzlePoleParams = serde_json::from_str(json).expect("deserialize");
        assert_eq!(params.decode.min_circumference_span, MIN_CIRCUMFERENCE_SPAN);
        assert_eq!(params.decode.min_axial_span, MIN_AXIAL_SPAN);
    }
}
