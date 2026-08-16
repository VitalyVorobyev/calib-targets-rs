//! Knobs for the decoding stage and associated validation helpers.

use calib_targets_core::{GridTransform, GRID_TRANSFORMS_C4, GRID_TRANSFORMS_D4};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

use crate::detector::error::PuzzleBoardDetectError;

/// Strategy for recovering the master-map origin during decode.
///
/// - [`PuzzleBoardSearchMode::Full`] considers every `(D4, origin)` candidate
///   against the full 501 × 501 master code. Works whether or not the caller
///   knows which printed board produced the image. (The candidate *space* is
///   `8 × 501 × 501`; the cyclic structure of the code means the decoder does
///   not enumerate it — see `detector::decode`.)
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
    /// A declared board is a sub-rectangle *cut from* the master, so this is
    /// the same scoring problem as [`PuzzleBoardSearchMode::Full`] restricted
    /// to the origins that rectangle admits. Restricting the origins also
    /// restricts the residue classes they reach, so the shared precompute does
    /// strictly less work — which makes declaring the board **cheaper** than
    /// not declaring it, not merely bounded. The saving shrinks as the board
    /// approaches the master's 167-long period; declaring a full 501 × 501
    /// board is the one case where the full search is faster, because there
    /// the declaration carries no information and [`PuzzleBoardSearchMode::Full`]
    /// gets a CRT collapse this mode cannot.
    ///
    /// Two guarantees come with the restriction:
    ///
    /// - **Origin inside the board.** The decode cannot return a position the
    ///   printed board does not cover, so every emitted `target_position` lies
    ///   within it.
    /// - **Partial-view consistency.** Any subset of the printed board decodes
    ///   to the same master IDs a full-view decode would produce, so subsets
    ///   across frames or cameras stitch cleanly.
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

/// Which board orientations the decoder is allowed to consider.
///
/// A detected fragment carries no cue for how the printed board was oriented in
/// front of the camera, so the decoder tries every candidate relabelling of the
/// grid before it can recover an absolute origin. This knob says which
/// relabellings are physically possible for your setup.
///
/// - [`PuzzleBoardSymmetryMode::Rotations`] (default) is correct for an
///   ordinary camera looking at a printed board: the board may appear rotated
///   by any multiple of 90°, but it cannot appear mirrored. This is both the
///   faster search (four hypotheses instead of eight) and the *more unique*
///   one — the four mirrored hypotheses can no longer alias a correct decode
///   into a rejection, so fragments that would otherwise be declined for
///   ambiguity now decode.
/// - [`PuzzleBoardSymmetryMode::RotationsAndReflections`] is needed only when
///   the optical path flips handedness — the board is seen through a mirror or
///   a beam splitter, or the image was mirrored before it reached the detector.
///   Enable it in exactly those cases; enabling it otherwise only costs time
///   and uniqueness.
///
/// Leaving this at the default never mislabels a mirrored view: a mirrored
/// fragment simply fails to decode (a miss), which is the safe direction.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PuzzleBoardSymmetryMode {
    /// Search the four 90° rotations only. Default.
    #[default]
    Rotations,
    /// Search the four rotations *and* their four mirror images.
    RotationsAndReflections,
}

impl PuzzleBoardSymmetryMode {
    /// The grid transforms the decoder searches under this mode.
    ///
    /// [`Rotations`](Self::Rotations) yields the orientation-preserving
    /// subgroup [`GRID_TRANSFORMS_C4`];
    /// [`RotationsAndReflections`](Self::RotationsAndReflections) yields the
    /// full dihedral table [`GRID_TRANSFORMS_D4`]. The former is a prefix of
    /// the latter, so a transform index means the same thing under both.
    #[must_use]
    pub fn transforms(self) -> &'static [GridTransform] {
        match self {
            Self::Rotations => &GRID_TRANSFORMS_C4,
            Self::RotationsAndReflections => &GRID_TRANSFORMS_D4,
        }
    }
}

/// Tuning parameters for the decoding stage.
#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PuzzleBoardDecodeConfig {
    /// Minimum fragment span, in **corners** per side, required to attempt a
    /// decode. A span of `s` corners encloses `s - 1` squares.
    ///
    /// The default of 7 corners (6 × 6 squares) is exactly the paper's 4 × 4
    /// claim, restated in the units our sampler works in. Two conversions
    /// separate the two numbers, and neither is a safety margin:
    ///
    /// **Interior readout.** A dot is read against the two squares flanking it,
    /// so a fragment's outermost ring of edges is unreadable — we see only the
    /// interior `2(s-1)(s-2)` edges where the paper counts all `2w(w+1)` edges
    /// bounding its `w × w` block.
    ///
    /// **Information, not edge count.** Both maps repeat every three rows or
    /// columns, so edges are not independent bits. The paper says as much of its
    /// own 4 × 4 fragment: 40 edges, but *"only 30 bits of information, as the
    /// remaining bits are repetitions"*. An interior readout at span `s` carries
    /// `6(s-2)` distinct bits, so `s = 7` carries **30** — the same information
    /// the paper's 4 × 4 window carries, one ring further out.
    ///
    /// Verified exhaustively over all 251 001 master positions
    /// (`research/puzzleboard-rings`): under the default rotations-only search a
    /// 7-corner span is unique at every position, and a 6-corner span (24 bits)
    /// leaves 3 330 positions ambiguous. Uniqueness is information-theoretic, so
    /// the period-3 consensus stage cannot lower this floor — voting corrects
    /// errors, it does not manufacture bits. Under
    /// [`RotationsAndReflections`](PuzzleBoardSymmetryMode::RotationsAndReflections)
    /// the exhaustive floor is 9, and 7 leaves 504 positions ambiguous.
    ///
    /// Fragments below the floor become detection *misses* rather than a risked
    /// wrong absolute label.
    #[serde(default = "default_min_window")]
    pub min_window: u32,
    /// Per-bit confidence floor — bits below this are treated as unknown.
    #[serde(default = "default_min_bit_confidence")]
    pub min_bit_confidence: f32,
    /// Maximum fraction of bits allowed to be wrong after majority voting.
    ///
    /// Applied to the **voted** bits — one per distinct master bit the fragment
    /// reads — not to the raw dots, which is what makes the paper's budget
    /// meaningful: *"up to 401/1002 ≈ 40 % of all bits are allowed to be decoded
    /// incorrectly **after** averaging over all repetitions"*. Default is 0.3.
    ///
    /// With
    /// [`edge_consensus`](PuzzleBoardAdvancedTuning::edge_consensus) off, every
    /// dot counts as its own bit and this reverts to a raw-dot rate, which is
    /// strictly stricter.
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
    /// Board orientations the decoder is allowed to consider. Defaults to
    /// [`PuzzleBoardSymmetryMode::Rotations`], which is correct for any
    /// ordinary camera; switch to
    /// [`PuzzleBoardSymmetryMode::RotationsAndReflections`] only when the
    /// optical path mirrors the image.
    #[serde(default)]
    pub symmetry_mode: PuzzleBoardSymmetryMode,
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

/// Advanced, unstable decode-internals knobs for the PuzzleBoard decoder.
///
/// These govern the per-bit soft scoring, the hypothesis-acceptance margin used
/// under [`PuzzleBoardScoringMode::SoftLogLikelihood`], and the period-3
/// consensus stage. They are split out of [`PuzzleBoardDecodeConfig`] because
/// they are decoder-implementation tuning rather than the small stable decode
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
    ///
    /// Normalised by the **physical** dot count, not by the logical bit count
    /// the other gates use — see
    /// [`edge_consensus`](Self::edge_consensus).
    #[serde(default = "default_alignment_min_margin")]
    pub alignment_min_margin: f32,
    /// Collapse the period-3 replicas to one entry per distinct master bit
    /// before the accept/reject gates. Default `true`.
    ///
    /// Both code maps repeat every three rows or columns, so a fragment
    /// sampling `~2w²` dots reads only `~6w` distinct bits. With this on:
    ///
    /// - the hard scorer decodes the **voted** bits, gaining the majority-vote
    ///   error correction the pattern was designed to provide;
    /// - the soft scorer still sums per-dot log-likelihoods (already the
    ///   optimal way to combine replicas, since class members predict the same
    ///   bit under every hypothesis), but its BER and uniqueness gates are
    ///   evaluated over the voted bits, which is the code length those
    ///   bounded-distance arguments are actually about.
    ///
    /// Turning it off restores the pre-0.13 behaviour, in which every gate
    /// treated each dot as an independent code bit. That is strictly
    /// pessimistic rather than unsafe, and the switch exists so the difference
    /// can be measured; there is no reason to ship with it off.
    #[serde(default = "default_edge_consensus")]
    pub edge_consensus: bool,
}

impl Default for PuzzleBoardAdvancedTuning {
    fn default() -> Self {
        Self {
            bit_likelihood_slope: default_bit_likelihood_slope(),
            per_bit_floor: default_per_bit_floor(),
            alignment_min_margin: default_alignment_min_margin(),
            edge_consensus: default_edge_consensus(),
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
fn default_edge_consensus() -> bool {
    true
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
            symmetry_mode: PuzzleBoardSymmetryMode::default(),
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
/// `min_window` is a span in **corners**
/// ([`PuzzleBoardDecodeConfig::min_window`]), matching the span gate in
/// `PuzzleBoardDetector::decode_component`. A span of `s` corners encloses
/// `s - 1` squares and therefore yields `2(s-1)(s-2)` interior edges — 60 at
/// the default `s = 7`.
///
/// This used to read `min_window` as a count of *squares* (`2s(s-1)`, 84 at
/// `s = 7`), which is the floor for a span of 8. The two gates were a ring
/// apart, and the stricter one silently won: fragments spanning exactly the
/// documented minimum were rejected before they were ever decoded. 60 edges is
/// the span-7 floor that `research/puzzleboard-rings` verifies as uniquely
/// decodable at every one of the 251 001 master positions.
pub(crate) fn required_edges(min_window: u32) -> usize {
    let s = min_window.max(3) as usize;
    2 * (s - 1) * (s - 2)
}

/// Minimum number of *distinct* master bits a decode may be judged over.
///
/// Both code maps repeat every three rows or columns, so a `s`-corner interior
/// readout spans `3` residue classes on each family's short axis and `s - 2`
/// positions on the other: `6(s - 2)` distinct bits, **30** at the default
/// `s = 7`. That is the same information the paper attributes to its 4 × 4
/// fragment ("only 30 bits of information, as the remaining bits are
/// repetitions") and the count `research/puzzleboard-rings` verifies as uniquely
/// decodable at all 251 001 master positions.
///
/// [`required_edges`] bounds the *dots* a fragment must sample; this bounds the
/// *bits* they must resolve. The two come apart once dots disagree, because a
/// class that splits evenly is erased rather than guessed — so a fragment can
/// clear the edge floor and the corner span and still resolve far too little
/// code to be placed uniquely.
pub(crate) fn required_logical_bits(min_window: u32) -> usize {
    let s = min_window.max(3) as usize;
    6 * (s - 2)
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

    /// `min_window` is a corner span, so the edge floor is the *interior* edge
    /// count of the `s-1` squares it encloses.
    #[test]
    fn required_edges_counts_interior_edges_of_a_corner_span() {
        assert_eq!(required_edges(4), 12); // 3 squares
        assert_eq!(required_edges(5), 24); // 4 squares
        assert_eq!(required_edges(6), 40); // 5 squares
        assert_eq!(required_edges(7), 60); // 6 squares — the default
        assert_eq!(required_edges(8), 84); // 7 squares
    }

    /// The edge floor and the corner-span gate must agree on the unit, or the
    /// stricter one silently overrides the documented minimum.
    #[test]
    fn edge_floor_matches_what_a_minimum_span_actually_yields() {
        for span in 4u32..=12 {
            let squares = (span - 1) as usize;
            let interior = 2 * squares * (squares - 1);
            assert_eq!(
                required_edges(span),
                interior,
                "span {span} corners encloses {squares} squares"
            );
        }
    }

    #[test]
    fn min_edges_check_reports_filtered_count() {
        let err = ensure_min_edges(7, required_edges(5)).expect_err("too few edges");
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
    fn default_symmetry_mode_is_rotations_only() {
        // The shipped default must be the physically reachable subgroup: a
        // camera imaging the printed side of an opaque board cannot mirror it.
        let cfg = PuzzleBoardDecodeConfig::default();
        assert_eq!(cfg.symmetry_mode, PuzzleBoardSymmetryMode::Rotations);
        assert_eq!(cfg.symmetry_mode.transforms().len(), 4);
        assert_eq!(cfg.symmetry_mode.transforms(), &GRID_TRANSFORMS_C4);
    }

    #[test]
    fn symmetry_mode_transforms_select_the_right_table() {
        assert_eq!(
            PuzzleBoardSymmetryMode::Rotations.transforms(),
            &GRID_TRANSFORMS_C4
        );
        assert_eq!(
            PuzzleBoardSymmetryMode::RotationsAndReflections.transforms(),
            &GRID_TRANSFORMS_D4
        );
        // Every searched transform is invertible on the integer lattice; the
        // opt-in table is exactly the default table plus the reflections.
        for t in PuzzleBoardSymmetryMode::Rotations.transforms() {
            assert_eq!(t.determinant(), 1);
        }
        assert_eq!(
            PuzzleBoardSymmetryMode::RotationsAndReflections
                .transforms()
                .iter()
                .filter(|t| t.determinant() == -1)
                .count(),
            4
        );
    }

    #[test]
    fn symmetry_mode_round_trips_through_serde() {
        // Tagged like its neighbours: `{"kind": "..."}`.
        let value = serde_json::to_value(PuzzleBoardSymmetryMode::RotationsAndReflections).unwrap();
        assert_eq!(value["kind"], "rotations_and_reflections");
        let restored: PuzzleBoardSymmetryMode = serde_json::from_value(value).unwrap();
        assert_eq!(restored, PuzzleBoardSymmetryMode::RotationsAndReflections);

        // ...and as a config field, including the default-on-absent path that
        // keeps older serialized configs loadable.
        let cfg = PuzzleBoardDecodeConfig {
            symmetry_mode: PuzzleBoardSymmetryMode::RotationsAndReflections,
            ..Default::default()
        };
        let value = serde_json::to_value(&cfg).unwrap();
        assert_eq!(value["symmetry_mode"]["kind"], "rotations_and_reflections");
        let restored: PuzzleBoardDecodeConfig = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(
            restored.symmetry_mode,
            PuzzleBoardSymmetryMode::RotationsAndReflections
        );
        assert_eq!(serde_json::to_value(&restored).unwrap(), value);

        let mut without = value.as_object().unwrap().clone();
        without.remove("symmetry_mode");
        let restored: PuzzleBoardDecodeConfig =
            serde_json::from_value(serde_json::Value::Object(without)).unwrap();
        assert_eq!(restored.symmetry_mode, PuzzleBoardSymmetryMode::Rotations);
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
