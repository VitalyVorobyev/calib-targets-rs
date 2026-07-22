//! Chessboard detector parameters.
//!
//! The configuration is split into two surfaces:
//!
//! - [`ChessboardParams`] — the **stable core**. Three knobs a calibration
//!   consumer has a basis to set: the minimum labelled-corner count for a
//!   detection to be emitted, the maximum number of disconnected components
//!   returned by [`crate::ChessboardDetector::detect_all`], and the minimum ChESS corner
//!   strength pre-filter. These fields are covered by semver.
//! - [`ChessboardAdvancedTuning`] — the **opt-in, unstable** sub-struct behind
//!   [`ChessboardParams::advanced`]. ~40 stage-tuning knobs named after the
//!   internal pipeline stages, accreted over algorithm-debugging sessions.
//!   These knobs are **NOT covered by semver** and may change between minor
//!   versions. The defaults are chosen to hold the detector's
//!   precision-by-construction contract; tune only when a specific input
//!   fails and you have evidence for the change.
//!
//! `advanced` is `Option`-wrapped and serialized as a nested `"advanced"`
//! object (it is **not** flattened). When unset, the serialized config carries
//! only the three stable top-level keys and no `"advanced"` key, and detection
//! behaves exactly as if every advanced knob held its [`Default`] value —
//! [`ChessboardParams::effective_tuning`] returns an owned
//! [`ChessboardAdvancedTuning::default()`] in that case.

mod advanced;

pub use advanced::ChessboardAdvancedTuning;

use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// A [`ChessboardParams`] configuration the chessboard detector cannot honour.
///
/// Reserved for future configuration validations; currently none. No
/// configuration is rejected — every value combination the public surface can
/// express is honoured — so [`ChessboardParams::validate`] always returns
/// `Ok(())` and this error is never constructed today. The fallible
/// [`crate::ChessboardDetector::new`] signature and this `#[non_exhaustive]` error type
/// are retained as a stable seam so a future validation can be added (a new
/// variant) without a breaking change, and so the binding layer keeps wrapping
/// a single `Result` surface uniformly across the sibling detectors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ChessboardParamsError {}

impl core::fmt::Display for ChessboardParamsError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Uninhabited today: no validation rejects a config, so no value of
        // this type can exist. The generic message keeps the binding layer's
        // error mapping uniform without a per-variant match.
        f.write_str("invalid chessboard detector configuration")
    }
}

impl std::error::Error for ChessboardParamsError {}

/// Top-level detector configuration.
///
/// A small **stable core** of three knobs a calibration consumer has a basis to
/// set, plus an opt-in, unstable [`ChessboardAdvancedTuning`] sub-struct
/// ([`advanced`](Self::advanced)) holding the ~40 per-stage tuning knobs.
///
/// The grid `(i, j) → corner` labelling is built with the **topological** grid
/// finder: a Delaunay triangulation plus an axis-driven cell test (the
/// image-free variant of the SBF09 grid finder; see
/// [`projective_grid::TopologicalParams`]). It has a low setup cost, no global
/// cell-size dependency, high recall on the clean-chessboard regression set,
/// and tolerates severe radial distortion and low view angles well.
///
/// The three stable fields are part of the public configuration contract and
/// serialize as top-level JSON keys (`min_labeled_corners`, `max_components`,
/// `min_corner_strength`). The
/// advanced knobs are gated behind [`with_advanced`](Self::with_advanced):
/// when set they serialize under a nested `"advanced"` object, and when unset
/// no `"advanced"` key appears.
///
/// # Migrating from a flat tuning config (pre-3.0)
///
/// Earlier versions flattened all tuning knobs into the top level via a
/// `tuning` sub-struct, so every knob was a top-level JSON key. The knob set
/// is now nested under `"advanced"`, with the single exception of
/// `min_corner_strength`, which was promoted to a stable top-level field and
/// keeps its top-level key. To carry advanced knobs forward, move them into an
/// `"advanced"` object and build the params with
/// [`with_advanced`](Self::with_advanced):
///
/// ```
/// use calib_targets_chessboard::{ChessboardAdvancedTuning, ChessboardParams};
///
/// // `ChessboardAdvancedTuning` is `#[non_exhaustive]`, so build from `default()` and
/// // mutate the knobs you need rather than using struct-literal syntax.
/// let mut advanced = ChessboardAdvancedTuning::default();
/// advanced.cluster_tol_deg = 9.0;
/// let params = ChessboardParams::default().with_advanced(advanced);
/// ```
#[non_exhaustive]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChessboardParams {
    /// Minimum labelled corners for a
    /// [`ChessboardDetection`](crate::ChessboardDetection) to be emitted.
    /// Default `8` ([`ChessboardParams::default`]); a **required** key in a
    /// serialized config.
    pub min_labeled_corners: usize,

    /// Maximum number of components returned by [`crate::ChessboardDetector::detect_all`].
    ///
    /// A chessboard can split into multiple disconnected pieces on ChArUco
    /// scenes where markers break contiguity. Each iteration peels off one
    /// grown grid from the unconsumed corners and re-runs seed → grow →
    /// validate. Default `3` ([`ChessboardParams::default`]); a **required** key
    /// in a serialized config.
    ///
    /// Does NOT claim to support scenes with two separate physical boards —
    /// one target per frame is the contract.
    pub max_components: u32,

    /// Minimum corner strength (ChESS response) for the Stage-1 pre-filter.
    /// Corners with `strength < min_corner_strength` are dropped before
    /// clustering. `0.0` disables the filter.
    ///
    /// **Default `33.0`.** A defocused board edge (or marker-bit saddle)
    /// fires the ChESS detector weakly — strength ≈ 15–30 against a sharp
    /// board's ≈ 90+ — and such corners, while grid-consistent in position,
    /// are low-confidence and pollute the labelled frontier with a ragged,
    /// noisy row that is unhelpful for calibration. A `33.0` floor removes
    /// that weak frontier; sharp boards (whose every corner clears the
    /// floor) are unaffected. The value matches the ChArUco detector's floor
    /// (`CharucoParams::for_board`), so the chessboard and ChArUco grid
    /// builds now start from the same corner set — set this to `0.0`
    /// explicitly to recover the previous maximum-recall behaviour.
    ///
    /// Part of the stable configuration core. A **required** key in a
    /// serialized config (top-level `min_corner_strength`).
    pub min_corner_strength: f32,

    /// Opt-in, **unstable** per-stage tuning knobs. Leave unset (`None`)
    /// unless a specific input fails and you have evidence for the change;
    /// `None` behaves exactly like [`ChessboardAdvancedTuning::default()`]. Set via
    /// [`with_advanced`](Self::with_advanced). See [`ChessboardAdvancedTuning`] — its
    /// fields are NOT covered by semver.
    ///
    /// Serialized under a nested `"advanced"` object when `Some`, and omitted
    /// entirely when `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub advanced: Option<Box<ChessboardAdvancedTuning>>,
}

impl Default for ChessboardParams {
    fn default() -> Self {
        Self {
            min_labeled_corners: 8,
            max_components: 3,
            min_corner_strength: 33.0,
            advanced: None,
        }
    }
}

impl ChessboardParams {
    /// Validate the configuration, returning a typed error for any combination
    /// the detector cannot honour.
    ///
    /// Currently every value combination the public surface can express is
    /// honoured, so this always returns `Ok(())`. The fallible signature is
    /// retained as a stable seam (see [`ChessboardParamsError`]): the fallible
    /// constructor [`crate::ChessboardDetector::new`] calls this up front so the
    /// `detect*` methods always run on a validated configuration, and a future
    /// validation can be added without changing the call sites.
    pub fn validate(&self) -> Result<(), ChessboardParamsError> {
        Ok(())
    }

    /// Attach a [`ChessboardAdvancedTuning`] override and return the updated params.
    ///
    /// The advanced knobs are NOT covered by semver — see [`ChessboardAdvancedTuning`].
    /// Leaving them unset (the default) keeps detection on the
    /// precision-by-construction defaults.
    #[must_use]
    pub fn with_advanced(mut self, tuning: ChessboardAdvancedTuning) -> Self {
        self.advanced = Some(Box::new(tuning));
        self
    }

    /// The advanced tuning the detector will actually use.
    ///
    /// Returns [`Cow::Borrowed`] when [`advanced`](Self::advanced) is set, and
    /// an owned [`ChessboardAdvancedTuning::default()`] otherwise. Internal pipeline
    /// stages bind this once at the top of a function and read fields off it,
    /// so the default case allocates a single struct (no per-knob branching)
    /// and the configured case borrows without copying.
    #[must_use]
    pub fn effective_tuning(&self) -> Cow<'_, ChessboardAdvancedTuning> {
        match &self.advanced {
            Some(tuning) => Cow::Borrowed(tuning.as_ref()),
            None => Cow::Owned(ChessboardAdvancedTuning::default()),
        }
    }

    /// Three-config sweep preset: default + tighter + looser angular tolerances.
    ///
    /// Intended for `detect_chessboard_best`-style flows that try multiple
    /// configurations and return the result with the most labelled corners.
    /// All three configurations preserve the detector's
    /// precision-by-construction invariants; only recall-affecting
    /// tolerances are varied. The two non-default configurations carry an
    /// [`ChessboardAdvancedTuning`] override.
    pub fn sweep_default() -> Vec<Self> {
        let base = Self::default();
        let base_tuning = base.effective_tuning().into_owned();
        let tight = base.clone().with_advanced(ChessboardAdvancedTuning {
            cluster_tol_deg: 9.0,
            attach_axis_tol_deg: 12.0,
            ..base_tuning.clone()
        });
        let loose = base.clone().with_advanced(ChessboardAdvancedTuning {
            cluster_tol_deg: 16.0,
            attach_axis_tol_deg: 18.0,
            ..base_tuning
        });
        vec![base, tight, loose]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn sweep_default_has_three_configs() {
        let configs = ChessboardParams::sweep_default();
        assert_eq!(configs.len(), 3);
        let base = configs[0].effective_tuning();
        let tight = configs[1].effective_tuning();
        let loose = configs[2].effective_tuning();
        assert!(tight.cluster_tol_deg < base.cluster_tol_deg);
        assert!(loose.cluster_tol_deg > base.cluster_tol_deg);
        assert!(tight.attach_axis_tol_deg < base.attach_axis_tol_deg);
        assert!(loose.attach_axis_tol_deg > base.attach_axis_tol_deg);
    }

    #[test]
    fn effective_tuning_default_matches_advanced_default() {
        // `effective_tuning()` with `advanced: None` MUST be byte-identical
        // to `ChessboardAdvancedTuning::default()` — this is the behaviour-preservation
        // contract for the opt-in split.
        let params = ChessboardParams::default();
        let effective = params.effective_tuning();
        let expected = ChessboardAdvancedTuning::default();
        assert_eq!(
            serde_json::to_value(effective.as_ref()).unwrap(),
            serde_json::to_value(&expected).unwrap()
        );
    }

    #[test]
    fn default_params_serialize_only_stable_keys() {
        // The default config must carry exactly the three stable top-level
        // keys, NO advanced knobs, and no `"advanced"` key.
        let value = serde_json::to_value(ChessboardParams::default()).unwrap();
        let obj = value.as_object().expect("params serialize to an object");
        let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "max_components",
                "min_corner_strength",
                "min_labeled_corners",
            ]
        );
        assert!(!obj.contains_key("advanced"));
        // None of the advanced knobs leaked to the top level.
        for leaked in [
            "cluster_tol_deg",
            "seed_edge_tol",
            "weak_cluster_tol_deg",
            "topological",
            "component_merge",
        ] {
            assert!(
                !obj.contains_key(leaked),
                "advanced knob `{leaked}` leaked to the top level"
            );
        }
    }

    #[test]
    fn with_advanced_serializes_nested_block_and_round_trips() {
        let params = ChessboardParams::default().with_advanced(ChessboardAdvancedTuning::default());
        let value = serde_json::to_value(&params).unwrap();
        let obj = value.as_object().unwrap();
        assert!(
            obj.get("advanced").map(Value::is_object).unwrap_or(false),
            "expected a nested `advanced` object, got {value}"
        );
        // The nested block carries the advanced knobs (not the top level).
        let advanced = obj["advanced"].as_object().unwrap();
        assert!(advanced.contains_key("cluster_tol_deg"));
        assert!(!obj.contains_key("cluster_tol_deg"));

        // Round-trips back to an equivalent struct.
        let restored: ChessboardParams = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(&restored).unwrap(), value);
        assert!(restored.advanced.is_some());
    }

    #[test]
    fn min_corner_strength_is_top_level_and_stable() {
        let params = ChessboardParams {
            min_corner_strength: 0.5,
            ..ChessboardParams::default()
        };
        let value = serde_json::to_value(&params).unwrap();
        assert_eq!(value["min_corner_strength"], serde_json::json!(0.5));
        // Deserializing a config that sets only the stable keys (and omits
        // `advanced`) reads `min_corner_strength` from the top level and
        // leaves `advanced` unset — i.e. the detector keeps default tuning.
        let restored: ChessboardParams = serde_json::from_value(serde_json::json!({
            "min_corner_strength": 0.5,
            "min_labeled_corners": 8,
            "max_components": 3,
        }))
        .unwrap();
        assert_eq!(restored.min_corner_strength, 0.5);
        assert!(restored.advanced.is_none());
    }

    #[test]
    fn omitting_a_required_stable_key_fails_to_deserialize() {
        // No serde defaults on the stable core: a config that omits any of the
        // three required keys is rejected rather than silently defaulted —
        // there is no hidden redundant default to fall back on.
        let result = serde_json::from_value::<ChessboardParams>(serde_json::json!({
            "min_corner_strength": 33.0,
            "max_components": 3,
            // `min_labeled_corners` omitted on purpose.
        }));
        assert!(result.is_err(), "a config missing a required key must fail");
    }

    #[test]
    fn unknown_key_is_rejected() {
        // `deny_unknown_fields`: a stray / legacy key (e.g. the removed
        // `graph_build_algorithm`) is rejected, not silently ignored.
        let result = serde_json::from_value::<ChessboardParams>(serde_json::json!({
            "min_labeled_corners": 8,
            "max_components": 3,
            "min_corner_strength": 33.0,
            "graph_build_algorithm": "topological",
        }));
        assert!(result.is_err(), "an unknown key must be rejected");
    }
}
