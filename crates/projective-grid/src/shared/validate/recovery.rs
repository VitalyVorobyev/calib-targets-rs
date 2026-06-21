//! Lattice-general final-recovery filters used by a detector's mandatory
//! geometry check.
//!
//! These three filters operate purely over a labelled `(i, j) → index`
//! map plus per-index position / strength accessors. They carry no
//! target-specific vocabulary (no feature classes, no parity, no marker
//! IDs), so any square-lattice detector can sequence them after its
//! engine-specific gates. Each filter can only *drop* corners; the
//! caller folds the returned index sets into its own blacklist and stage
//! machine.
//!
//! The chessboard detector calls these from its
//! `pipeline::geometry_check` after the shared
//! [`validate`](crate::shared::validate::validate) pass; the order and
//! the flag-gated activation stay caller-side (they read the caller's
//! tuning), but the geometry itself lives here so the topological and
//! seed-and-grow strategies share one implementation.
//!
//! What does NOT belong here: the stage *orchestration* (which filter runs
//! when, and the parity / axis recheck) — that is irreducibly coupled to the
//! caller's stage machine and stays caller-side.
//!
//! **Tier:** advanced engine — semver-exempt pre-1.0.
//!
//! # Determinism
//!
//! None of these filters depend on `HashMap` iteration order: the
//! component filter pins its tie-break with an explicit total order over
//! the minimum member coordinate, and the wrong-label / weak-leaf passes
//! only ever *insert* into a result set, so the final set is
//! order-independent.

use std::collections::{HashMap, HashSet};

use nalgebra::{Point2, Vector2};

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

/// Minimum same-direction edges in a component (per direction) before the
/// global-reference fallback is trusted. Below this the component is too
/// small to define a reliable median, so a sparse edge is left for the
/// largest-component filter rather than judged.
const TOPO_MIN_GLOBAL_SAMPLES: usize = 8;

/// Overlong multiple for the **global** fallback reference (sparse frontier
/// regions, fewer than [`TOPO_MIN_LOCAL_SAMPLES`] nearby same-direction
/// edges). Looser than the local [`TOPO_OVERLONG_EDGE_RATIO`] because the
/// component-global median spans the whole board, so a foreshortened-but-
/// legitimate frontier edge can sit modestly above it; matches the
/// independent overlong-edge audit's `1.6×` threshold so the check and the
/// audit agree at the frontier.
const TOPO_GLOBAL_OVERLONG_EDGE_RATIO: f32 = 1.6;

/// Dimensionless smoothness bound for the frontier line-spacing check
/// (criterion 4). A frontier (outermost) grid-line member whose edge to its
/// inner neighbour *overshoots* the linear extrapolation of the next two
/// inner edges by more than this fraction of the inner edge length is a
/// structural kink in an otherwise-smooth line — a false attachment, not
/// distortion.
///
/// This is a **second-order** criterion (line-spacing curvature), the
/// complement of the first-order overlong / off-axis checks above: a
/// normal-length, on-axis false attachment one cell past the true board
/// edge passes all three first-order tests yet reverses the smooth spacing
/// trend of its own grid line. The bound is scale-free and
/// distortion-model-agnostic: under any smooth (C²) lens distortion the
/// edge-length sequence along a grid line is a smooth function, so its
/// normalised second difference stays well below this value (measured
/// ≈0.07–0.13 even on a heavily barrel-distorted board's interior) while a
/// kink jumps past it. It is therefore an order-of-magnitude smoothness
/// argument, **not** a constant fitted to any one frame. Only the overshoot
/// direction (`r > tol`) is flagged: a smooth foreshortened frontier
/// undershoots or matches the extrapolation, so this can never peel a
/// legitimate compressed frontier.
const TOPO_FRONTIER_CURV_TOL: f32 = 0.30;

/// The frontier line-spacing check judges only genuinely dangling frontier
/// members: a line-endpoint with cardinal degree above this is part of a
/// well-supported board edge (a kink there is caught by the overlong /
/// local-H checks, and dropping a supported corner could fragment the
/// grid). Degree ≤ 2 admits true frontier leaves (degree 1) and board-corner
/// members (degree 2) only — a mid-edge member with a full perpendicular
/// neighbour pair has degree 3 and is spared.
const TOPO_FRONTIER_MAX_DEGREE: usize = 2;

/// Direct local wrong-label edge detector for the topological grid
/// builder.
///
/// The seed-and-grow edge-shape gate cannot reach the dominant
/// topological wrong-label classes — interior skipped-corner edges (its
/// overlong check is gated behind a collinear triple) and duplicate-pixel
/// labels — so this targets them directly using only local geometry,
/// robust to perspective:
///
/// 1. **Overlong edge.** Each cardinal edge is compared to the median
///    length of nearby same-direction edges; an edge ≥ `1.5`× that median
///    is a skipped-corner boundary.
/// 2. **Off-axis edge.** Each edge's direction is compared to the local
///    same-direction mean; a deviation > `30`° is an axis-reversed /
///    diagonal label.
/// 3. **Duplicate pixel.** Two distinct labels within `0.2`× the cell
///    size cannot both be correct.
/// 4. **Frontier line-spacing kink.** A frontier (line-endpoint) member
///    whose edge overshoots the smooth spacing extrapolation of its grid
///    line is a false attachment past the true board edge — a *second-order*
///    signal that catches normal-length, on-axis extensions the first three
///    (first-order) checks miss (see the private `frontier_curvature_drops`).
///
/// **Sparse-frontier fallback.** Checks (1)/(2) need
/// `TOPO_MIN_LOCAL_SAMPLES` nearby same-direction edges to define a local
/// reference; a ragged frontier in a defocused band can fall below that and
/// historically slipped through. When the local window is too sparse, the
/// overlong test (1) falls back to the **component-global** same-direction
/// median at the looser `TOPO_GLOBAL_OVERLONG_EDGE_RATIO` (`1.6×`, the
/// independent audit's threshold), skipped only when the component itself
/// has fewer than `TOPO_MIN_GLOBAL_SAMPLES` edges in that direction. The
/// off-axis test (2) is *not* applied in the global branch — a board-spanning
/// mean direction is unreliable under perspective rotation, so only the
/// length signal carries over.
///
/// Both endpoints of a flagged edge are dropped; the caller's
/// largest-component filter then sweeps any strip orphaned by the drop.
/// The result is deterministic — it does not depend on `HashMap`
/// iteration order.
///
/// `position_of` maps a labelled corner index to its image-pixel
/// position.
pub fn topological_wrong_label_drops<F>(
    labelled: &HashMap<(i32, i32), usize>,
    position_of: F,
    cell_size: f32,
) -> HashSet<usize>
where
    F: Fn(usize) -> Point2<f32>,
{
    let mut drop: HashSet<usize> = HashSet::new();

    // Component-global same-direction references (median length + summed unit
    // direction), one per cardinal direction. Computed once; used only as the
    // sparse-frontier fallback below, so dense interiors are unaffected.
    let global_reference = |di: i32, dj: i32| -> Option<f32> {
        let mut lens: Vec<f32> = Vec::new();
        for (&(i, j), _) in labelled.iter() {
            if let Some(&idx_b) = labelled.get(&(i + di, j + dj)) {
                let idx_a = labelled[&(i, j)];
                let l = (position_of(idx_b) - position_of(idx_a)).norm();
                if l > 0.0 {
                    lens.push(l);
                }
            }
        }
        if lens.len() < TOPO_MIN_GLOBAL_SAMPLES {
            return None;
        }
        lens.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        Some(lens[lens.len() / 2])
    };
    let global_median = [global_reference(1, 0), global_reference(0, 1)];

    // Overlong / off-axis edge checks against nearby same-direction edges.
    for (&(i, j), &idx_g) in labelled.iter() {
        for (dir_k, (di, dj)) in [(1i32, 0i32), (0, 1)].into_iter().enumerate() {
            let Some(&idx_n) = labelled.get(&(i + di, j + dj)) else {
                continue;
            };
            let edge = position_of(idx_n) - position_of(idx_g);
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
                        let e = position_of(ib) - position_of(ia);
                        let l = e.norm();
                        if l > 0.0 {
                            lens.push(l);
                            dir_sum += e / l;
                        }
                    }
                }
            }
            // Pick the reference: the local same-direction window when dense
            // enough (the precise, perspective-robust path — byte-identical to
            // the original), else the component-global median as a sparse-
            // frontier fallback (overlong-only). Skip entirely when even the
            // global reference is too sparse.
            let (median, overlong_ratio, off_axis_dir) = if lens.len() >= TOPO_MIN_LOCAL_SAMPLES {
                lens.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                (
                    lens[lens.len() / 2],
                    TOPO_OVERLONG_EDGE_RATIO,
                    Some(dir_sum),
                )
            } else {
                match global_median[dir_k] {
                    Some(gm) => (gm, TOPO_GLOBAL_OVERLONG_EDGE_RATIO, None),
                    None => continue, // too sparse to judge — leave it alone
                }
            };
            let overlong = median > 0.0 && len > overlong_ratio * median;
            let off_axis = match off_axis_dir {
                Some(ds) if ds.norm() > 0.0 => {
                    let cos = (edge / len).dot(&ds.normalize()).clamp(-1.0, 1.0);
                    cos.acos().to_degrees() > TOPO_OFF_AXIS_TOL_DEG
                }
                _ => false,
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
            .map(|&idx| (idx, position_of(idx)))
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

    // Criterion 4: frontier line-spacing smoothness. Catches a
    // normal-length, on-axis false attachment one cell past the true board
    // edge — invisible to the first-order overlong / off-axis tests above —
    // as a second-order kink in the otherwise-smooth grid-line spacing.
    drop.extend(frontier_curvature_drops(labelled, &position_of));

    drop
}

/// Frontier line-spacing smoothness check (criterion 4 of the topological
/// wrong-label detector). See [`TOPO_FRONTIER_CURV_TOL`].
///
/// For every grid line — each row (`j = const`) and column (`i = const`) —
/// whose outermost four members are consecutive, the frontier edge `e0`
/// (between the outermost member and its inner neighbour) is compared to the
/// linear extrapolation `2·e1 − e2` of the next two inner edges. A frontier
/// member of cardinal degree ≤ [`TOPO_FRONTIER_MAX_DEGREE`] whose edge
/// **overshoots** that extrapolation by more than [`TOPO_FRONTIER_CURV_TOL`]
/// × `e1` is dropped — and only it, never its (legitimate) inner neighbours.
///
/// Both ends of every line are tested. The check is order-independent: it
/// only ever inserts into the returned set, and every decision depends only
/// on the (sorted) members of a single line.
fn frontier_curvature_drops<F>(
    labelled: &HashMap<(i32, i32), usize>,
    position_of: F,
) -> HashSet<usize>
where
    F: Fn(usize) -> Point2<f32>,
{
    let mut drop: HashSet<usize> = HashSet::new();
    let degree = |i: i32, j: i32| -> usize {
        [(1, 0), (-1, 0), (0, 1), (0, -1)]
            .into_iter()
            .filter(|&(di, dj)| labelled.contains_key(&(i + di, j + dj)))
            .count()
    };

    // `axis == 0` → rows (line key = j, member coordinate = i);
    // `axis == 1` → columns (line key = i, member coordinate = j).
    for axis in 0..2 {
        let mut lines: HashMap<i32, Vec<i32>> = HashMap::new();
        for &(i, j) in labelled.keys() {
            let (key, coord) = if axis == 0 { (j, i) } else { (i, j) };
            lines.entry(key).or_default().push(coord);
        }
        for (&key, coords) in lines.iter_mut() {
            if coords.len() < 4 {
                continue;
            }
            coords.sort_unstable();
            let cell_of = |coord: i32| -> (i32, i32) {
                if axis == 0 {
                    (coord, key)
                } else {
                    (key, coord)
                }
            };
            let pos_at = |coord: i32| -> Point2<f32> { position_of(labelled[&cell_of(coord)]) };
            // Both ends: ascending (`dir = +1`) and descending (`dir = -1`).
            for &dir in &[1i32, -1] {
                let outer: Vec<i32> = if dir == 1 {
                    coords[..4].to_vec()
                } else {
                    coords[coords.len() - 4..].iter().rev().copied().collect()
                };
                let (c0, c1, c2, c3) = (outer[0], outer[1], outer[2], outer[3]);
                // Extrapolation is only defined on an unbroken run of four.
                if c1 != c0 + dir || c2 != c0 + 2 * dir || c3 != c0 + 3 * dir {
                    continue;
                }
                let e0 = (pos_at(c1) - pos_at(c0)).norm();
                let e1 = (pos_at(c2) - pos_at(c1)).norm();
                let e2 = (pos_at(c3) - pos_at(c2)).norm();
                if e1 <= 0.0 {
                    continue;
                }
                let predicted = 2.0 * e1 - e2;
                let r = (e0 - predicted) / e1;
                let (oi, oj) = cell_of(c0);
                if r > TOPO_FRONTIER_CURV_TOL && degree(oi, oj) <= TOPO_FRONTIER_MAX_DEGREE {
                    drop.insert(labelled[&(oi, oj)]);
                }
            }
        }
    }
    drop
}

/// Iterative weak-leaf peel.
///
/// Iteratively drops a corner that is BOTH a graph leaf (cardinal degree
/// ≤ 1 among surviving labels) AND weak in response relative to the frame
/// (strength below `score_frac` × the labelled-set median). Removing a
/// leaf can NEVER disconnect the remaining graph, so unlike a blanket
/// low-support / zero-cell prune this cannot fragment a legitimately
/// sparse distorted detection. It peels weak appendage chains from their
/// free end while leaving weak *bridges* intact — a bridge between two
/// strong regions never becomes a leaf, so it is never peeled.
///
/// `already_dropped` carries the indices removed by earlier filters; the
/// live graph used for the leaf-degree test excludes them. `strength_of`
/// maps a labelled corner index to its detector response strength. The
/// returned set is the indices this pass newly peels (a subset disjoint
/// from `already_dropped`).
pub fn weak_leaf_peel<F>(
    labelled: &HashMap<(i32, i32), usize>,
    strength_of: F,
    already_dropped: &HashSet<usize>,
    score_frac: f32,
) -> HashSet<usize>
where
    F: Fn(usize) -> f32,
{
    let mut weak_leaf_drop: HashSet<usize> = HashSet::new();
    let mut all_drop: HashSet<usize> = already_dropped.clone();

    let mut strengths: Vec<f32> = labelled.values().map(|&idx| strength_of(idx)).collect();
    strengths.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = strengths.get(strengths.len() / 2).copied().unwrap_or(0.0);
    let weak_threshold = score_frac * median;
    loop {
        let live: HashMap<(i32, i32), usize> = labelled
            .iter()
            .filter(|(_, idx)| !all_drop.contains(idx))
            .map(|(&k, &v)| (k, v))
            .collect();
        let mut progressed = false;
        for (&(i, j), &idx) in &live {
            if strength_of(idx) >= weak_threshold {
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
    weak_leaf_drop
}

/// Result of the largest-cardinally-connected-component filter.
///
/// Data carrier — not `#[non_exhaustive]` (the caller reads both fields
/// directly).
#[derive(Clone, Debug, Default)]
pub struct ComponentFilter {
    /// Indices belonging to a non-largest component — false positives to
    /// drop.
    pub drop: HashSet<usize>,
    /// Number of cardinally-connected components seen among the surviving
    /// labels (diagnostic).
    pub components_seen: u32,
}

/// Largest-cardinally-connected-component filter.
///
/// A square-lattice detection is by construction one `(i, j)`-labelled
/// cardinally-connected planar graph; any singleton or small component
/// that survived earlier stages is a false positive. Keeps only the
/// largest component and reports the rest for dropping.
///
/// Components are computed AFTER `already_dropped` is applied so that
/// dropping a "bridge" corner in an earlier filter can split a component
/// and have the smaller half correctly removed here.
///
/// Tie-break is deterministic: on equal size, the component whose
/// smallest member coordinate is lexicographically smallest wins. The
/// component vector is built from a `HashMap` scan (randomized per
/// process), so the explicit total-order key is required for
/// reproducibility.
pub fn largest_component_filter(
    labelled: &HashMap<(i32, i32), usize>,
    already_dropped: &HashSet<usize>,
) -> ComponentFilter {
    let surviving_labels: Vec<((i32, i32), usize)> = labelled
        .iter()
        .filter(|(_, &idx)| !already_dropped.contains(&idx))
        .map(|(&k, &v)| (k, v))
        .collect();
    let label_set: HashMap<(i32, i32), usize> = surviving_labels.iter().copied().collect();
    let mut visited: HashSet<(i32, i32)> = HashSet::new();
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
    let mut drop: HashSet<usize> = HashSet::new();
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
                    drop.insert(idx);
                }
            }
        }
    }
    ComponentFilter {
        drop,
        components_seen,
    }
}

/// Per-stage breakdown of a [`drop_set`] precision pass.
pub struct DropSet {
    /// Union of every dropped index across the three filters below.
    pub drop: HashSet<usize>,
    /// Indices dropped by the line-collinearity / local-H validate pass.
    pub validate_drop: HashSet<usize>,
    /// Indices dropped by the topological wrong-label check (empty when
    /// `apply_wrong_label_drops` was `false`).
    pub wrong_label_drop: HashSet<usize>,
    /// Indices dropped by the largest-component filter (empty when
    /// `apply_largest_component` was `false`).
    pub component_drop: HashSet<usize>,
    /// Number of cardinally-connected components the component filter saw
    /// (`0` when `apply_largest_component` was `false`).
    pub components_seen: u32,
}

/// Combined precision drop-set shared by the geometry-only recovery
/// schedule's revalidation and a downstream detector's mandatory final
/// geometry check.
///
/// Over a labelled `(i, j) → index` map, computes the union of:
/// 1. line-collinearity + local-H residual outliers
///    ([`validate`](crate::shared::validate::validate));
/// 2. topological wrong-label drops ([`topological_wrong_label_drops`]) when
///    `apply_wrong_label_drops`;
/// 3. the non-largest cardinally-connected component sweep
///    ([`largest_component_filter`]) when `apply_largest_component`, computed
///    over the union of (1)+(2) so a drop that splits a component removes the
///    orphaned half.
///
/// Entries are materialised in deterministic `(i, j)`-sorted order before the
/// validate pass, so the result never depends on the caller's `HashMap`
/// iteration order (the validate pass is order-sensitive on coincident
/// positions; the other two filters are already order-independent — see the
/// module-level determinism note). The per-stage index sets are returned
/// alongside the union so the caller can attribute drops in its own trace.
///
/// This is the *composition*; applying the drop set to the caller's stage
/// machine (blacklist, `CornerStage`, refusal threshold) stays caller-side.
pub fn drop_set<F>(
    labelled: &HashMap<(i32, i32), usize>,
    position: F,
    cell_size: f32,
    validate_params: &super::ValidationParams,
    apply_wrong_label_drops: bool,
    apply_largest_component: bool,
) -> DropSet
where
    F: Fn(usize) -> Point2<f32>,
{
    let mut ordered: Vec<((i32, i32), usize)> = labelled.iter().map(|(&k, &v)| (k, v)).collect();
    ordered.sort_unstable_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let entries: Vec<super::LabelledEntry> = ordered
        .iter()
        .map(|&(grid, idx)| super::LabelledEntry {
            idx,
            pixel: position(idx),
            grid,
        })
        .collect();

    let validate_drop = super::validate(&entries, cell_size, validate_params).blacklist;
    let mut drop = validate_drop.clone();

    let mut wrong_label_drop = HashSet::new();
    if apply_wrong_label_drops {
        wrong_label_drop = topological_wrong_label_drops(labelled, &position, cell_size);
        drop.extend(wrong_label_drop.iter().copied());
    }

    let mut component_drop = HashSet::new();
    let mut components_seen = 0;
    if apply_largest_component {
        let comp = largest_component_filter(labelled, &drop);
        components_seen = comp.components_seen;
        component_drop = comp.drop;
        drop.extend(component_drop.iter().copied());
    }

    DropSet {
        drop,
        validate_drop,
        wrong_label_drop,
        component_drop,
        components_seen,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f32, y: f32) -> Point2<f32> {
        Point2::new(x, y)
    }

    #[test]
    fn component_filter_keeps_largest() {
        // One 3-cell row component, plus an isolated singleton far away.
        let mut labelled = HashMap::new();
        labelled.insert((0, 0), 0usize);
        labelled.insert((1, 0), 1);
        labelled.insert((2, 0), 2);
        labelled.insert((10, 10), 9); // isolated
        let res = largest_component_filter(&labelled, &HashSet::new());
        assert_eq!(res.components_seen, 2);
        assert_eq!(res.drop, HashSet::from([9]));
    }

    #[test]
    fn component_filter_tiebreak_is_deterministic() {
        // Two equal-size components; the lexicographically-smaller min
        // coord wins regardless of HashMap order. Run several times.
        for _ in 0..16 {
            let mut labelled = HashMap::new();
            // Component A: min coord (0,0)
            labelled.insert((0, 0), 0usize);
            labelled.insert((1, 0), 1);
            // Component B: min coord (5,5)
            labelled.insert((5, 5), 2);
            labelled.insert((6, 5), 3);
            let res = largest_component_filter(&labelled, &HashSet::new());
            assert_eq!(res.components_seen, 2);
            // A wins (min coord (0,0) < (5,5)); B's indices dropped.
            assert_eq!(res.drop, HashSet::from([2, 3]));
        }
    }

    #[test]
    fn weak_leaf_peels_weak_dangling_leaf_only() {
        // A 3-cell strong row plus one weak leaf hanging off the end.
        let mut labelled = HashMap::new();
        labelled.insert((0, 0), 0usize);
        labelled.insert((1, 0), 1);
        labelled.insert((2, 0), 2);
        labelled.insert((3, 0), 3); // weak leaf
        let strength_of = |idx: usize| if idx == 3 { 0.1 } else { 1.0 };
        let drop = weak_leaf_peel(&labelled, strength_of, &HashSet::new(), 0.55);
        assert_eq!(drop, HashSet::from([3]));
    }

    #[test]
    fn weak_bridge_is_not_peeled() {
        // Weak corner is a bridge (degree 2), never a leaf → kept.
        let mut labelled = HashMap::new();
        labelled.insert((0, 0), 0usize);
        labelled.insert((1, 0), 1); // weak bridge between (0,0) and (2,0)
        labelled.insert((2, 0), 2);
        let strength_of = |idx: usize| if idx == 1 { 0.1 } else { 1.0 };
        let drop = weak_leaf_peel(&labelled, strength_of, &HashSet::new(), 0.55);
        assert!(drop.is_empty());
    }

    #[test]
    fn topological_overlong_edge_drops_both_endpoints() {
        // A dense 5x5 grid of unit edges, plus one corner displaced far
        // along +i from (4,2) so that edge is overlong vs the local
        // median.
        let mut labelled = HashMap::new();
        let mut pos: HashMap<usize, Point2<f32>> = HashMap::new();
        let mut idx = 0usize;
        for i in 0..5 {
            for j in 0..5 {
                labelled.insert((i, j), idx);
                pos.insert(idx, p(i as f32, j as f32));
                idx += 1;
            }
        }
        // Add a far corner at (5,2): the (4,2)->(5,2) edge is length ~6
        // vs unit local median.
        labelled.insert((5, 2), idx);
        pos.insert(idx, p(10.0, 2.0));
        let far = idx;
        let position_of = |i: usize| pos[&i];
        let drop = topological_wrong_label_drops(&labelled, position_of, 1.0);
        assert!(drop.contains(&far));
    }

    #[test]
    fn topological_global_fallback_drops_sparse_overlong_edge() {
        // Dense 6×6 unit grid → component-global +i/+j median = 1.0 with many
        // samples (>= TOPO_MIN_GLOBAL_SAMPLES). Plus a sparse 2-cell vertical
        // pair far away whose edge is length 3 — overlong vs the global median —
        // and whose local 2-cell window holds no other +j edge. This is exactly
        // the sparse-frontier-bypass class the global fallback must now catch
        // (the original local-only check `continue`d and kept it).
        let mut labelled = HashMap::new();
        let mut pos: HashMap<usize, Point2<f32>> = HashMap::new();
        let mut idx = 0usize;
        for i in 0..6 {
            for j in 0..6 {
                labelled.insert((i, j), idx);
                pos.insert(idx, p(i as f32, j as f32));
                idx += 1;
            }
        }
        labelled.insert((20, 0), idx);
        pos.insert(idx, p(20.0, 0.0));
        let a = idx;
        idx += 1;
        labelled.insert((20, 1), idx);
        pos.insert(idx, p(20.0, 3.0));
        let b = idx;
        let position_of = |i: usize| pos[&i];
        let drop = topological_wrong_label_drops(&labelled, position_of, 1.0);
        assert!(
            drop.contains(&a) && drop.contains(&b),
            "sparse overlong edge must be caught by the global fallback"
        );
    }

    #[test]
    fn topological_global_fallback_keeps_ragged_frontier_edge() {
        // Same dense grid, but the sparse far pair's edge is only 1.1× the
        // global median — a legitimate ragged / foreshortened frontier. The
        // global fallback (overlong-only at 1.6×) must NOT drop it, proving the
        // fix does not cost recall on a genuinely sparse-but-valid frontier.
        let mut labelled = HashMap::new();
        let mut pos: HashMap<usize, Point2<f32>> = HashMap::new();
        let mut idx = 0usize;
        for i in 0..6 {
            for j in 0..6 {
                labelled.insert((i, j), idx);
                pos.insert(idx, p(i as f32, j as f32));
                idx += 1;
            }
        }
        labelled.insert((20, 0), idx);
        pos.insert(idx, p(20.0, 0.0));
        let a = idx;
        idx += 1;
        labelled.insert((20, 1), idx);
        pos.insert(idx, p(20.0, 1.1));
        let b = idx;
        let position_of = |i: usize| pos[&i];
        let drop = topological_wrong_label_drops(&labelled, position_of, 1.0);
        assert!(
            !drop.contains(&a) && !drop.contains(&b),
            "a sparse but ~unit-length frontier edge must be kept"
        );
    }

    #[test]
    fn topological_duplicate_pixel_drops_both() {
        let mut labelled = HashMap::new();
        let mut pos: HashMap<usize, Point2<f32>> = HashMap::new();
        labelled.insert((0, 0), 0usize);
        pos.insert(0, p(0.0, 0.0));
        labelled.insert((1, 0), 1);
        pos.insert(1, p(0.01, 0.0)); // collapsed onto ~same pixel as 0
        let position_of = |i: usize| pos[&i];
        let drop = topological_wrong_label_drops(&labelled, position_of, 1.0);
        assert_eq!(drop, HashSet::from([0, 1]));
    }

    /// A labelled `(i, j) → idx` map plus an idx-indexed position list.
    type GridFixture = (HashMap<(i32, i32), usize>, Vec<Point2<f32>>);

    /// Build a labelled map + position lookup from `(i, j, x, y)` rows.
    fn grid_from(cells: &[(i32, i32, f32, f32)]) -> GridFixture {
        let mut labelled = HashMap::new();
        let mut pos = Vec::new();
        for &(i, j, x, y) in cells {
            let idx = pos.len();
            labelled.insert((i, j), idx);
            pos.push(p(x, y));
        }
        (labelled, pos)
    }

    #[test]
    fn frontier_curvature_keeps_clean_grid() {
        // Uniform 6×6 grid: every edge equal → extrapolation exact → r = 0.
        let mut cells = Vec::new();
        for i in 0..6 {
            for j in 0..6 {
                cells.push((i, j, i as f32 * 50.0, j as f32 * 50.0));
            }
        }
        let (labelled, pos) = grid_from(&cells);
        let drop = frontier_curvature_drops(&labelled, |idx| pos[idx]);
        assert!(drop.is_empty(), "{drop:?}");
    }

    #[test]
    fn frontier_curvature_keeps_smooth_distorted_frontier() {
        // A single row whose cell pitch follows a smooth bell (barrel-style:
        // shrinking toward both ends). Cumulative x from edges 40,50,56,56,50,40.
        let xs = [0.0_f32, 40.0, 90.0, 146.0, 202.0, 252.0, 292.0];
        let cells: Vec<_> = xs
            .iter()
            .enumerate()
            .map(|(i, &x)| (i as i32, 0, x, 0.0))
            .collect();
        let (labelled, pos) = grid_from(&cells);
        let drop = frontier_curvature_drops(&labelled, |idx| pos[idx]);
        assert!(
            drop.is_empty(),
            "smooth distorted frontier must be kept: {drop:?}"
        );
    }

    #[test]
    fn frontier_curvature_drops_overshoot_leaf() {
        // Reproduces the GeminiChess1 row-2 signature: an isolated frontier
        // leaf whose outermost edge (45) overshoots the smooth inward trend
        // (edges 41, 50, …; extrapolated frontier ≈ 32). r = (45−32)/41 ≈
        // 0.32 > 0.30. Only the leaf (0,0) is dropped, not its neighbours.
        let xs = [0.0_f32, 45.0, 86.0, 136.0, 193.0];
        let cells: Vec<_> = xs
            .iter()
            .enumerate()
            .map(|(i, &x)| (i as i32, 0, x, 0.0))
            .collect();
        let (labelled, pos) = grid_from(&cells);
        let leaf = labelled[&(0, 0)];
        let drop = frontier_curvature_drops(&labelled, |idx| pos[idx]);
        assert_eq!(drop, HashSet::from([leaf]), "only the overshoot leaf drops");

        // And it propagates through the public detector (cell_size irrelevant
        // to this criterion; pass the inner edge length).
        let via_public = topological_wrong_label_drops(&labelled, |idx| pos[idx], 41.0);
        assert!(via_public.contains(&leaf));
    }

    #[test]
    fn frontier_curvature_skips_across_gap() {
        // Low end is non-consecutive (missing i=1), so the wild i=0 overshoot
        // is NOT judged — extrapolation is undefined across a gap.
        let cells = [
            (0, 0, 0.0, 0.0), // wild overshoot, but unreachable across the gap
            (2, 0, 200.0, 0.0),
            (3, 0, 250.0, 0.0),
            (4, 0, 300.0, 0.0),
            (5, 0, 345.0, 0.0),
        ];
        let (labelled, pos) = grid_from(&cells);
        let leaf = labelled[&(0, 0)];
        let drop = frontier_curvature_drops(&labelled, |idx| pos[idx]);
        assert!(
            !drop.contains(&leaf),
            "must not judge across a gap: {drop:?}"
        );
    }

    #[test]
    fn frontier_curvature_spares_supported_edge_member() {
        // (0,1) overshoots on its row, but it has cardinal degree 3 (a
        // well-supported left-edge member, not a dangling frontier), so the
        // degree gate spares it.
        let mut cells = vec![(0, 1, -20.0, 50.0)]; // displaced outward → row-1 overshoot
        for j in 0..3 {
            for i in 0..4 {
                if (i, j) == (0, 1) {
                    continue;
                }
                cells.push((i, j, i as f32 * 50.0, j as f32 * 50.0));
            }
        }
        let (labelled, pos) = grid_from(&cells);
        let supported = labelled[&(0, 1)];
        assert_eq!(
            [(1, 1), (-1, 1), (0, 0), (0, 2)]
                .into_iter()
                .filter(|c| labelled.contains_key(c))
                .count(),
            3,
            "test setup: (0,1) must have cardinal degree 3"
        );
        let drop = frontier_curvature_drops(&labelled, |idx| pos[idx]);
        assert!(
            !drop.contains(&supported),
            "degree-3 member must be spared: {drop:?}"
        );
    }
}
