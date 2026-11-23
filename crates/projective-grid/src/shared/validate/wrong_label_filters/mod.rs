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
//! tuning), but the geometry itself lives here as the shared
//! topological pipeline implementation.
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

/// Maximum consecutive edge length ratio on a small or sparse component before
/// treating the shared vertex as a gross topological kink. Perspective changes
/// consecutive cell lengths gradually; only skipped-corner / saddle jumps
/// exceed `2.0×`.
const TOPO_GROSS_CONT_RATIO: f32 = 2.0;

/// Maximum turning angle (degrees) between consecutive collinear edges sharing
/// a vertex. True grids stay smoothly collinear; false saddle bridges bend
/// sharply past `45°`.
const TOPO_GROSS_CONT_ANGLE_DEG: f32 = 45.0;

/// Maximum opposing-side length ratio in a complete `2×2` quad cell. Strong
/// perspective stays well below `2.0×` across a single cell; false saddle
/// quads collapse on one side and stretch past `2.0×`.
const TOPO_GROSS_CELL_OPPOSITE_RATIO: f32 = 2.0;

/// Maximum opposing-side angle (degrees) in a complete `2×2` quad cell.
const TOPO_GROSS_CELL_OPPOSITE_ANGLE_DEG: f32 = 45.0;

/// Lattice-orientation drops — a **parity invariant**, not a tolerance.
///
/// The map from lattice coordinates to image positions is a homography, and a
/// homography is orientation-preserving over any region that does not cross
/// its vanishing line — which a physically imaged planar target never does:
/// the whole board lies in front of the camera, so the projective weight keeps
/// one sign across it. The signed area of the local basis
///
/// ```text
/// e_u = p(u+1, v) - p(u, v)      e_v = p(u, v+1) - p(u, v)
/// orientation(u, v) = sign(e_u × e_v)
/// ```
///
/// therefore has the **same sign at every labelled cell of a correctly
/// labelled component**, for every viewpoint, every focal length and any
/// amount of smooth lens distortion.
///
/// A sub-block glued in with a *reflected* axis — the dominant outcome when a
/// component merge accepts a mirroring D4 transform on thin overlap, and the
/// mechanism behind whole swapped or mirrored rows — reverses that sign while
/// leaving edge lengths, edge directions and pixel separations entirely
/// plausible. None of the first-order checks in
/// `topological_wrong_label_drops` can see it. This one sees it directly,
/// and there is nothing to tune: it compares a sign.
///
/// A label is dropped only when every cell it takes part in carries the
/// minority sign; a label on the seam between the two regions is a legitimate
/// member of the majority region and is kept. The caller's largest-component
/// filter then sweeps whatever the drop orphans.
///
/// **Exact tie.** When the two signs occur equally often no side is more
/// credible than the other, and emitting either half would be a coin flip on
/// correctness — which the detector's precision contract forbids. Both sides'
/// exclusive labels are dropped, leaving the component to be refused rather
/// than shipped half-wrong.
///
/// The result is deterministic: sign counting and set membership do not depend
/// on `HashMap` iteration order.
pub fn lattice_orientation_drops<F>(
    labelled: &HashMap<(i32, i32), usize>,
    position_of: F,
) -> HashSet<usize>
where
    F: Fn(usize) -> Point2<f32>,
{
    // Cell orientation, one entry per labelled cell that has both cardinal
    // successors. `members` is the three labels the sign is computed from.
    let mut positive: HashSet<usize> = HashSet::new();
    let mut negative: HashSet<usize> = HashSet::new();
    let mut n_positive = 0usize;
    let mut n_negative = 0usize;

    for (&(u, v), &idx_o) in labelled.iter() {
        let (Some(&idx_u), Some(&idx_v)) = (labelled.get(&(u + 1, v)), labelled.get(&(u, v + 1)))
        else {
            continue;
        };
        let origin = position_of(idx_o);
        let e_u = position_of(idx_u) - origin;
        let e_v = position_of(idx_v) - origin;
        let cross = e_u.x * e_v.y - e_u.y * e_v.x;
        if cross == 0.0 {
            continue; // degenerate cell — the duplicate-pixel guard owns this
        }
        let (side, count) = if cross > 0.0 {
            (&mut positive, &mut n_positive)
        } else {
            (&mut negative, &mut n_negative)
        };
        *count += 1;
        side.extend([idx_o, idx_u, idx_v]);
    }

    if n_positive == 0 || n_negative == 0 {
        return HashSet::new(); // consistent orientation — nothing to say
    }

    if n_positive == n_negative {
        // No majority: neither half is more credible than the other.
        let mut drop: HashSet<usize> = positive.difference(&negative).copied().collect();
        drop.extend(negative.difference(&positive).copied());
        return drop;
    }

    let (minority, majority) = if n_positive > n_negative {
        (&negative, &positive)
    } else {
        (&positive, &negative)
    };
    minority.difference(majority).copied().collect()
}

/// Direct local wrong-label edge detector for the topological grid
/// builder.
///
/// An edge-shape gate whose overlong check is gated behind a collinear triple
/// cannot reach the dominant topological wrong-label classes — interior
/// skipped-corner edges and duplicate-pixel labels — so this targets them
/// directly using only local geometry, robust to perspective:
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
        for &(i, j) in labelled.keys() {
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

    // Criterion 5: scale-free gross local continuation & quad-cell sanity
    // (Gap 32). Arms regardless of component size so small 6–9 corner false
    // grids below `TOPO_MIN_LOCAL_SAMPLES` / `TOPO_MIN_GLOBAL_SAMPLES` cannot
    // slip through unjudged.
    drop.extend(gross_local_shape_drops(labelled, &position_of));

    drop
}

fn vec_angle_deg(a: Vector2<f32>, b: Vector2<f32>) -> Option<f32> {
    let an = a.norm();
    let bn = b.norm();
    if !an.is_finite() || !bn.is_finite() || an <= f32::EPSILON || bn <= f32::EPSILON {
        return None;
    }
    let cos = (a.dot(&b) / (an * bn)).clamp(-1.0, 1.0);
    Some(cos.acos().to_degrees())
}

fn vec_length_ratio(a: Vector2<f32>, b: Vector2<f32>) -> Option<f32> {
    let an = a.norm();
    let bn = b.norm();
    if !an.is_finite() || !bn.is_finite() {
        return None;
    }
    let lo = an.min(bn);
    if lo <= f32::EPSILON {
        return None;
    }
    Some(an.max(bn) / lo)
}

fn gross_local_shape_drops<F>(
    labelled: &HashMap<(i32, i32), usize>,
    position_of: F,
) -> HashSet<usize>
where
    F: Fn(usize) -> Point2<f32>,
{
    let mut drop: HashSet<usize> = HashSet::new();

    for (&(i, j), &idx) in labelled {
        let p_cur = position_of(idx);
        let mut kinked = false;
        for (di, dj) in [(1i32, 0i32), (0, 1)] {
            if let (Some(&prev_idx), Some(&next_idx)) = (
                labelled.get(&(i - di, j - dj)),
                labelled.get(&(i + di, j + dj)),
            ) {
                let e1 = p_cur - position_of(prev_idx);
                let e2 = position_of(next_idx) - p_cur;
                let bad_ratio = vec_length_ratio(e1, e2).is_none_or(|r| r > TOPO_GROSS_CONT_RATIO);
                let bad_angle = vec_angle_deg(e1, e2).is_none_or(|a| a > TOPO_GROSS_CONT_ANGLE_DEG);
                if bad_ratio || bad_angle {
                    kinked = true;
                    // The kink cannot say which end is false. An endpoint
                    // with no further neighbour along this line has the
                    // kinked edge as its only evidence on it (a frontier
                    // leaf), so it goes too: otherwise a false leaf would
                    // survive while its legitimate neighbour is dropped.
                    if !labelled.contains_key(&(i - 2 * di, j - 2 * dj)) {
                        drop.insert(prev_idx);
                    }
                    if !labelled.contains_key(&(i + 2 * di, j + 2 * dj)) {
                        drop.insert(next_idx);
                    }
                    break;
                }
            }
        }
        if kinked {
            drop.insert(idx);
            continue;
        }

        let mut adjacent_cells = 0u32;
        let mut valid_cells = 0u32;
        for (ci, cj) in [(i, j), (i - 1, j), (i - 1, j - 1), (i, j - 1)] {
            let (Some(&tl), Some(&tr), Some(&br), Some(&bl)) = (
                labelled.get(&(ci, cj)),
                labelled.get(&(ci + 1, cj)),
                labelled.get(&(ci + 1, cj + 1)),
                labelled.get(&(ci, cj + 1)),
            ) else {
                continue;
            };
            adjacent_cells += 1;
            let p_tl = position_of(tl);
            let p_tr = position_of(tr);
            let p_br = position_of(br);
            let p_bl = position_of(bl);
            let top = p_tr - p_tl;
            let bottom = p_br - p_bl;
            let left = p_bl - p_tl;
            let right = p_br - p_tr;
            let cell_ok = [(top, bottom), (left, right)].into_iter().all(|(a, b)| {
                vec_length_ratio(a, b).is_some_and(|r| r <= TOPO_GROSS_CELL_OPPOSITE_RATIO)
                    && vec_angle_deg(a, b)
                        .is_some_and(|ang| ang <= TOPO_GROSS_CELL_OPPOSITE_ANGLE_DEG)
            });
            if cell_ok {
                valid_cells += 1;
            }
        }
        if adjacent_cells > 0 && valid_cells == 0 {
            drop.insert(idx);
        }
    }

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
#[non_exhaustive]
#[derive(Debug, Default)]
pub struct DropSet {
    /// Union of every dropped index across the filters below.
    pub drop: HashSet<usize>,
    /// Indices dropped by the line-collinearity / local-H validate pass.
    pub validate_drop: HashSet<usize>,
    /// Indices dropped by the lattice-orientation parity invariant
    /// (`lattice_orientation_drops`). Always applied — it is an invariant,
    /// not a heuristic, so it is not gated by `apply_wrong_label_drops`.
    pub orientation_drop: HashSet<usize>,
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
/// 2. lattice-orientation parity violations (`lattice_orientation_drops`) —
///    **always applied**, because a reversed local basis is a proof of a wrong
///    label rather than a tuned suspicion;
/// 3. topological wrong-label drops when `apply_wrong_label_drops`;
/// 4. the non-largest cardinally-connected component sweep when
///    `apply_largest_component`, computed
///    over the union of (1)+(2)+(3) so a drop that splits a component removes
///    the orphaned half.
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

    let orientation_drop = lattice_orientation_drops(labelled, &position);
    drop.extend(orientation_drop.iter().copied());

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
        orientation_drop,
        wrong_label_drop,
        component_drop,
        components_seen,
    }
}

#[cfg(test)]
mod tests;
