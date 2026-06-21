//! Param-schema catalogue: human-facing metadata (section, label, help,
//! value-kind, gating) for every editable `AdvancedTuning` knob, served to the
//! Studio Config tab so the advanced-params form renders grouped, labelled,
//! tooltipped fields instead of raw snake_case JSON keys.
//!
//! This is a **hand-maintained mirror** of
//! [`AdvancedTuning`](calib_targets::chessboard) — the same drift contract as
//! the WASM typings (`crates/calib-targets-wasm/typescript-extras.d.ts`). The
//! `every_advanced_leaf_has_metadata` test walks the materialised
//! `advanced` tree (the exact JSON `/api/configs/_defaults` returns) and fails
//! if any editable scalar knob lacks a catalogue entry, so a newly-added knob
//! cannot silently render unlabelled. `help` is **distilled** from each field's
//! rustdoc — a one-line tooltip, not the verbatim multi-paragraph rationale.
//!
//! Pointers are RFC-6901 JSON pointers into the materialised `DetectorParams`
//! (`/advanced/<name>`, `/advanced/topological/<name>`,
//! `/advanced/component_merge/<name>`) — the same pointer space the bench
//! ablation catalogue (`calib_targets_bench::ablate`) uses. The four stable
//! core fields (`graph_build_algorithm`, …) keep their bespoke hand-labelled
//! controls in the Config tab and are intentionally not in this catalogue.

use axum::Json;
use serde::Serialize;

/// Editable value kind; drives the input widget the frontend renders.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ParamKind {
    /// Checkbox (`bool`).
    Bool,
    /// Integer number input (`usize` / `u32`).
    Int,
    /// Floating-point number input (`f32`).
    Float,
}

/// One section of the param form, in display order.
#[derive(Serialize)]
pub struct ParamGroup {
    /// Stable id referenced by [`ParamField::group`].
    pub id: &'static str,
    /// Human-readable section heading.
    pub title: &'static str,
}

/// Metadata for one editable knob, keyed by its JSON pointer into the
/// materialised `DetectorParams`.
#[derive(Serialize)]
pub struct ParamField {
    /// RFC-6901 JSON pointer into the materialised params (e.g.
    /// `/advanced/cluster_tol_deg`).
    pub pointer: &'static str,
    /// [`ParamGroup::id`] this field belongs to.
    pub group: &'static str,
    /// Human-readable label.
    pub label: &'static str,
    /// One-line tooltip distilled from the field's rustdoc.
    pub help: &'static str,
    /// Value kind for the input widget.
    pub kind: ParamKind,
    /// Pointer of the boolean flag that gates this knob, if any. The frontend
    /// greys the field out (without clearing its value) when the parent is
    /// `false`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gated_by: Option<&'static str>,
}

/// Full schema payload returned by [`schema`].
#[derive(Serialize)]
pub struct ParamSchema {
    /// Sections in display order.
    pub groups: Vec<ParamGroup>,
    /// Field metadata (unordered; the frontend buckets by `group`).
    pub fields: Vec<ParamField>,
}

const fn field(
    pointer: &'static str,
    group: &'static str,
    label: &'static str,
    help: &'static str,
    kind: ParamKind,
    gated_by: Option<&'static str>,
) -> ParamField {
    ParamField {
        pointer,
        group,
        label,
        help,
        kind,
        gated_by,
    }
}

/// The committed advanced-param schema. One entry per editable scalar knob of
/// `AdvancedTuning` (its nested `topological` / `component_merge` leaves
/// included); the non-scalar `topological.axis_cluster_centers`
/// (`Option<[f32; 2]>`, default `null`) is intentionally omitted — it has no
/// scalar widget and falls through to the frontend's raw-key fallback.
fn catalogue() -> ParamSchema {
    use ParamKind::{Bool, Float, Int};

    let groups = vec![
        ParamGroup {
            id: "prefilter",
            title: "Prefilter",
        },
        ParamGroup {
            id: "cluster_axes",
            title: "Axis clustering",
        },
        ParamGroup {
            id: "grow",
            title: "BFS grow",
        },
        ParamGroup {
            id: "validate",
            title: "Validation",
        },
        ParamGroup {
            id: "geometry_check",
            title: "Geometry check",
        },
        ParamGroup {
            id: "boosters",
            title: "Recall boosters",
        },
        ParamGroup {
            id: "topological",
            title: "Topological grid build",
        },
        ParamGroup {
            id: "component_merge",
            title: "Component merge",
        },
    ];

    let fields = vec![
        // --- prefilter ------------------------------------------------------
        field(
            "/advanced/max_fit_rms_ratio",
            "prefilter",
            "Max fit RMS ratio",
            "Drop ChESS corners whose fit RMS exceeds this ratio times their contrast. Higher admits weaker corners; ∞ disables the prefilter.",
            Float,
            None,
        ),
        // --- cluster_axes ---------------------------------------------------
        field(
            "/advanced/num_bins",
            "cluster_axes",
            "Histogram bins",
            "Histogram bins over [0, π) used to seed the two grid-axis directions.",
            Int,
            None,
        ),
        field(
            "/advanced/max_iters_2means",
            "cluster_axes",
            "2-means iterations",
            "Maximum 2-means refinement iterations for the axis-direction clustering.",
            Int,
            None,
        ),
        field(
            "/advanced/cluster_tol_deg",
            "cluster_axes",
            "Cluster tolerance (°)",
            "Per-axis tolerance (deg) for a corner's axis to match a cluster centre.",
            Float,
            None,
        ),
        field(
            "/advanced/cluster_sigma_k",
            "cluster_axes",
            "Cluster σ multiplier",
            "Multiplier on each corner's axis uncertainty added to the cluster tolerance (0 = fixed tolerance).",
            Float,
            None,
        ),
        field(
            "/advanced/peak_min_separation_deg",
            "cluster_axes",
            "Peak min separation (°)",
            "Minimum angular separation (deg) between the two axis peaks; true grid axes are ~90° apart.",
            Float,
            None,
        ),
        field(
            "/advanced/min_peak_weight_fraction",
            "cluster_axes",
            "Min peak weight fraction",
            "Minimum fraction of total axis-vote weight for a histogram peak to be considered.",
            Float,
            None,
        ),
        // --- grow -----------------------------------------------------------
        field(
            "/advanced/attach_search_rel",
            "grow",
            "Attach search radius",
            "Candidate-search radius around a predicted cell, as a fraction of cell size.",
            Float,
            None,
        ),
        field(
            "/advanced/attach_axis_tol_deg",
            "grow",
            "Attach axis tolerance (°)",
            "Axis-alignment tolerance (deg) when attaching a corner during BFS grow.",
            Float,
            None,
        ),
        field(
            "/advanced/attach_ambiguity_factor",
            "grow",
            "Attach ambiguity factor",
            "Skip an attachment if the second-nearest candidate is within this factor of the nearest.",
            Float,
            None,
        ),
        field(
            "/advanced/step_tol",
            "grow",
            "Step tolerance",
            "Edge-length window (fraction of cell size) enforced on new edges during grow.",
            Float,
            None,
        ),
        field(
            "/advanced/edge_axis_tol_deg",
            "grow",
            "Edge axis tolerance (°)",
            "Edge axis-direction tolerance (deg) enforced when admitting a grow edge.",
            Float,
            None,
        ),
        // --- validate -------------------------------------------------------
        field(
            "/advanced/line_min_members",
            "validate",
            "Line min members",
            "Minimum corners required to fit a row/column line for collinearity checks.",
            Int,
            None,
        ),
        field(
            "/advanced/validate_step_aware",
            "validate",
            "Step-aware validation",
            "Scale validation tolerances by a per-corner local step instead of the global cell size (anisotropic).",
            Bool,
            None,
        ),
        // --- geometry_check -------------------------------------------------
        field(
            "/advanced/geometry_check_line_tol_rel",
            "geometry_check",
            "Geometry line tolerance",
            "Line-collinearity tolerance (fraction of cell size) for the mandatory final geometry check; looser than validation.",
            Float,
            None,
        ),
        field(
            "/advanced/geometry_check_local_h_tol_rel",
            "geometry_check",
            "Geometry local-H tolerance",
            "Local-homography residual tolerance (fraction of cell size) for the final geometry check.",
            Float,
            None,
        ),
        field(
            "/advanced/enable_final_edge_shape_check",
            "geometry_check",
            "Final edge-shape check",
            "Final local edge-shape gate (cardinal support, edge continuation, opposite-side consistency) for standalone detections.",
            Bool,
            None,
        ),
        // --- boosters -------------------------------------------------------
        field(
            "/advanced/enable_weak_cluster_rescue",
            "boosters",
            "Enable weak-cluster rescue",
            "Re-admit corners that clustered only within the looser weak-cluster tolerance as recall-booster candidates.",
            Bool,
            None,
        ),
        field(
            "/advanced/weak_cluster_tol_deg",
            "boosters",
            "Weak-cluster tolerance (°)",
            "Cluster tolerance (deg) for weakly-clustered booster candidates; must be ≥ the cluster tolerance.",
            Float,
            Some("/advanced/enable_weak_cluster_rescue"),
        ),
        field(
            "/advanced/max_booster_iters",
            "boosters",
            "Max booster iterations",
            "Cap on the outer recall-booster loop.",
            Int,
            Some("/advanced/enable_weak_cluster_rescue"),
        ),
        // --- topological (nested) ------------------------------------------
        field(
            "/advanced/topological/axis_align_tol_rad",
            "topological",
            "Axis-align tolerance (rad)",
            "Max angle (rad) between an edge direction and a corner axis for the edge to count as a grid edge.",
            Float,
            None,
        ),
        field(
            "/advanced/topological/max_axis_sigma_rad",
            "topological",
            "Max axis σ (rad)",
            "Max 1σ axis uncertainty (rad) for an axis to be treated as informative.",
            Float,
            None,
        ),
        field(
            "/advanced/topological/opposing_edge_ratio_max",
            "topological",
            "Opposing-edge ratio max",
            "Reject quads whose opposing edges differ in length by more than this factor.",
            Float,
            None,
        ),
        field(
            "/advanced/topological/edge_length_min_rel",
            "topological",
            "Edge length min (rel)",
            "Lower bound on a quad edge length as a fraction of the component median (0 disables).",
            Float,
            None,
        ),
        field(
            "/advanced/topological/edge_length_max_rel",
            "topological",
            "Edge length max (rel)",
            "Upper bound on a quad edge length as a fraction of the component median (∞ disables).",
            Float,
            None,
        ),
        field(
            "/advanced/topological/min_corners_for_component",
            "topological",
            "Min corners / component",
            "Discard labelled components with fewer than this many corners.",
            Int,
            None,
        ),
        field(
            "/advanced/topological/min_quads_per_component",
            "topological",
            "Min quads / component",
            "Discard quad-mesh components below this many quads.",
            Int,
            None,
        ),
        field(
            "/advanced/topological/cluster_axis_tol_rad",
            "topological",
            "Cluster axis tolerance (rad)",
            "Per-axis tolerance (rad) against optional global axis centres (only used when centres are set).",
            Float,
            None,
        ),
        // --- component_merge (nested) --------------------------------------
        field(
            "/advanced/component_merge/position_tol_rel",
            "component_merge",
            "Position tolerance",
            "Position tolerance for treating two corners as the same point, as a fraction of cell size.",
            Float,
            None,
        ),
        field(
            "/advanced/component_merge/cell_size_ratio_tol",
            "component_merge",
            "Cell-size ratio tolerance",
            "Max relative cell-size disagreement between two components before a merge is attempted.",
            Float,
            None,
        ),
        field(
            "/advanced/component_merge/min_overlap",
            "component_merge",
            "Min overlap",
            "Minimum overlapping labels (after alignment) required to accept a merge.",
            Int,
            None,
        ),
        field(
            "/advanced/component_merge/max_components",
            "component_merge",
            "Max components",
            "Upper bound on the number of components kept after merging.",
            Int,
            None,
        ),
    ];

    ParamSchema { groups, fields }
}

/// `GET /api/params/schema` — the advanced-param UI metadata catalogue.
pub async fn schema() -> Json<ParamSchema> {
    Json(catalogue())
}

#[cfg(test)]
mod tests {
    use super::*;
    use calib_targets::chessboard::DetectorParams;
    use std::collections::HashSet;

    /// Materialise the exact JSON `/api/configs/_defaults` (no family) returns:
    /// the bare chessboard defaults with the full `advanced` block present.
    fn materialised_defaults() -> serde_json::Value {
        let chess = DetectorParams::default();
        let advanced = chess.effective_tuning().into_owned();
        serde_json::to_value(chess.with_advanced(advanced)).expect("serialize DetectorParams")
    }

    /// Collect the JSON pointers of every form-editable scalar leaf (bool /
    /// number) under `advanced`, recursing one level into nested objects
    /// (`topological` / `component_merge`). `null` / array / object values are
    /// skipped — they have no scalar widget.
    fn editable_leaf_pointers(value: &serde_json::Value) -> HashSet<String> {
        let mut out = HashSet::new();
        let Some(advanced) = value.get("advanced").and_then(|a| a.as_object()) else {
            panic!("materialised defaults missing an `advanced` object");
        };
        for (key, val) in advanced {
            match val {
                serde_json::Value::Object(sub) => {
                    for (subkey, subval) in sub {
                        if is_scalar(subval) {
                            out.insert(format!("/advanced/{key}/{subkey}"));
                        }
                    }
                }
                v if is_scalar(v) => {
                    out.insert(format!("/advanced/{key}"));
                }
                _ => {}
            }
        }
        out
    }

    fn is_scalar(v: &serde_json::Value) -> bool {
        v.is_boolean() || v.is_number()
    }

    /// Every editable scalar leaf has a catalogue entry, and the catalogue has
    /// no stale entries. This is the C4 drift contract: adding a knob to
    /// `AdvancedTuning` turns this test red until its metadata is supplied.
    #[test]
    fn every_advanced_leaf_has_metadata() {
        let defaults = materialised_defaults();
        let leaves = editable_leaf_pointers(&defaults);
        let schema = catalogue();
        let cataloged: HashSet<&str> = schema.fields.iter().map(|f| f.pointer).collect();

        let mut missing: Vec<&String> = leaves
            .iter()
            .filter(|p| !cataloged.contains(p.as_str()))
            .collect();
        missing.sort();
        assert!(
            missing.is_empty(),
            "advanced knobs with no param-schema metadata (add them to \
             params_schema::catalogue): {missing:?}"
        );

        let mut stale: Vec<&&str> = cataloged.iter().filter(|p| !leaves.contains(**p)).collect();
        stale.sort();
        assert!(
            stale.is_empty(),
            "param-schema entries that no longer resolve to an editable leaf \
             (remove or fix the pointer): {stale:?}"
        );
    }

    /// Every catalogued field's declared `kind` matches the JSON value at its
    /// pointer, and every `group` / `gated_by` resolves.
    #[test]
    fn metadata_is_internally_consistent() {
        let defaults = materialised_defaults();
        let schema = catalogue();
        let group_ids: HashSet<&str> = schema.groups.iter().map(|g| g.id).collect();
        let bool_pointers: HashSet<&str> = schema
            .fields
            .iter()
            .filter(|f| f.kind == ParamKind::Bool)
            .map(|f| f.pointer)
            .collect();

        for f in &schema.fields {
            assert!(
                group_ids.contains(f.group),
                "field {} references unknown group {:?}",
                f.pointer,
                f.group
            );
            let val = defaults
                .pointer(f.pointer)
                .unwrap_or_else(|| panic!("field {} does not resolve in defaults", f.pointer));
            let kind_ok = match f.kind {
                ParamKind::Bool => val.is_boolean(),
                ParamKind::Int => val.is_i64() || val.is_u64(),
                ParamKind::Float => val.is_f64(),
            };
            assert!(
                kind_ok,
                "field {} declared {:?} but JSON value is {val}",
                f.pointer, f.kind
            );
            if let Some(parent) = f.gated_by {
                assert!(
                    bool_pointers.contains(parent),
                    "field {} is gated_by {parent}, which is not a catalogued bool field",
                    f.pointer
                );
            }
        }
    }
}
