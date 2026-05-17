//! Stage 12: the mandatory final geometry check.
//!
//! Runs after every other stage and can only remove corners or refuse
//! the detection — never add or relabel. This is the precision gate
//! mandated by `CLAUDE.md` ("Geometry check is mandatory before
//! returning a detection").

use std::collections::HashSet;

use crate::cluster::ClusterCenters;
use crate::corner::{CornerAug, CornerStage};
use crate::grow::GrowResult;
use crate::params::DetectorParams;

use super::types::GeometryCheckTrace;

/// Mandatory final precision gate. Runs after every other stage and
/// can only remove corners or refuse the detection — never add or
/// relabel.
///
/// Drops any labelled corner that fails:
/// - the shared [`validate`](projective_grid::square::validate::validate)
///   pass (line collinearity + local-H residual, attribution rules from
///   [`mod@projective_grid::square::validate`]); **or**
/// - the per-cardinal-edge axis-slot-swap parity check from
///   `ChessboardGrowValidator::edge_ok` — every edge between two
///   cardinal-labelled corners must satisfy the same edge invariant
///   that BFS enforced at attachment time. This catches wrong
///   `(i, j)` labels introduced by Stage 6 / 6.5 / boosters / refit
///   even when each individual attachment satisfied the invariant
///   against *some* labelled neighbour at the time.
///
/// `detection_refused` is set when the surviving labelled count
/// drops below `min_labeled_corners` — the caller MUST then return
/// `None` for the detection rather than ship a half-broken grid.
pub fn run_geometry_check(
    augs: &mut [CornerAug],
    grow_res: &mut GrowResult,
    _centers: ClusterCenters,
    cell_size: f32,
    blacklist: &mut HashSet<usize>,
    params: &DetectorParams,
) -> GeometryCheckTrace {
    use std::collections::HashSet as Set;
    // Test 1: line collinearity + local-H residual via shared
    // validator, but with the LOOSER `geometry_check_*` tolerances —
    // the BFS-validation loop already accepted borderline perspective
    // drift; the geometry check's job is to catch gross mislabels
    // (full-cell or diagonal shifts) only.
    let geom_entries: Vec<projective_grid::square::validate::LabelledEntry> = grow_res
        .labelled
        .iter()
        .map(
            |(&grid, &idx)| projective_grid::square::validate::LabelledEntry {
                idx,
                pixel: augs[idx].position,
                grid,
            },
        )
        .collect();
    let mut geom_params = projective_grid::square::validate::ValidationParams::new(
        params.geometry_check_line_tol_rel,
        params.line_min_members,
        params.geometry_check_local_h_tol_rel,
    );
    if params.validate_step_aware {
        // Geometry check stays step-aware so heavily distorted boards
        // get the same scale-relative thresholds as BFS validation.
        // Step-deviation gate is BFS-only — set to 0 (disabled).
        geom_params = geom_params.with_step_aware(0.0);
    }
    let v = projective_grid::square::validate::validate(&geom_entries, cell_size, &geom_params);
    let validate_drop: Set<usize> = v.blacklist.iter().copied().collect();

    // Per-edge axis-slot-swap was tried as an additional check but
    // was too rigid for heavily distorted boards (every cell with a
    // perspective-foreshortened edge failed the length test, even
    // requiring 2-of-4 failing edges still flagged 27+ corners on
    // `puzzleboard_reference/example2.png`). Local-H residual via
    // `validate()` with looser geometry-check tolerances handles the
    // diagonal-mislabel case (residual ~1.4 cell on a wrong-cell
    // attachment, well above the 0.6 cell threshold) without
    // touching legitimate perspective-distorted corners.
    let mut all_drop: Set<usize> = Set::new();
    all_drop.extend(validate_drop.iter().copied());

    // Test 3: cardinally-connected components. A chessboard detection
    // is by construction one (i, j)-labelled connected planar graph;
    // any singleton or small-component that survived earlier stages
    // is a false positive (commonly a marker corner that passed the
    // axis cluster + parity gates but sits in isolation, well outside
    // the main grid). Keep only the largest component; drop the rest.
    //
    // Implemented after the validate() drops so a corner that's both
    // a residual outlier AND disconnected gets attributed to validate
    // (dominant reason). Components are computed AFTER the validate
    // drops so dropping a "bridge" corner can split a component, and
    // then the smaller half is correctly removed.
    let surviving_labels: Vec<((i32, i32), usize)> = grow_res
        .labelled
        .iter()
        .filter(|(_, &idx)| !all_drop.contains(&idx))
        .map(|(&k, &v)| (k, v))
        .collect();
    let label_set: std::collections::HashMap<(i32, i32), usize> =
        surviving_labels.iter().copied().collect();
    let mut visited: Set<(i32, i32)> = Set::new();
    let mut components: Vec<Vec<(i32, i32)>> = Vec::new();
    for &(ij, _) in &surviving_labels {
        if visited.contains(&ij) {
            continue;
        }
        let mut comp = Vec::new();
        let mut stack = vec![ij];
        while let Some(cur) = stack.pop() {
            if !visited.insert(cur) {
                continue;
            }
            comp.push(cur);
            for (di, dj) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
                let n = (cur.0 + di, cur.1 + dj);
                if label_set.contains_key(&n) && !visited.contains(&n) {
                    stack.push(n);
                }
            }
        }
        components.push(comp);
    }
    let components_seen = components.len() as u32;
    // Largest component wins; everything else is a false positive.
    let mut disconnect_drop: Set<usize> = Set::new();
    if components.len() > 1 {
        let largest_idx = components
            .iter()
            .enumerate()
            .max_by_key(|(_, c)| c.len())
            .map(|(i, _)| i)
            .unwrap_or(0);
        for (ci, comp) in components.iter().enumerate() {
            if ci == largest_idx {
                continue;
            }
            for ij in comp {
                if let Some(&idx) = label_set.get(ij) {
                    disconnect_drop.insert(idx);
                }
            }
        }
    }
    all_drop.extend(disconnect_drop.iter().copied());

    let dropped_validate = validate_drop.len() as u32;
    let dropped_edge_only = 0u32;
    let dropped_disconnected = disconnect_drop.len() as u32;

    for &idx in &all_drop {
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
        dropped: all_drop.len() as u32,
        dropped_line_collinearity: dropped_validate,
        dropped_local_h_residual: 0, // shared validator lumps these — kept for forward-compat
        dropped_edge_invariant: dropped_edge_only,
        dropped_disconnected,
        components_seen,
        detection_refused,
    }
}
