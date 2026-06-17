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
/// - [`Topological`](GraphBuildAlgorithm::Topological) — the **default**. A
///   Delaunay triangulation plus an axis-driven cell test (the image-free
///   variant of the SBF09 grid finder; see
///   [`projective_grid::TopologicalParams`]). Lower setup cost, no global
///   cell-size dependency, and higher recall on the clean-chessboard
///   regression set than seed-and-grow; it also tolerates severe radial
///   distortion and low view angles better.
/// - [`SeedAndGrow`](GraphBuildAlgorithm::SeedAndGrow) — finds a
///   self-consistent 4-corner seed, then grows the grid outward (axis
///   clustering → cell-size estimate → seed → BFS grow → validate →
///   boosters). Pinned for ChArUco because non-uniform marker cells defeat
///   the topological cell test; available elsewhere per call via
///   [`DetectorParams::graph_build_algorithm`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum GraphBuildAlgorithm {
    /// Delaunay-triangulation + axis-driven cell-test grid builder; the
    /// default builder. Lower setup cost, no global cell-size dependency.
    #[default]
    Topological,
    /// Self-consistent 4-corner seed plus BFS grow. Pinned for ChArUco
    /// (non-uniform marker cells defeat the topological cell test).
    SeedAndGrow,
}

fn default_graph_build_algorithm() -> GraphBuildAlgorithm {
    GraphBuildAlgorithm::default()
}

/// Where the topological grid builder gets each corner's two local grid
/// directions.
///
/// **Experimental — NOT covered by semver.** Today this only affects the
/// [`Topological`](GraphBuildAlgorithm::Topological) builder: the native
/// [`SeedAndGrow`](GraphBuildAlgorithm::SeedAndGrow) pipeline consumes ChESS
/// corner axes directly throughout its seed / grow / validate / booster
/// stages and cannot run orientation-free. Pairing
/// [`NeighbourEdges`](OrientationSource::NeighbourEdges) with `SeedAndGrow`
/// is a typed [`ChessboardParamsError::NeighbourEdgesRequiresTopological`]
/// (surfaced by [`DetectorParams::validate`] / [`crate::Detector::new`])
/// rather than a silent fallback to ChESS axes (which would make a head-to-head
/// measurement secretly compare the wrong thing).
///
/// Default: [`ChessAxes`](OrientationSource::ChessAxes) — detection behaves
/// exactly as before unless a caller opts in, and the default value is omitted
/// from serialization so the stable config surface is unchanged.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum OrientationSource {
    /// Use the per-corner ChESS axis estimates carried by each
    /// [`ChessCorner`](crate::ChessCorner). The production default.
    #[default]
    ChessAxes,
    /// Ignore the ChESS axes and synthesize each corner's two local grid
    /// directions from neighbour-edge geometry (`projective_grid`'s
    /// `synthesize_oriented2`). Topological builder only.
    NeighbourEdges,
}

fn default_orientation_source() -> OrientationSource {
    OrientationSource::default()
}

fn default_min_labeled_corners() -> usize {
    8
}

fn default_max_components() -> u32 {
    3
}

fn default_min_corner_strength() -> f32 {
    33.0
}

/// A [`DetectorParams`] configuration the chessboard detector cannot honour.
///
/// Returned by [`DetectorParams::validate`] / [`crate::Detector::new`]. The
/// only current variant is the orientation-source / graph-builder mismatch that
/// previously panicked at runtime; the enum is `#[non_exhaustive]` so future
/// validations can be added without a breaking change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ChessboardParamsError {
    /// [`OrientationSource::NeighbourEdges`] was paired with
    /// [`GraphBuildAlgorithm::SeedAndGrow`]. The native seed-and-grow pipeline
    /// consumes ChESS corner axes directly throughout its seed / grow /
    /// validate / booster stages and cannot run orientation-free; honouring
    /// `NeighbourEdges` there would silently fall back to ChESS axes (a
    /// head-to-head measurement would then secretly compare the wrong thing).
    /// Use [`GraphBuildAlgorithm::Topological`] for the orientation-free path.
    NeighbourEdgesRequiresTopological,
}

impl core::fmt::Display for ChessboardParamsError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NeighbourEdgesRequiresTopological => f.write_str(
                "OrientationSource::NeighbourEdges is only supported with \
                 GraphBuildAlgorithm::Topological; the native SeedAndGrow pipeline \
                 consumes ChESS corner axes directly and cannot run orientation-free",
            ),
        }
    }
}

impl std::error::Error for ChessboardParamsError {}

fn is_default_orientation_source(value: &OrientationSource) -> bool {
    *value == OrientationSource::default()
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
    /// Default: [`GraphBuildAlgorithm::Topological`].
    #[serde(default = "default_graph_build_algorithm")]
    pub graph_build_algorithm: GraphBuildAlgorithm,

    /// **Experimental:** where the topological builder gets per-corner grid
    /// directions. See [`OrientationSource`]. Default
    /// [`OrientationSource::ChessAxes`]; only affects the
    /// [`Topological`](GraphBuildAlgorithm::Topological) builder. Omitted from
    /// serialization at its default so the stable config surface is unchanged.
    #[serde(
        default = "default_orientation_source",
        skip_serializing_if = "is_default_orientation_source"
    )]
    pub orientation_source: OrientationSource,

    /// Minimum labelled corners for a
    /// [`ChessboardDetection`](crate::ChessboardDetection) to be emitted.
    /// Default `8`; defaulted on deserialization so partial configs (and
    /// legacy configs that omit it) keep parsing.
    #[serde(default = "default_min_labeled_corners")]
    pub min_labeled_corners: usize,

    /// Maximum number of components returned by [`crate::Detector::detect_all`].
    ///
    /// A chessboard can split into multiple disconnected pieces on ChArUco
    /// scenes where markers break contiguity. Each iteration peels off one
    /// grown grid from the unconsumed corners and re-runs seed → grow →
    /// validate. Default `3`; defaulted on deserialization so partial configs
    /// keep parsing.
    ///
    /// Does NOT claim to support scenes with two separate physical boards —
    /// one target per frame is the contract.
    #[serde(default = "default_max_components")]
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
    /// Part of the stable configuration core. Serializes as the top-level
    /// `min_corner_strength` key.
    #[serde(default = "default_min_corner_strength")]
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
            orientation_source: OrientationSource::default(),
            min_labeled_corners: 8,
            max_components: 3,
            min_corner_strength: default_min_corner_strength(),
            advanced: None,
        }
    }
}

impl DetectorParams {
    /// Validate the configuration, returning a typed error for any combination
    /// the detector cannot honour.
    ///
    /// [`OrientationSource::NeighbourEdges`] is **topological-only**. It is
    /// rejected with [`GraphBuildAlgorithm::SeedAndGrow`] (see
    /// [`ChessboardParamsError::NeighbourEdgesRequiresTopological`]) because the
    /// seed-and-grow seed finder stakes the whole grid frame on ~4 seed corners'
    /// axes, and a measured head-to-head (2026-06-17) showed synthesized axes
    /// collapse it — 0 corners on 3 of 6 clutter-free frames, 19 vs 373 on a
    /// dense board — whereas the topological builder, which labels connected
    /// components from many local edge classifications, tolerates the noise. The
    /// fallible constructor [`crate::Detector::new`] calls this up front and
    /// surfaces the typed error, so the `detect*` methods always run on a
    /// validated configuration.
    pub fn validate(&self) -> Result<(), ChessboardParamsError> {
        if matches!(self.graph_build_algorithm, GraphBuildAlgorithm::SeedAndGrow)
            && matches!(self.orientation_source, OrientationSource::NeighbourEdges)
        {
            return Err(ChessboardParamsError::NeighbourEdgesRequiresTopological);
        }
        Ok(())
    }

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
    fn topological_preset_matches_the_default_builder() {
        // Topological is now the default builder, so the `topological()`
        // preset agrees with `default()` on the algorithm and leaves the
        // tuning / labelled-corner knobs untouched.
        let topo = DetectorParams::topological();
        let default = DetectorParams::default();
        assert_eq!(topo.graph_build_algorithm, GraphBuildAlgorithm::Topological);
        assert_eq!(
            default.graph_build_algorithm,
            GraphBuildAlgorithm::Topological
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

    #[test]
    fn orientation_source_omitted_at_default_round_trips_when_set() {
        // Default (ChessAxes) is skipped → the stable 4-key surface is unchanged
        // (also asserted by `default_params_serialize_only_stable_keys`).
        let default = serde_json::to_value(DetectorParams::default()).unwrap();
        assert!(!default
            .as_object()
            .unwrap()
            .contains_key("orientation_source"));

        // Explicitly set NeighbourEdges → serializes as snake_case + round-trips.
        let params = DetectorParams {
            orientation_source: OrientationSource::NeighbourEdges,
            ..DetectorParams::default()
        };
        let value = serde_json::to_value(&params).unwrap();
        assert_eq!(
            value["orientation_source"],
            serde_json::json!("neighbour_edges")
        );
        let restored: DetectorParams = serde_json::from_value(value).unwrap();
        assert_eq!(
            restored.orientation_source,
            OrientationSource::NeighbourEdges
        );

        // A config that omits the key deserializes back to the ChessAxes default.
        let restored: DetectorParams = serde_json::from_value(serde_json::json!({
            "graph_build_algorithm": "topological",
            "min_labeled_corners": 8,
            "max_components": 3,
        }))
        .unwrap();
        assert_eq!(restored.orientation_source, OrientationSource::ChessAxes);
    }
}
