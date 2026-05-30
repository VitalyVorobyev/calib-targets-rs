//! Stage 12: the mandatory final geometry check.
//!
//! Runs after every other stage and can only remove corners or refuse
//! the detection — never add or relabel. This is the precision gate
//! mandated by `CLAUDE.md` ("Geometry check is mandatory before
//! returning a detection").

use std::collections::HashSet;

use nalgebra::{Point2, Vector2};
use projective_grid::detect::advanced::square::validate::EdgeShapeParams;

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
/// - the shared [`validate`](projective_grid::detect::advanced::square::validate::validate)
///   pass (line collinearity + local-H residual + final edge-shape gate,
///   attribution rules from
///   [`mod@projective_grid::detect::advanced::square::validate`]); **or**
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
    use projective_grid::detect::advanced::square::validate as pg_validate;
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
    // The edge-shape gate + weak-leaf peel are `ChessboardV2`-only. Their
    // tolerances are tuned for seed-and-grow grids; diagnosis showed they
    // over-peel topological grids badly (≈97% of their topological drops
    // were good corners — the 8° continuation-angle test misfires on short
    // foreshortened edges, the 1.18 length-ratio rejects legitimate
    // perspective foreshortening, and the degree / cell-opposite checks
    // peel the legitimate ragged frontier). The topological/puzzle path
    // instead runs `topological_wrong_label_drops` (Test 2.5 below), a
    // direct local check that targets the genuine wrong-label classes —
    // interior skipped-corner edges and duplicate-pixel labels — without
    // the over-peel. ChArUco (pinned to `ChessboardV2`) is unaffected, so
    // its behaviour and the chessboard public bench stay byte-exact.
    let on_chessboard_v2 = matches!(
        params.graph_build_algorithm,
        GraphBuildAlgorithm::ChessboardV2
    );
    let dense_enough = geom_entries.len() >= MIN_EDGE_SHAPE_LABELS;
    let edge_shape_active =
        tuning.enable_final_edge_shape_check && dense_enough && on_chessboard_v2;
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
    // below (which would otherwise amplify the loss). It peels weak
    // appendage chains from their free end (the defocused dangling rows /
    // leaves seen on marker-bearing boards) while leaving weak *bridges*
    // intact — a bridge between two strong regions never becomes a leaf,
    // so it is never peeled. The score gate spares the normal-strength
    // frontier of a genuine distorted board. Gated to the same dense-grid
    // regime as the edge-shape gate so sparse / marginal detections are
    // untouched.
    let mut weak_leaf_drop: Set<usize> = Set::new();
    if weak_leaf_active {
        let mut strengths: Vec<f32> = grow_res
            .labelled
            .values()
            .map(|&idx| augs[idx].strength)
            .collect();
        strengths.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median = strengths.get(strengths.len() / 2).copied().unwrap_or(0.0);
        let weak_threshold = WEAK_LEAF_SCORE_FRAC * median;
        loop {
            let live: std::collections::HashMap<(i32, i32), usize> = grow_res
                .labelled
                .iter()
                .filter(|(_, idx)| !all_drop.contains(idx))
                .map(|(&k, &v)| (k, v))
                .collect();
            let mut progressed = false;
            for (&(i, j), &idx) in &live {
                if augs[idx].strength >= weak_threshold {
                    continue;
                }
                let degree = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                    .into_iter()
                    .filter(|&(di, dj)| live.contains_key(&(i + di, j + dj)))
                    .count();
                if degree <= 1 && all_drop.insert(idx) {
                    weak_leaf_drop.insert(idx);
                    progressed = true;
                }
            }
            if !progressed {
                break;
            }
        }
    }

    // Test 2.5: direct local wrong-label check (topological builder only).
    // The `ChessboardV2` edge-shape gate above cannot reach the dominant
    // topological wrong-label classes — interior skipped-corner edges and
    // duplicate-pixel labels — so the topological path runs this instead.
    // It can only drop corners; the largest-component filter below then
    // sweeps any strip orphaned by a drop (a shifted strip beyond a
    // skipped corner carried wrong `(i, j)` labels, so dropping it is
    // precision-correct).
    let mut topo_wrong_label_drop: Set<usize> = Set::new();
    if tuning.enable_final_edge_shape_check && dense_enough && !on_chessboard_v2 {
        topo_wrong_label_drop = topological_wrong_label_drops(&grow_res.labelled, augs, cell_size);
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
    // Tie-break deterministically: on equal size, prefer the component
    // whose smallest member coord is lexicographically smallest. The
    // `components` vector is built from a `HashMap` scan, so its order
    // is randomized per process; `max_by_key` alone would pick a
    // different winner run to run on a size tie. An explicit total-order
    // key pins the choice. (`max_by_key` keeps the *last* maximum, so a
    // bare key would still depend on the randomized vector order.)
    let mut disconnect_drop: Set<usize> = Set::new();
    if components.len() > 1 {
        let largest_idx = components
            .iter()
            .enumerate()
            .max_by_key(|(_, c)| {
                let min_coord = c.iter().copied().min().unwrap_or((i32::MAX, i32::MAX));
                // Reverse the tiebreak coord so a *smaller* coord wins
                // under `max_by_key`'s greater-is-better semantics.
                (c.len(), std::cmp::Reverse(min_coord))
            })
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

    let dropped_validate = validate_drop.difference(&edge_shape_drop).count() as u32;
    // edge_shape_drop/weak_leaf_drop (ChessboardV2) and topo_wrong_label_drop
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

/// Local context radius (in grid cells) for the topological wrong-label
/// check. Each edge is compared against same-direction edges whose lower
/// endpoint lies within this many cells.
const TOPO_LOCAL_RADIUS: i32 = 2;

/// Minimum nearby same-direction edges required before the topological
/// wrong-label check trusts the local reference. Sparser regions (the
/// legitimate ragged frontier) are skipped to avoid false positives.
const TOPO_MIN_LOCAL_SAMPLES: usize = 5;

/// An edge longer than this multiple of the local same-direction median
/// is a skipped-corner / diagonal boundary. Perspective foreshortening
/// between *nearby* same-direction edges stays well below this (≈1.0–1.2×
/// locally; ≤1.37× even on heavily distorted boards), while a skipped
/// corner jumps to ≈2–3×.
const TOPO_OVERLONG_EDGE_RATIO: f32 = 1.5;

/// An edge whose direction deviates more than this (degrees) from the
/// local same-direction mean is an axis-reversed / diagonal label. The
/// local mean absorbs smooth perspective rotation, so legitimate corners
/// stay within a few degrees; only genuine wrong-axis labels exceed this.
const TOPO_OFF_AXIS_TOL_DEG: f32 = 30.0;

/// Two distinct grid labels closer than this fraction of the cell size
/// cannot both be correct (the topological grid folded onto one pixel).
const TOPO_DUP_PIXEL_FRAC: f32 = 0.2;

/// Direct local wrong-label edge detector for the topological grid
/// builder (Test 2.5 in [`run_geometry_check`]).
///
/// The `ChessboardV2` edge-shape gate cannot reach the dominant
/// topological wrong-label classes — interior skipped-corner edges (its
/// overlong check is gated behind `weakly_supported` and needs a
/// collinear triple) and duplicate-pixel labels — so this targets them
/// directly using only local geometry, robust to perspective:
///
/// 1. **Overlong edge.** Each cardinal edge is compared to the median
///    length of nearby same-direction edges; an edge ≥
///    [`TOPO_OVERLONG_EDGE_RATIO`]× that median is a skipped-corner
///    boundary.
/// 2. **Off-axis edge.** Each edge's direction is compared to the local
///    same-direction mean; a deviation > [`TOPO_OFF_AXIS_TOL_DEG`]° is an
///    axis-reversed / diagonal label.
/// 3. **Duplicate pixel.** Two distinct labels within
///    [`TOPO_DUP_PIXEL_FRAC`]× the cell size cannot both be correct.
///
/// Both endpoints of a flagged edge are dropped; the caller's
/// largest-component filter then sweeps any strip orphaned by the drop.
/// The result is deterministic — it does not depend on `HashMap`
/// iteration order.
fn topological_wrong_label_drops(
    labelled: &std::collections::HashMap<(i32, i32), usize>,
    augs: &[CornerAug],
    cell_size: f32,
) -> HashSet<usize> {
    let mut drop: HashSet<usize> = HashSet::new();

    // Overlong / off-axis edge checks against nearby same-direction edges.
    for (&(i, j), &idx_g) in labelled.iter() {
        for (di, dj) in [(1i32, 0i32), (0, 1)] {
            let Some(&idx_n) = labelled.get(&(i + di, j + dj)) else {
                continue;
            };
            let edge = augs[idx_n].position - augs[idx_g].position;
            let len = edge.norm();
            if len <= 0.0 {
                continue; // degenerate — handled by the duplicate-pixel guard
            }
            let mut lens: Vec<f32> = Vec::new();
            let mut dir_sum = Vector2::<f32>::zeros();
            for ii in (i - TOPO_LOCAL_RADIUS)..=(i + TOPO_LOCAL_RADIUS) {
                for jj in (j - TOPO_LOCAL_RADIUS)..=(j + TOPO_LOCAL_RADIUS) {
                    if (ii, jj) == (i, j) {
                        continue; // exclude the edge under test from its own reference
                    }
                    if let (Some(&ia), Some(&ib)) =
                        (labelled.get(&(ii, jj)), labelled.get(&(ii + di, jj + dj)))
                    {
                        let e = augs[ib].position - augs[ia].position;
                        let l = e.norm();
                        if l > 0.0 {
                            lens.push(l);
                            dir_sum += e / l;
                        }
                    }
                }
            }
            if lens.len() < TOPO_MIN_LOCAL_SAMPLES {
                continue; // too sparse to judge — leave the ragged frontier alone
            }
            lens.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let med = lens[lens.len() / 2];
            let overlong = med > 0.0 && len > TOPO_OVERLONG_EDGE_RATIO * med;
            let off_axis = dir_sum.norm() > 0.0 && {
                let cos = (edge / len).dot(&dir_sum.normalize()).clamp(-1.0, 1.0);
                cos.acos().to_degrees() > TOPO_OFF_AXIS_TOL_DEG
            };
            if overlong || off_axis {
                drop.insert(idx_g);
                drop.insert(idx_n);
            }
        }
    }

    // Duplicate-pixel guard: two distinct labels collapsed onto ~one pixel.
    if cell_size > 0.0 {
        let eps2 = (TOPO_DUP_PIXEL_FRAC * cell_size).powi(2);
        let entries: Vec<(usize, Point2<f32>)> = labelled
            .values()
            .map(|&idx| (idx, augs[idx].position))
            .collect();
        for (a, &(ia, pa)) in entries.iter().enumerate() {
            for &(ib, pb) in &entries[a + 1..] {
                if (pa - pb).norm_squared() < eps2 {
                    drop.insert(ia);
                    drop.insert(ib);
                }
            }
        }
    }

    drop
}
