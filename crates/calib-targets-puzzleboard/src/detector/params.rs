//! Knobs for the decoding stage and associated validation helpers.

use serde::{Deserialize, Serialize};
use std::borrow::Cow;

use crate::detector::error::PuzzleBoardDetectError;

/// Strategy for recovering the master-map origin during decode.
///
/// - [`PuzzleBoardSearchMode::Full`] scans all `501 × 501 × 8` `(D4, origin)`
///   candidates against the full 501 × 501 master code. Works whether or not
///   the caller knows which printed board produced the image.
/// - [`PuzzleBoardSearchMode::FixedBoard`] matches observations directly
///   against the *declared* board's bit pattern (read from
///   [`crate::board::PuzzleBoardSpec`] at decode time). Any partial view of
///   that specific board decodes to the same absolute master IDs — useful
///   whenever the caller already knows which board they printed, whether
///   that's one camera seeing a fragment of a large board or several
///   cameras each seeing a different fragment.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PuzzleBoardSearchMode {
    /// Scan every `(D4, master_row, master_col)` in the 501 × 501 master.
    #[default]
    Full,
    /// Match observations against the declared board's own bit pattern
    /// (read from `PuzzleBoardParams.board` at decode time).
    ///
    /// Bounded search space `8 × (rows+1)²` — cheaper than
    /// [`PuzzleBoardSearchMode::Full`] for small boards and fast enough for
    /// large ones (50 × 50 native under 10 ms at typical edge counts).
    ///
    /// Partial-view guarantee: any subset of the printed board decodes to
    /// the same master IDs a full-view decode would produce, so subsets
    /// across frames or cameras stitch cleanly.
    FixedBoard,
}

/// Scoring function used when ranking candidate `(D4, origin)` hypotheses.
///
/// - [`PuzzleBoardScoringMode::HardWeighted`] (legacy): rank by
///   `edges_matched` (hard bit-match count) with confidence-weighted sum as
///   the tie-break. No margin gate; the highest-match-count hypothesis always
///   wins.
/// - [`PuzzleBoardScoringMode::SoftLogLikelihood`] (default): rank by a
///   summed per-bit `log_sigmoid` of a linear logit proportional to the
///   per-bit confidence. Rejects the winner if it does not clear a
///   best-vs-runner-up margin gate. Mirrors the ChArUco board-level
///   matcher in `calib-targets-charuco/src/detector/board_match.rs`.
///
/// Soft scoring is more robust to real-world bit noise and small observation
/// windows; in particular, on multi-camera captures of the same physical
/// board it produces per-camera decodes that agree on the same `(D4, origin)`
/// far more consistently than hard-weighted scoring.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PuzzleBoardScoringMode {
    /// Hard bit-match count with confidence-weighted tie-break. Kept for
    /// one or two releases so callers can opt out while the soft scorer is
    /// evaluated on new datasets.
    HardWeighted,
    /// Soft per-bit log-likelihood with margin gate. Default.
    #[default]
    SoftLogLikelihood,
}

/// Tuning parameters for the decoding stage.
#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PuzzleBoardDecodeConfig {
    /// Minimum window size (in squares) required to attempt a decode.
    ///
    /// The paper's "4×4 = 100 % unique" holds only for a *noise-free* read: the
    /// master edge code has minimum Hamming distance `1` at a 4×4 window (zero
    /// error-correction), so a single corrupted bit can turn a corrupted 4×4
    /// fragment into a perfect read of a *different* master location — a false
    /// positive. The code's minimum distance grows with window size; an empirical
    /// 300k-trial sweep (random origins × error patterns up to the BER budget)
    /// finds the smallest window with zero false-accepts under the uniqueness
    /// gate to be **7×7 (84 interior edges)** at both 30 % and 40 % BER. The
    /// default is therefore 7 (bounded-distance decoding); smaller boards become
    /// correct detection *misses* rather than risk a wrong absolute label.
    #[serde(default = "default_min_window")]
    pub min_window: u32,
    /// Per-bit confidence floor — bits below this are treated as unknown.
    #[serde(default = "default_min_bit_confidence")]
    pub min_bit_confidence: f32,
    /// Maximum fraction of bits allowed to be wrong after majority voting.
    ///
    /// The paper allows up to 40 % (401 / 1002). Default is 0.3.
    #[serde(default = "default_max_bit_error_rate")]
    pub max_bit_error_rate: f32,
    /// If true, attempt to decode each connected component independently.
    #[serde(default = "default_search_all_components")]
    pub search_all_components: bool,
    /// Sample radius for edge-midpoint disk (fraction of the edge length).
    #[serde(default = "default_sample_radius_rel")]
    pub sample_radius_rel: f32,
    /// Master-origin search strategy. Defaults to
    /// [`PuzzleBoardSearchMode::Full`]; set to
    /// [`PuzzleBoardSearchMode::FixedBoard`] when the physical board's
    /// own bit pattern is known and you only need to recover its pose
    /// within the master grid.
    #[serde(default)]
    pub search_mode: PuzzleBoardSearchMode,
    /// Scoring function used when ranking candidate hypotheses. Defaults
    /// to [`PuzzleBoardScoringMode::SoftLogLikelihood`].
    #[serde(default)]
    pub scoring_mode: PuzzleBoardScoringMode,
    /// Opt-in, **unstable** soft-scorer tuning knobs. Leave unset (`None`)
    /// unless a specific input fails and you have evidence for the change;
    /// `None` behaves exactly like [`PuzzleBoardAdvancedTuning::default()`].
    /// Set via [`with_advanced`](Self::with_advanced). See
    /// [`PuzzleBoardAdvancedTuning`] — its fields are NOT covered by semver.
    ///
    /// Serialized under a nested `"advanced"` object when `Some`, and omitted
    /// entirely when `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub advanced: Option<Box<PuzzleBoardAdvancedTuning>>,
}

/// Advanced, unstable soft-log-likelihood tuning knobs for the PuzzleBoard
/// decoder.
///
/// These knobs govern the per-bit soft-scoring and the hypothesis-acceptance
/// margin used under [`PuzzleBoardScoringMode::SoftLogLikelihood`]. They are
/// split out of [`PuzzleBoardDecodeConfig`] because they are
/// decode-scorer-implementation tuning rather than the small stable decode
/// core a consumer has a basis to set.
///
/// **Unstable:** every field here is **NOT covered by semver** and may be
/// retuned, retyped, or removed between minor versions as the decoder
/// evolves. Leave the whole struct at [`Default`] unless you are tuning
/// against a specific dataset with measured evidence.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PuzzleBoardAdvancedTuning {
    /// Soft-LL logit slope: `logit = bit_likelihood_slope × confidence` at a
    /// clean match. Higher values produce sharper soft-match/soft-mismatch
    /// separation.
    #[serde(default = "default_bit_likelihood_slope")]
    pub bit_likelihood_slope: f32,
    /// Lower bound applied to each per-bit `log_sigmoid` contribution.
    /// Prevents a single catastrophically wrong bit from dominating the
    /// hypothesis score.
    #[serde(default = "default_per_bit_floor")]
    pub per_bit_floor: f32,
    /// Minimum per-observation score gap between the winning hypothesis and
    /// the runner-up. Detections below this gate are rejected with
    /// [`crate::detector::error::PuzzleBoardDetectError::DecodeFailed`].
    #[serde(default = "default_alignment_min_margin")]
    pub alignment_min_margin: f32,
}

impl Default for PuzzleBoardAdvancedTuning {
    fn default() -> Self {
        Self {
            bit_likelihood_slope: default_bit_likelihood_slope(),
            per_bit_floor: default_per_bit_floor(),
            alignment_min_margin: default_alignment_min_margin(),
        }
    }
}

impl PuzzleBoardAdvancedTuning {
    /// Build the advanced soft-scorer tuning knobs at their default values.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

fn default_min_window() -> u32 {
    // 7×7 (84 interior edges) — the smallest window the master code can decode
    // with zero empirical false-accepts under the uniqueness gate at ≤40 % BER
    // (bounded-distance decoding; see `min_window` field docs and Gap 19).
    7
}
fn default_min_bit_confidence() -> f32 {
    0.15
}
fn default_max_bit_error_rate() -> f32 {
    0.30
}
fn default_search_all_components() -> bool {
    true
}
fn default_sample_radius_rel() -> f32 {
    1.0 / 6.0
}
fn default_bit_likelihood_slope() -> f32 {
    12.0
}
fn default_per_bit_floor() -> f32 {
    -6.0
}
fn default_alignment_min_margin() -> f32 {
    0.02
}

impl Default for PuzzleBoardDecodeConfig {
    fn default() -> Self {
        Self {
            min_window: default_min_window(),
            min_bit_confidence: default_min_bit_confidence(),
            max_bit_error_rate: default_max_bit_error_rate(),
            search_all_components: default_search_all_components(),
            sample_radius_rel: default_sample_radius_rel(),
            search_mode: PuzzleBoardSearchMode::default(),
            scoring_mode: PuzzleBoardScoringMode::default(),
            advanced: None,
        }
    }
}

impl PuzzleBoardDecodeConfig {
    /// Attach a [`PuzzleBoardAdvancedTuning`] override and return the updated
    /// config.
    ///
    /// The advanced knobs are NOT covered by semver — see
    /// [`PuzzleBoardAdvancedTuning`]. Leaving them unset (the default) keeps
    /// the decoder on the canonical soft-LL tuning.
    #[must_use]
    pub fn with_advanced(mut self, tuning: PuzzleBoardAdvancedTuning) -> Self {
        self.advanced = Some(Box::new(tuning));
        self
    }

    /// The advanced soft-scorer tuning the decoder will actually use.
    ///
    /// Returns [`Cow::Borrowed`] when [`advanced`](Self::advanced) is set, and
    /// an owned [`PuzzleBoardAdvancedTuning::default()`] otherwise. Decode
    /// stages bind this once and read fields off it, so the default case
    /// allocates a single struct (no per-knob branching) and the configured
    /// case borrows without copying.
    #[must_use]
    pub fn effective_tuning(&self) -> Cow<'_, PuzzleBoardAdvancedTuning> {
        match &self.advanced {
            Some(tuning) => Cow::Borrowed(tuning.as_ref()),
            None => Cow::Owned(PuzzleBoardAdvancedTuning::default()),
        }
    }
}

/// Minimum number of observed interior edges required to attempt decoding.
///
/// A decode window of `w × w` squares produces `2w(w-1)` interior edges.
pub(crate) fn required_edges(min_window: u32) -> usize {
    let w = min_window.max(3) as usize;
    2 * w * (w - 1)
}

/// Return an error if fewer than `needed` edges were observed.
pub(crate) fn ensure_min_edges(
    observed: usize,
    needed: usize,
) -> Result<(), PuzzleBoardDetectError> {
    if observed < needed {
        return Err(PuzzleBoardDetectError::NotEnoughEdges { observed, needed });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_edges_scales_with_window() {
        assert_eq!(required_edges(3), 12);
        assert_eq!(required_edges(4), 24);
        assert_eq!(required_edges(5), 40);
    }

    #[test]
    fn min_edges_check_reports_filtered_count() {
        let err = ensure_min_edges(7, required_edges(4)).expect_err("too few edges");
        assert!(matches!(
            err,
            PuzzleBoardDetectError::NotEnoughEdges {
                observed: 7,
                needed: 24
            }
        ));
    }

    #[test]
    fn default_config_serializes_without_advanced_key() {
        // The default decode config leaves `advanced` unset, so the moved
        // soft-LL knobs MUST NOT appear at the top level and there MUST be no
        // `"advanced"` key.
        let value = serde_json::to_value(PuzzleBoardDecodeConfig::default()).unwrap();
        let obj = value.as_object().expect("config serializes to an object");
        assert!(
            !obj.contains_key("advanced"),
            "default must omit `advanced`"
        );
        for leaked in [
            "bit_likelihood_slope",
            "per_bit_floor",
            "alignment_min_margin",
        ] {
            assert!(
                !obj.contains_key(leaked),
                "advanced knob `{leaked}` leaked to the top level"
            );
        }
    }

    #[test]
    fn effective_tuning_default_matches_advanced_default() {
        // `effective_tuning()` with `advanced: None` MUST be byte-identical to
        // `PuzzleBoardAdvancedTuning::default()` — the behaviour-preservation
        // contract for the opt-in split.
        let cfg = PuzzleBoardDecodeConfig::default();
        assert!(cfg.advanced.is_none());
        let effective = cfg.effective_tuning();
        let expected = PuzzleBoardAdvancedTuning::default();
        assert_eq!(
            serde_json::to_value(effective.as_ref()).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
        assert_eq!(effective.bit_likelihood_slope, 12.0);
        assert_eq!(effective.per_bit_floor, -6.0);
        assert_eq!(effective.alignment_min_margin, 0.02);
    }

    #[test]
    fn with_advanced_serializes_nested_block_and_round_trips() {
        let tuning = PuzzleBoardAdvancedTuning {
            bit_likelihood_slope: 15.0,
            ..Default::default()
        };
        let cfg = PuzzleBoardDecodeConfig::default().with_advanced(tuning);
        let value = serde_json::to_value(&cfg).unwrap();
        let obj = value.as_object().unwrap();
        let advanced = obj
            .get("advanced")
            .and_then(|v| v.as_object())
            .expect("expected a nested `advanced` object");
        assert_eq!(advanced["bit_likelihood_slope"], 15.0);
        assert!(!obj.contains_key("bit_likelihood_slope"));

        let restored: PuzzleBoardDecodeConfig = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(&restored).unwrap(), value);
        assert_eq!(restored.effective_tuning().bit_likelihood_slope, 15.0);
    }
}
