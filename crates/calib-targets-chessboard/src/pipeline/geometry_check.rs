//! Stage 12: the mandatory final geometry check.
//!
//! Runs after every other stage and can only remove corners or refuse
//! the detection — never add or relabel. This is the precision gate
//! mandated by `CLAUDE.md` ("Geometry check is mandatory before
//! returning a detection").

use std::collections::HashSet;

use super::cluster::ClusterCenters;
use crate::corner::{CornerAug, CornerStage};
use crate::params::DetectorParams;
use projective_grid::shared::grow::GrowResult;

use super::types::GeometryCheckTrace;

const MIN_EDGE_SHAPE_LABELS: usize = 40;

/// Mandatory final precision gate. Runs after every other stage and
/// can only remove corners or refuse the detection — never add or
/// relabel.
///
/// Drops any labelled corner that fails:
/// - the shared [`validate`](projective_grid::shared::validate::validate)
///   pass (line collinearity + local-H residual, attribution rules from
///   [`mod@projective_grid::shared::validate`]); **or**
/// - the direct local wrong-label check
///   ([`topological_wrong_label_drops`](projective_grid::shared::validate::wrong_label_filters::topological_wrong_label_drops)),
///   which targets the dominant topological wrong-label classes — interior
///   skipped-corner edges and duplicate-pixel labels; **or**
/// - the largest-cardinally-connected-component filter, which removes any
///   isolated false-positive label that sits outside the main grid.
///
/// `detection_refused` is set when the surviving labelled count
/// drops below `min_labeled_corners` — the caller MUST then return
/// `None` for the detection rather than ship a half-broken grid.
pub(crate) fn run_geometry_check(
    augs: &mut [CornerAug],
    grow_res: &mut GrowResult,
    _centers: ClusterCenters,
    cell_size: f32,
    blacklist: &mut HashSet<usize>,
    params: &DetectorParams,
) -> GeometryCheckTrace {
    use projective_grid::shared::validate as pg_validate;

    let tuning = params.effective_tuning();

    // Looser `geometry_check_*` tolerances than the topological walk: the
    // walk already accepted borderline perspective drift; the geometry
    // check's job is to catch gross mislabels (full-cell or diagonal
    // shifts) only. (Per-edge axis-slot-swap was tried and rejected — too
    // rigid for heavily distorted boards; the local-H residual in
    // `validate` handles the diagonal-mislabel case without touching
    // legitimate perspective-distorted corners.)
    let mut geom_params = pg_validate::ValidationParams::new(
        tuning.geometry_check_line_tol_rel,
        tuning.line_min_members,
        tuning.geometry_check_local_h_tol_rel,
    );
    if tuning.validate_step_aware {
        // Geometry check stays step-aware so heavily distorted boards get
        // the same scale-relative thresholds as the walk. The
        // step-deviation gate is disabled here (set to 0).
        geom_params = geom_params.with_step_aware(0.0);
    }

    // The validate → wrong-label → largest-component composition (and its
    // deterministic input ordering) lives in the shared `drop_set` helper —
    // the same path the geometry-only recovery schedule routes through. Only
    // the chessboard-specific bookkeeping below (stage machine, blacklist,
    // refusal threshold) stays here. The direct wrong-label check needs a
    // dense enough grid to be meaningful, so it stays gated on the
    // chessboard's label count.
    let dense_enough = grow_res.labelled.len() >= MIN_EDGE_SHAPE_LABELS;
    let result = pg_validate::wrong_label_filters::drop_set(
        &grow_res.labelled,
        |idx| augs[idx].position,
        cell_size,
        &geom_params,
        tuning.enable_final_edge_shape_check && dense_enough,
        true,
    );

    let dropped_validate = result.validate_drop.len() as u32;
    let dropped_edge_only = result.wrong_label_drop.len() as u32;
    let dropped_disconnected = result.component_drop.len() as u32;
    let components_seen = result.components_seen;

    for &idx in &result.drop {
        if let CornerStage::Labeled { at, .. } = augs[idx].stage {
            augs[idx].stage = CornerStage::LabeledThenBlacklisted {
                at,
                reason: "geometry-check".into(),
            };
        }
        grow_res.labelled.retain(|_, &mut v| v != idx);
        grow_res.by_corner.remove(&idx);
        blacklist.insert(idx);
    }

    let detection_refused = grow_res.labelled.len() < params.min_labeled_corners;
    GeometryCheckTrace {
        dropped: result.drop.len() as u32,
        dropped_line_collinearity: dropped_validate,
        dropped_local_h_residual: 0, // shared validator lumps these — kept for forward-compat
        dropped_edge_invariant: dropped_edge_only,
        dropped_disconnected,
        components_seen,
        detection_refused,
    }
}
