//! Chessboard detector parameters.
//!
//! The configuration is split into two surfaces:
//!
//! - [`DetectorParams`] — the **stable core**. Four knobs a calibration
//!   consumer has a basis to set: the graph-build algorithm, the minimum
//!   labelled-corner count for a detection to be emitted, the maximum number
//!   of disconnected components returned by
//!   [`crate::Detector::detect_all`], and the minimum ChESS corner strength
//!   pre-filter. These fields are covered by semver.
//! - [`AdvancedTuning`] — the **opt-in, unstable** sub-struct behind
//!   [`DetectorParams::advanced`]. ~40 stage-tuning knobs named after the
//!   internal pipeline stages, accreted over algorithm-debugging sessions.
//!   These knobs are **NOT covered by semver** and may change between minor
//!   versions. The defaults are chosen to hold the detector's
//!   precision-by-construction contract; tune only when a specific input
//!   fails and you have evidence for the change.
//!
//! `advanced` is `Option`-wrapped and serialized as a nested `"advanced"`
//! object (it is **not** flattened). When unset, the serialized config carries
//! only the four stable top-level keys and no `"advanced"` key, and detection
//! behaves exactly as if every advanced knob held its [`Default`] value —
//! [`DetectorParams::effective_tuning`] returns an owned
//! [`AdvancedTuning::default()`] in that case.

mod advanced;

pub use advanced::AdvancedTuning;

use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// Which graph-build algorithm to run.
///
/// The detector ships two grid builders side by side. Both produce the
/// same `(i, j) → corner` labelling, so downstream consumers stay agnostic
/// to the choice.
///
/// - [`SeedAndGrow`](GraphBuildAlgorithm::SeedAndGrow) — the **default**.
///   Finds a self-consistent 4-corner seed, then grows the grid outward
///   (axis clustering → cell-size estimate → seed → BFS grow → validate →
///   boosters). Robust across all four target families, and pinned for
///   ChArUco because non-uniform marker cells defeat the topological
///   cell test.
/// - [`Topological`](GraphBuildAlgorithm::Topological) — **opt-in**. A
///   Delaunay triangulation plus an axis-driven cell test (the image-free
///   variant of the SBF09 grid finder; see
///   [`projective_grid::TopologicalParams`]). Lower setup cost and no
///   global cell-size dependency, which helps on severe radial distortion
///   and low view angles. Select it per call via
///   [`DetectorParams::graph_build_algorithm`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum GraphBuildAlgorithm {
    /// Delaunay-triangulation + axis-driven cell-test grid builder.
    /// Opt-in; lower setup cost and no global cell-size dependency.
    Topological,
    /// Self-consistent 4-corner seed plus BFS grow; the default builder,
    /// robust across all four target families.
    #[default]
    SeedAndGrow,
}

fn default_graph_build_algorithm() -> GraphBuildAlgorithm {
    GraphBuildAlgorithm::default()
}

/// Top-level detector configuration.
///
/// A small **stable core** of four knobs a calibration consumer has a basis to
/// set, plus an opt-in, unstable [`AdvancedTuning`] sub-struct
/// ([`advanced`](Self::advanced)) holding the ~40 per-stage tuning knobs.
///
/// The four stable fields are part of the public configuration contract and
/// serialize as top-level JSON keys (`graph_build_algorithm`,
/// `min_labeled_corners`, `max_components`, `min_corner_strength`). The
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
/// use calib_targets_chessboard::{AdvancedTuning, DetectorParams};
///
/// // `AdvancedTuning` is `#[non_exhaustive]`, so build from `default()` and
/// // mutate the knobs you need rather than using struct-literal syntax.
/// let mut advanced = AdvancedTuning::default();
/// advanced.cluster_tol_deg = 9.0;
/// let params = DetectorParams::default().with_advanced(advanced);
/// ```
#[non_exhaustive]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DetectorParams {
    /// Which graph-build algorithm to run. See [`GraphBuildAlgorithm`].
    /// Default: [`GraphBuildAlgorithm::SeedAndGrow`].
    #[serde(default = "default_graph_build_algorithm")]
    pub graph_build_algorithm: GraphBuildAlgorithm,

    /// Minimum labelled corners for a
    /// [`ChessboardDetection`](crate::ChessboardDetection) to be emitted.
    pub min_labeled_corners: usize,

    /// Maximum number of components returned by [`crate::Detector::detect_all`].
    ///
    /// A chessboard can split into multiple disconnected pieces on ChArUco
    /// scenes where markers break contiguity. Each iteration peels off one
    /// grown grid from the unconsumed corners and re-runs seed → grow →
    /// validate. Default `3`.
    ///
    /// Does NOT claim to support scenes with two separate physical boards —
    /// one target per frame is the contract.
    pub max_components: u32,

    /// Minimum corner strength (ChESS response) for the Stage-1 pre-filter.
    /// Corners with `strength < min_corner_strength` are dropped before
    /// clustering. `0.0` (the default) disables the filter.
    ///
    /// Part of the stable configuration core. Serializes as the top-level
    /// `min_corner_strength` key.
    #[serde(default)]
    pub min_corner_strength: f32,

    /// Opt-in, **unstable** per-stage tuning knobs. Leave unset (`None`)
    /// unless a specific input fails and you have evidence for the change;
    /// `None` behaves exactly like [`AdvancedTuning::default()`]. Set via
    /// [`with_advanced`](Self::with_advanced). See [`AdvancedTuning`] — its
    /// fields are NOT covered by semver.
    ///
    /// Serialized under a nested `"advanced"` object when `Some`, and omitted
    /// entirely when `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub advanced: Option<Box<AdvancedTuning>>,
}

impl Default for DetectorParams {
    fn default() -> Self {
        Self {
            graph_build_algorithm: GraphBuildAlgorithm::default(),
            min_labeled_corners: 8,
            max_components: 3,
            min_corner_strength: 0.0,
            advanced: None,
        }
    }
}

impl DetectorParams {
    /// Attach an [`AdvancedTuning`] override and return the updated params.
    ///
    /// The advanced knobs are NOT covered by semver — see [`AdvancedTuning`].
    /// Leaving them unset (the default) keeps detection on the
    /// precision-by-construction defaults.
    #[must_use]
    pub fn with_advanced(mut self, tuning: AdvancedTuning) -> Self {
        self.advanced = Some(Box::new(tuning));
        self
    }

    /// The advanced tuning the detector will actually use.
    ///
    /// Returns [`Cow::Borrowed`] when [`advanced`](Self::advanced) is set, and
    /// an owned [`AdvancedTuning::default()`] otherwise. Internal pipeline
    /// stages bind this once at the top of a function and read fields off it,
    /// so the default case allocates a single struct (no per-knob branching)
    /// and the configured case borrows without copying.
    #[must_use]
    pub fn effective_tuning(&self) -> Cow<'_, AdvancedTuning> {
        match &self.advanced {
            Some(tuning) => Cow::Borrowed(tuning.as_ref()),
            None => Cow::Owned(AdvancedTuning::default()),
        }
    }

    /// Convenience preset for the topological graph builder.
    ///
    /// Equivalent to `DetectorParams { graph_build_algorithm:
    /// GraphBuildAlgorithm::Topological, ..DetectorParams::default() }`.
    /// Useful for examples and one-off experiments where the caller wants
    /// the Delaunay/topological path without spelling out the full struct
    /// update.
    pub fn topological() -> Self {
        Self {
            graph_build_algorithm: GraphBuildAlgorithm::Topological,
            ..Self::default()
        }
    }

    /// Three-config sweep preset: default + tighter + looser angular tolerances.
    ///
    /// Intended for `detect_chessboard_best`-style flows that try multiple
    /// configurations and return the result with the most labelled corners.
    /// All three configurations preserve the detector's
    /// precision-by-construction invariants; only recall-affecting
    /// tolerances are varied. The two non-default configurations carry an
    /// [`AdvancedTuning`] override.
    pub fn sweep_default() -> Vec<Self> {
        let base = Self::default();
        let base_tuning = base.effective_tuning().into_owned();
        let tight = base.clone().with_advanced(AdvancedTuning {
            cluster_tol_deg: 9.0,
            seed_edge_tol: 0.18,
            attach_axis_tol_deg: 12.0,
            ..base_tuning.clone()
        });
        let loose = base.clone().with_advanced(AdvancedTuning {
            cluster_tol_deg: 16.0,
            seed_edge_tol: 0.32,
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
        let configs = DetectorParams::sweep_default();
        assert_eq!(configs.len(), 3);
        let base = configs[0].effective_tuning();
        let tight = configs[1].effective_tuning();
        let loose = configs[2].effective_tuning();
        assert!(tight.cluster_tol_deg < base.cluster_tol_deg);
        assert!(loose.cluster_tol_deg > base.cluster_tol_deg);
        assert!(tight.seed_edge_tol < base.seed_edge_tol);
        assert!(loose.seed_edge_tol > base.seed_edge_tol);
    }

    #[test]
    fn topological_preset_only_changes_graph_builder() {
        let topo = DetectorParams::topological();
        let default = DetectorParams::default();
        assert_eq!(topo.graph_build_algorithm, GraphBuildAlgorithm::Topological);
        assert_eq!(
            default.graph_build_algorithm,
            GraphBuildAlgorithm::SeedAndGrow
        );
        assert_eq!(
            topo.effective_tuning().topological.axis_align_tol_rad,
            default.effective_tuning().topological.axis_align_tol_rad
        );
        assert_eq!(topo.min_labeled_corners, default.min_labeled_corners);
    }

    #[test]
    fn effective_tuning_default_matches_advanced_default() {
        // `effective_tuning()` with `advanced: None` MUST be byte-identical
        // to `AdvancedTuning::default()` — this is the behaviour-preservation
        // contract for the opt-in split.
        let params = DetectorParams::default();
        let effective = params.effective_tuning();
        let expected = AdvancedTuning::default();
        assert_eq!(
            serde_json::to_value(effective.as_ref()).unwrap(),
            serde_json::to_value(&expected).unwrap()
        );
    }

    #[test]
    fn default_params_serialize_only_stable_keys() {
        // The default config must carry exactly the four stable top-level
        // keys, NO advanced knobs, and no `"advanced"` key.
        let value = serde_json::to_value(DetectorParams::default()).unwrap();
        let obj = value.as_object().expect("params serialize to an object");
        let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "graph_build_algorithm",
                "max_components",
                "min_corner_strength",
                "min_labeled_corners",
            ]
        );
        assert!(!obj.contains_key("advanced"));
        // None of the advanced knobs leaked to the top level.
        for leaked in [
            "cluster_tol_deg",
            "max_fit_rms_ratio",
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
        let params = DetectorParams::default().with_advanced(AdvancedTuning::default());
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
        let restored: DetectorParams = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(&restored).unwrap(), value);
        assert!(restored.advanced.is_some());
    }

    #[test]
    fn min_corner_strength_is_top_level_and_stable() {
        let params = DetectorParams {
            min_corner_strength: 0.5,
            ..DetectorParams::default()
        };
        let value = serde_json::to_value(&params).unwrap();
        assert_eq!(value["min_corner_strength"], serde_json::json!(0.5));
        // Deserializing a config that sets only the stable keys (and omits
        // `advanced`) reads `min_corner_strength` from the top level and
        // leaves `advanced` unset — i.e. the detector keeps default tuning.
        let restored: DetectorParams = serde_json::from_value(serde_json::json!({
            "min_corner_strength": 0.5,
            "min_labeled_corners": 8,
            "max_components": 3,
        }))
        .unwrap();
        assert_eq!(restored.min_corner_strength, 0.5);
        assert!(restored.advanced.is_none());
    }
}
