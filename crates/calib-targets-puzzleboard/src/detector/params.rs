//! Knobs for the decoding stage and associated validation helpers.

use serde::{Deserialize, Serialize};

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
pub struct PuzzleBoardDecodeConfig {
    /// Minimum window size (in squares) required to attempt a decode.
    ///
    /// The paper guarantees 3×3 = 99.33 % unique and 4×4 = 100 % unique.
    /// Default is 4 for calibration use.
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
    /// Soft-LL logit slope: `logit = bit_likelihood_slope × confidence` at a
    /// clean match. Higher values produce sharper soft-match/soft-mismatch
    /// separation. Only used under
    /// [`PuzzleBoardScoringMode::SoftLogLikelihood`].
    #[serde(default = "default_bit_likelihood_slope")]
    pub bit_likelihood_slope: f32,
    /// Lower bound applied to each per-bit `log_sigmoid` contribution.
    /// Prevents a single catastrophically wrong bit from dominating the
    /// hypothesis score. Only used under
    /// [`PuzzleBoardScoringMode::SoftLogLikelihood`].
    #[serde(default = "default_per_bit_floor")]
    pub per_bit_floor: f32,
    /// Minimum per-observation score gap between the winning hypothesis and
    /// the runner-up. Detections below this gate are rejected with
    /// [`crate::detector::error::PuzzleBoardDetectError::DecodeFailed`].
    /// Only used under [`PuzzleBoardScoringMode::SoftLogLikelihood`].
    #[serde(default = "default_alignment_min_margin")]
    pub alignment_min_margin: f32,
}

fn default_min_window() -> u32 {
    4
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
            bit_likelihood_slope: default_bit_likelihood_slope(),
            per_bit_floor: default_per_bit_floor(),
            alignment_min_margin: default_alignment_min_margin(),
        }
    }
}

impl PuzzleBoardDecodeConfig {
    /// Construct with explicit values for every field except `search_mode`,
    /// `scoring_mode`, and the soft-LL knobs, which default to
    /// [`PuzzleBoardSearchMode::Full`] / [`PuzzleBoardScoringMode::SoftLogLikelihood`]
    /// and the canonical soft-LL tuning.
    ///
    /// To use a different search or scoring mode, assign the field after
    /// construction:
    /// ```ignore
    /// let mut cfg = PuzzleBoardDecodeConfig::new(...);
    /// cfg.search_mode = PuzzleBoardSearchMode::FixedBoard;
    /// cfg.scoring_mode = PuzzleBoardScoringMode::HardWeighted;
    /// ```
    pub fn new(
        min_window: u32,
        min_bit_confidence: f32,
        max_bit_error_rate: f32,
        search_all_components: bool,
        sample_radius_rel: f32,
    ) -> Self {
        Self {
            min_window,
            min_bit_confidence,
            max_bit_error_rate,
            search_all_components,
            sample_radius_rel,
            search_mode: PuzzleBoardSearchMode::default(),
            scoring_mode: PuzzleBoardScoringMode::default(),
            bit_likelihood_slope: default_bit_likelihood_slope(),
            per_bit_floor: default_per_bit_floor(),
            alignment_min_margin: default_alignment_min_margin(),
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
}
