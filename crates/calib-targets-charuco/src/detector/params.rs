use crate::board::CharucoBoardSpec;
use calib_targets_aruco::ScanDecodeConfig;
use calib_targets_chessboard::ChessboardParams;
use calib_targets_core::{default_chess_config, DetectorConfig};
use chess_corners::{ChessRefiner, SaddlePointConfig};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// Configuration for the ChArUco detector.
#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharucoParams {
    /// ChESS corner front-end configuration for the main detection pass.
    ///
    /// Defaults to [`default_chess_config`]. Override it to run the corner
    /// pass coarse-to-fine (`MultiscaleConfig::Pyramid`) on large frames, or
    /// to pre-upscale low-resolution board crops (`UpscaleConfig::Fixed`)
    /// whose corners would otherwise fall inside the ChESS ring margin —
    /// the common failure mode on small ChArUco cutouts. Corner positions are
    /// always reported in input-image pixels regardless.
    ///
    /// This is the *main* detection pass; the local re-detection of individual
    /// suspicious corners runs on separate, internal fixed defaults.
    ///
    /// Every whole-image entry point runs this front-end over the input image
    /// to produce the corner cloud:
    /// [`CharucoDetector::detect`](crate::CharucoDetector::detect),
    /// [`CharucoDetector::detect_corners`](crate::CharucoDetector::detect_corners),
    /// and the facade's `detect_charuco` / `detect_charuco_best`. The
    /// `*_with_corners` entry points consume an already-detected
    /// `&[ChessCorner]` and never read this field.
    #[serde(default = "default_chess_config")]
    pub chess: DetectorConfig,
    /// Pixels per board square in the canonical sampling space.
    #[serde(default = "default_px_per_square")]
    pub px_per_square: f32,
    /// Chessboard detection parameters.
    ///
    /// ChArUco runs on the **topological** grid builder (the workspace
    /// default). The `min_corner_strength` floor set by
    /// [`CharucoParams::for_board`] keeps marker-bit saddles out of the grid,
    /// so the topological cell test labels ChArUco grids cleanly, and the
    /// marker-decode stages downstream of grid construction are
    /// builder-agnostic.
    #[serde(default)]
    pub chessboard: ChessboardParams,
    /// ChArUco board parameters
    #[serde(alias = "charuco")]
    pub board: CharucoBoardSpec,
    /// Marker scan parameters.
    ///
    /// `CharucoParams::for_board` uses a slightly smaller inset
    /// (`inset_frac = 0.06`) to improve real-image robustness. If
    /// `scan.marker_size_rel <= 0.0`, it is filled from the board spec.
    #[serde(default)]
    pub scan: ScanDecodeConfig,
    /// Minimal number of marker inliers needed to accept the alignment.
    #[serde(default = "default_min_marker_inliers")]
    pub min_marker_inliers: usize,
    /// ChESS detector configuration used for local corner re-detection.
    ///
    /// When validation identifies a false corner, this config drives a
    /// single-scale ROI detection ([`chess_corners::Detector::detect_u8_roi`])
    /// in a small window centred on the marker-predicted seed position.
    ///
    /// Internal (`pub(crate)`): the field carries the upstream `chess_corners`
    /// [`DetectorConfig`] and is `#[serde(skip)]`, so JSON / binding consumers
    /// could never set it — it is reconstructed from defaults on every
    /// deserialisation and is not part of the public configuration surface.
    #[serde(skip)]
    pub(crate) corner_redetect_params: DetectorConfig,
    /// Opt-in, **unstable** board-level-matcher and corner-validation tuning
    /// knobs. Leave unset (`None`) unless a specific input fails and you have
    /// evidence for the change; `None` behaves exactly like
    /// [`CharucoAdvancedTuning::default()`]. Set via
    /// [`with_advanced`](Self::with_advanced). See [`CharucoAdvancedTuning`] —
    /// its fields are NOT covered by semver.
    ///
    /// Serialized under a nested `"advanced"` object when `Some`, and omitted
    /// entirely when `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub advanced: Option<Box<CharucoAdvancedTuning>>,
}

/// Advanced, unstable tuning knobs for the ChArUco detector.
///
/// These knobs govern the soft-bit log-likelihood scoring and the
/// hypothesis-acceptance margin inside [`crate::CharucoDetector`]'s
/// board-level matcher, plus the grid-smoothness / marker-constrained
/// corner-validation gates and the secondary-component inlier floor. They
/// are split out of [`CharucoParams`] because they are
/// implementation-tuning rather than the small stable configuration core a
/// calibration consumer has a basis to set.
///
/// **Unstable:** every field here is **NOT covered by semver** and may be
/// retuned, retyped, or removed between minor versions as the detector
/// evolves. Leave the whole struct at [`Default`] unless you are tuning
/// against a specific dataset with measured evidence.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharucoAdvancedTuning {
    /// Logistic slope κ used in the board-level matcher's soft-bit
    /// log-likelihood. Larger = more confident per bit; 8–16 is a
    /// reasonable range.
    #[serde(default = "default_bit_likelihood_slope")]
    pub bit_likelihood_slope: f32,
    /// Clip floor applied to each per-bit log-likelihood term before
    /// summing across bits, so a single wildly-wrong bit cannot dominate
    /// a cell score.
    #[serde(default = "default_per_bit_floor")]
    pub per_bit_floor: f32,
    /// Minimum `(best − runner-up) / |best|` margin required for the
    /// board-level matcher to accept a hypothesis. Below this, detection
    /// is rejected rather than mislabelled.
    #[serde(default = "default_alignment_min_margin")]
    pub alignment_min_margin: f32,
    /// Border-black fraction threshold below which a cell's weight is
    /// attenuated linearly toward 0 in the board-level score.
    #[serde(default = "default_cell_weight_border_threshold")]
    pub cell_weight_border_threshold: f32,
    /// Relative threshold for the local grid-smoothness pre-filter.
    ///
    /// Each grid corner's position is predicted from its immediate neighbors
    /// via midpoint averaging. If the actual position deviates by more than
    /// `grid_smoothness_threshold_rel * px_per_square` pixels, the corner is
    /// re-detected locally or removed.
    ///
    /// Set to `f32::INFINITY` to disable.
    /// Default: `0.05` (3 px at 60 px/sq).
    #[serde(default = "default_grid_smoothness_threshold_rel")]
    pub grid_smoothness_threshold_rel: f32,
    /// Relative threshold for marker-constrained corner validation.
    ///
    /// A detected ChArUco corner is considered a false corner if its pixel
    /// position deviates from the marker-predicted seed by more than
    /// `corner_validation_threshold_rel * px_per_square` pixels.
    ///
    /// Set to `f32::INFINITY` to disable validation entirely.
    /// Typical value: `0.08` (8 % of a board square side, ~5 px at 60 px/sq).
    #[serde(default = "default_corner_validation_threshold_rel")]
    pub corner_validation_threshold_rel: f32,
    /// Minimum marker inliers for secondary (non-largest) components.
    ///
    /// Lower than [`CharucoParams::min_marker_inliers`] because even a few
    /// markers suffice to confirm alignment for a small grid fragment.
    #[serde(default = "default_min_secondary_marker_inliers")]
    pub min_secondary_marker_inliers: usize,
}

impl Default for CharucoAdvancedTuning {
    fn default() -> Self {
        Self {
            bit_likelihood_slope: default_bit_likelihood_slope(),
            per_bit_floor: default_per_bit_floor(),
            alignment_min_margin: default_alignment_min_margin(),
            cell_weight_border_threshold: default_cell_weight_border_threshold(),
            grid_smoothness_threshold_rel: default_grid_smoothness_threshold_rel(),
            corner_validation_threshold_rel: default_corner_validation_threshold_rel(),
            min_secondary_marker_inliers: default_min_secondary_marker_inliers(),
        }
    }
}

impl CharucoAdvancedTuning {
    /// Build the advanced tuning knobs at their default values.
    pub fn new() -> Self {
        Self::default()
    }
}

fn default_grid_smoothness_threshold_rel() -> f32 {
    0.05
}

fn default_corner_validation_threshold_rel() -> f32 {
    0.08
}

fn default_px_per_square() -> f32 {
    60.0
}

fn default_min_marker_inliers() -> usize {
    // Board-appropriate floor for the board-level matcher, which is its own
    // inlier gate (it accepts/rejects on the margin gate, so the downstream
    // inlier floor stays low).
    1
}

fn default_min_secondary_marker_inliers() -> usize {
    1
}

fn default_bit_likelihood_slope() -> f32 {
    // Logistic slope κ for the soft-bit log-likelihood: larger = more
    // confident per bit. Too small a slope compresses the per-bit logit so
    // that a runner-up board hypothesis can nearly tie the top one (a
    // mislabel risk); beyond a moderate value the per-bit terms saturate and
    // the outcome stops changing. This default sits in that saturated regime.
    36.0
}

fn default_per_bit_floor() -> f32 {
    -6.0
}

fn default_alignment_min_margin() -> f32 {
    0.05
}

fn default_cell_weight_border_threshold() -> f32 {
    0.5
}

/// Build the ChESS config used for local re-detection inside a small ROI.
///
/// Lower threshold and looser cluster requirement compared to the global scan,
/// because we already know approximately where the true corner should be.
/// `threshold = 0.0` keeps every strictly-positive response. Orientation is
/// skipped ([`without_orientation`](DetectorConfig::without_orientation)): the
/// re-detection consumes only the refined corner position, so the per-corner
/// axis fit would be wasted work — preserving the old hand-rolled path's cost.
pub(crate) fn default_redetect_params() -> DetectorConfig {
    DetectorConfig::chess()
        .with_threshold(0.0)
        .with_detection(|d| {
            d.nms_radius = 2;
            d.min_cluster_size = 1;
        })
        .with_chess(|c| c.refiner = ChessRefiner::SaddlePoint(SaddlePointConfig::default()))
        .without_orientation()
}

impl CharucoParams {
    /// Three-config sweep preset built on top of
    /// [`ChessboardParams::sweep_default`] (canonical + tighter + looser
    /// chessboard tolerances).
    pub fn sweep_for_board(board: &CharucoBoardSpec) -> Vec<Self> {
        let base = Self::for_board(*board);
        // The ChArUco base sets a strength floor (stable field) and disables
        // the standalone final edge-shape gate (advanced knob). Re-apply both
        // to every recall-bracketed sweep config, preserving each config's own
        // advanced overrides (the tighter / looser tolerances).
        let base_strength = base.chessboard.min_corner_strength;
        let base_edge_shape_check = base
            .chessboard
            .effective_tuning()
            .enable_final_edge_shape_check;
        ChessboardParams::sweep_default()
            .into_iter()
            .map(|mut chessboard| {
                // `sweep_default()` already builds topological configs (the
                // workspace default), which is the builder ChArUco runs on.
                chessboard.min_corner_strength = base_strength;
                let mut advanced = chessboard.effective_tuning().into_owned();
                advanced.enable_final_edge_shape_check = base_edge_shape_check;
                chessboard = chessboard.with_advanced(advanced);
                Self {
                    chessboard,
                    ..base.clone()
                }
            })
            .collect()
    }

    /// Build a reasonable default configuration for the given board.
    ///
    /// The chessboard detector is scale-invariant and discovers cell size
    /// from the seed itself, and ChArUco's marker-driven alignment is the
    /// geometry gate, so no explicit expected-row / expected-column /
    /// completeness / minimum-corner-count gates are required.
    pub fn for_board(board: CharucoBoardSpec) -> Self {
        let mut chessboard = ChessboardParams::default();
        // `ChessboardParams::default()` already selects the topological builder,
        // which is the builder ChArUco runs on (see `CharucoParams::chessboard`).
        // Absolute ChESS-strength floor. In defocused regions the corner
        // detector fires weakly on ArUco-marker bit saddles that can align
        // with a grid extrapolation; those false corners are grid-consistent
        // (they pass the homography validation) and so survive into the
        // ChArUco product as biased corners — geometry alone cannot reject
        // them. Cutting weak corners *before* the grid grows keeps the grid
        // out of the blur entirely, and — because the marker cells are
        // sampled from that grid — also reduces spurious marker cells, so it
        // is the principled precision lever here. The cost is the weakest
        // blurred-margin corners (least useful for calibration). The board
        // alignment is a *location* tool, never a corner-drop gate, so this
        // floor — not marker presence — is what removes the bias.
        chessboard.min_corner_strength = 33.0;
        // The chessboard's final wrong-label geometry check is left enabled,
        // exactly as for the standalone chessboard detector. Marker decoding
        // is not a substitute for it: the board alignment searches
        // `D4 × integer translation` — a *rigid* relabelling of whatever
        // lattice the chessboard produced — so it cannot see, penalise or
        // repair a labelling that is wrong *within* a component. It picks the
        // alignment the healthy region votes for and applies it to the broken
        // region too.

        let scan = ScanDecodeConfig::default()
            .with_marker_size_rel(board.marker_size_rel)
            // The border ring is a physical property of the printed board, so
            // it comes from the spec rather than from a default: sampling the
            // wrong ring width shifts every payload bit.
            .with_border_bits(board.border_bits)
            .with_inset_frac(0.06)
            // Lower than the default (0.85) — downstream alignment validation
            // rejects false positives, so a looser bar here improves recall on
            // blurry or unevenly-lit images.
            .with_min_border_score(0.75);

        Self {
            chess: default_chess_config(),
            px_per_square: 60.0,
            chessboard,
            board,
            scan,
            // The board-level soft-LL matcher is its own inlier gate (it
            // accepts/rejects on the margin gate), so it is robust on partial /
            // blurry views and takes board-appropriate low inlier floors
            // (1 primary / 1 secondary).
            min_marker_inliers: 1,
            corner_redetect_params: default_redetect_params(),
            // `None` behaves exactly like `CharucoAdvancedTuning::default()`,
            // which carries the previous inline values (grid smoothness 0.05,
            // corner validation 0.08, secondary inliers 1), so detection is
            // byte-identical to the old defaults.
            advanced: None,
        }
    }

    /// Attach a [`CharucoAdvancedTuning`] override and return the updated
    /// params.
    ///
    /// The advanced knobs are NOT covered by semver — see
    /// [`CharucoAdvancedTuning`]. Leaving them unset (the default) keeps
    /// detection on the tuned defaults.
    #[must_use]
    pub fn with_advanced(mut self, tuning: CharucoAdvancedTuning) -> Self {
        self.advanced = Some(Box::new(tuning));
        self
    }

    /// The advanced tuning the detector will actually use.
    ///
    /// Returns [`Cow::Borrowed`] when [`advanced`](Self::advanced) is set, and
    /// an owned [`CharucoAdvancedTuning::default()`] otherwise. Pipeline stages
    /// bind this once and read fields off it, so the default case allocates a
    /// single struct (no per-knob branching) and the configured case borrows
    /// without copying.
    #[must_use]
    pub fn effective_tuning(&self) -> Cow<'_, CharucoAdvancedTuning> {
        match &self.advanced {
            Some(tuning) => Cow::Borrowed(tuning.as_ref()),
            None => Cow::Owned(CharucoAdvancedTuning::default()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_redetect_params_uses_saddle_point_refiner() {
        let cfg = default_redetect_params();
        // Absolute floor of 0.0 — the paper contract, and what this path has
        // always effectively run at. See the note in `default_redetect_params`.
        assert_eq!(cfg.threshold, 0.0);
        assert_eq!(cfg.detection.nms_radius, 2);
        assert_eq!(cfg.detection.min_cluster_size, 1);
        // Orientation is skipped: the re-detection uses only the refined
        // position, so the per-corner axis fit would be wasted cost.
        assert!(cfg.orientation_method.is_none());
        let chess_corners::DetectionStrategy::Chess(chess) = cfg.strategy else {
            panic!("expected the ChESS strategy, got {:?}", cfg.strategy);
        };
        assert!(
            matches!(chess.refiner, ChessRefiner::SaddlePoint(_)),
            "expected SaddlePoint refiner, got {:?}",
            chess.refiner,
        );
    }

    #[test]
    fn default_params_serialize_without_advanced_key() {
        // A `for_board` config leaves `advanced` unset, so the moved knobs
        // (`grid_smoothness_threshold_rel`, `corner_validation_threshold_rel`,
        // `min_secondary_marker_inliers`) MUST NOT appear at the top level and
        // there MUST be no `"advanced"` key.
        let board = CharucoBoardSpec::new(5, 7, 20.0, 0.75, builtin_dict());
        let value = serde_json::to_value(CharucoParams::for_board(board)).unwrap();
        let obj = value.as_object().expect("params serialize to an object");
        assert!(
            !obj.contains_key("advanced"),
            "default must omit `advanced`"
        );
        for leaked in [
            "grid_smoothness_threshold_rel",
            "corner_validation_threshold_rel",
            "min_secondary_marker_inliers",
            "bit_likelihood_slope",
            "corner_redetect_params",
        ] {
            assert!(
                !obj.contains_key(leaked),
                "advanced/internal knob `{leaked}` leaked to the top level"
            );
        }
    }

    #[test]
    fn effective_tuning_default_matches_advanced_default() {
        // `effective_tuning()` with `advanced: None` MUST be byte-identical to
        // `CharucoAdvancedTuning::default()` — the behaviour-preservation
        // contract for the opt-in split.
        let board = CharucoBoardSpec::new(5, 7, 20.0, 0.75, builtin_dict());
        let params = CharucoParams::for_board(board);
        assert!(params.advanced.is_none());
        let effective = params.effective_tuning();
        let expected = CharucoAdvancedTuning::default();
        assert_eq!(
            serde_json::to_value(effective.as_ref()).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
        // The moved knobs keep their previous inline defaults.
        assert_eq!(effective.grid_smoothness_threshold_rel, 0.05);
        assert_eq!(effective.corner_validation_threshold_rel, 0.08);
        assert_eq!(effective.min_secondary_marker_inliers, 1);
    }

    #[test]
    fn with_advanced_serializes_nested_block_and_round_trips() {
        let board = CharucoBoardSpec::new(5, 7, 20.0, 0.75, builtin_dict());
        let tuning = CharucoAdvancedTuning {
            min_secondary_marker_inliers: 2,
            ..Default::default()
        };
        let params = CharucoParams::for_board(board).with_advanced(tuning);
        let value = serde_json::to_value(&params).unwrap();
        let obj = value.as_object().unwrap();
        let advanced = obj
            .get("advanced")
            .and_then(|v| v.as_object())
            .expect("expected a nested `advanced` object");
        assert_eq!(advanced["min_secondary_marker_inliers"], 2);
        assert!(!obj.contains_key("min_secondary_marker_inliers"));

        // Round-trips back to an equivalent struct.
        let restored: CharucoParams = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(&restored).unwrap(), value);
        assert_eq!(restored.effective_tuning().min_secondary_marker_inliers, 2);
    }

    fn builtin_dict() -> calib_targets_aruco::Dictionary {
        calib_targets_aruco::builtins::builtin_dictionary("DICT_4X4_50").expect("builtin dict")
    }
}
