//! Stage 12: the mandatory final geometry check.
//!
//! Runs after every other stage and can only remove corners or refuse
//! the detection — never add or relabel. This is the precision gate
//! mandated by `CLAUDE.md` ("Geometry check is mandatory before
//! returning a detection").

use std::collections::HashSet;

use projective_grid::shared::validate::EdgeShapeParams;

use crate::cluster::ClusterCenters;
use crate::corner::{CornerAug, CornerStage};
use crate::grow::GrowResult;
use crate::params::{DetectorParams, GraphBuildAlgorithm};

use super::types::GeometryCheckTrace;

const MIN_EDGE_SHAPE_LABELS: usize = 40;

/// A labelled corner is peeled by the weak-leaf rule only when its ChESS
/// strength falls below this fraction of the labelled-set median. The
/// genuine failure mode (weak corners in defocused / low-contrast regions
/// on marker-bearing boards) sits at 0.16–0.48× the median; a legitimate
/// distorted board's frontier sits at ≈1× (median 1.06× on the canonical
/// heavy-distortion regression image), so this isolates the former.
const WEAK_LEAF_SCORE_FRAC: f32 = 0.55;

/// Mandatory final precision gate. Runs after every other stage and
/// can only remove corners or refuse the detection — never add or
/// relabel.
///
/// Drops any labelled corner that fails:
/// - the shared [`validate`](projective_grid::shared::validate::validate)
///   pass (line collinearity + local-H residual + final edge-shape gate,
///   attribution rules from
///   [`mod@projective_grid::shared::validate`]); **or**
/// - the per-cardinal-edge axis-slot-swap parity check from
///   `ChessboardSquareAttachPolicy::edge_ok` — every edge between two
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
    use projective_grid::shared::validate as pg_validate;
    use std::collections::HashSet as Set;

    let tuning = params.effective_tuning();

    // Test 1: line collinearity + local-H residual via shared
    // validator, but with the LOOSER `geometry_check_*` tolerances —
    // the BFS-validation loop already accepted borderline perspective
    // drift; the geometry check's job is to catch gross mislabels
    // (full-cell or diagonal shifts) only. The edge-shape gate adds
    // local degree / continuation / cell-opposite-side checks that are
    // useful only at this final precision stage.
    let geom_entries: Vec<pg_validate::LabelledEntry> = grow_res
        .labelled
        .iter()
        .map(|(&grid, &idx)| pg_validate::LabelledEntry {
            idx,
            pixel: augs[idx].position,
            grid,
        })
        .collect();
    let mut geom_params = pg_validate::ValidationParams::new(
        tuning.geometry_check_line_tol_rel,
        tuning.line_min_members,
        tuning.geometry_check_local_h_tol_rel,
    );
    // The edge-shape gate + weak-leaf peel are `SeedAndGrow`-only. Their
    // tolerances are tuned for seed-and-grow grids; diagnosis showed they
    // over-peel topological grids badly (≈97% of their topological drops
    // were good corners — the 8° continuation-angle test misfires on short
    // foreshortened edges, the 1.18 length-ratio rejects legitimate
    // perspective foreshortening, and the degree / cell-opposite checks
    // peel the legitimate ragged frontier). The topological/puzzle path
    // instead runs `topological_wrong_label_drops` (Test 2.5 below), a
    // direct local check that targets the genuine wrong-label classes —
    // interior skipped-corner edges and duplicate-pixel labels — without
    // the over-peel. ChArUco (pinned to `SeedAndGrow`) is unaffected, so
    // its behaviour and the chessboard public bench stay byte-exact.
    let on_seed_and_grow = matches!(
        params.graph_build_algorithm,
        GraphBuildAlgorithm::SeedAndGrow
    );
    let dense_enough = geom_entries.len() >= MIN_EDGE_SHAPE_LABELS;
    let edge_shape_active =
        tuning.enable_final_edge_shape_check && dense_enough && on_seed_and_grow;
    if edge_shape_active {
        geom_params = geom_params.with_edge_shape_gate(EdgeShapeParams::default());
    }
    let weak_leaf_active = edge_shape_active;
    if tuning.validate_step_aware {
        // Geometry check stays step-aware so heavily distorted boards
        // get the same scale-relative thresholds as BFS validation.
        // Step-deviation gate is BFS-only — set to 0 (disabled).
        geom_params = geom_params.with_step_aware(0.0);
    }
    let v = pg_validate::validate(&geom_entries, cell_size, &geom_params);
    let validate_drop: Set<usize> = v.blacklist.iter().copied().collect();
    let edge_shape_drop: Set<usize> = v.edge_shape_reasons.keys().copied().collect();

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

    // Test 2: weak-leaf peel. Iteratively drop a corner that is BOTH a
    // graph leaf (cardinal degree <= 1 among surviving labels) AND weak
    // in ChESS response relative to the frame (strength below
    // WEAK_LEAF_SCORE_FRAC x the labelled-set median). Removing a leaf
    // can NEVER disconnect the remaining graph, so unlike a blanket
    // low-support / zero-cell prune this cannot fragment a legitimately
    // sparse distorted detection and feed the largest-component filter
    // below (which would otherwise amplify the loss). The lattice-general
    // peel logic lives in
    // [`projective_grid::shared::validate::recovery::weak_leaf_peel`]; the
    // dense-grid / edge-shape activation gate (`weak_leaf_active`) and the
    // ChESS-strength accessor stay here. Gated to the same dense-grid
    // regime as the edge-shape gate so sparse / marginal detections are
    // untouched.
    let mut weak_leaf_drop: Set<usize> = Set::new();
    if weak_leaf_active {
        weak_leaf_drop = pg_validate::recovery::weak_leaf_peel(
            &grow_res.labelled,
            |idx| augs[idx].strength,
            &all_drop,
            WEAK_LEAF_SCORE_FRAC,
        );
        all_drop.extend(weak_leaf_drop.iter().copied());
    }

    // Test 2.5: direct local wrong-label check (topological builder only).
    // The `SeedAndGrow` edge-shape gate above cannot reach the dominant
    // topological wrong-label classes — interior skipped-corner edges and
    // duplicate-pixel labels — so the topological path runs this instead.
    // It can only drop corners; the largest-component filter below then
    // sweeps any strip orphaned by a drop (a shifted strip beyond a
    // skipped corner carried wrong `(i, j)` labels, so dropping it is
    // precision-correct). The lattice-general geometry lives in
    // [`projective_grid::shared::validate::recovery::topological_wrong_label_drops`];
    // the builder gate stays here.
    let mut topo_wrong_label_drop: Set<usize> = Set::new();
    if tuning.enable_final_edge_shape_check && dense_enough && !on_seed_and_grow {
        topo_wrong_label_drop = pg_validate::recovery::topological_wrong_label_drops(
            &grow_res.labelled,
            |idx| augs[idx].position,
            cell_size,
        );
        all_drop.extend(topo_wrong_label_drop.iter().copied());
    }

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
    // then the smaller half is correctly removed. The lattice-general
    // filter (component scan + deterministic tie-break) lives in
    // [`projective_grid::shared::validate::recovery::largest_component_filter`].
    let component_filter =
        pg_validate::recovery::largest_component_filter(&grow_res.labelled, &all_drop);
    let components_seen = component_filter.components_seen;
    let disconnect_drop = component_filter.drop;
    all_drop.extend(disconnect_drop.iter().copied());

    let dropped_validate = validate_drop.difference(&edge_shape_drop).count() as u32;
    // edge_shape_drop/weak_leaf_drop (SeedAndGrow) and topo_wrong_label_drop
    // (topological) are mutually exclusive by builder, so summing is exact.
    let dropped_edge_only =
        (edge_shape_drop.len() + weak_leaf_drop.len() + topo_wrong_label_drop.len()) as u32;
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
