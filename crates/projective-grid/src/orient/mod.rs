//! Local lattice-orientation synthesis from point positions alone.
//!
//! Both square strategies consume [`OrientedFeature<2>`] — each corner carries
//! two local grid directions. When the caller has no per-corner orientation
//! (`Evidence::Positions`: a dot grid, a circle grid, or a chessboard whose
//! corners carry no axis estimate), this module recovers those two directions
//! geometrically so the topological machinery runs
//! unchanged.
//!
//! # The perspective problem
//!
//! The grid is viewed in perspective. The two grid directions are therefore
//! **not** orthogonal in the image, and the angle between them **varies across
//! the image** (the grid-line families converge toward two vanishing points).
//! Any method that assumes a fixed 90° between the axes, or a single global
//! orientation, is wrong. The estimate must be *local* and must not constrain
//! the inter-axis angle.
//!
//! # What is perspective-invariant
//!
//! A projective map preserves straight lines, so three collinear grid points
//! `(i−1, j), (i, j), (i+1, j)` stay collinear in the image. Hence a corner's
//! `+u` and `−u` neighbour chords are **exactly antipodal** (180° apart) even
//! under arbitrary perspective. Folding chord orientation **modulo π**
//! therefore collapses each axis neighbour-pair onto a single direction
//! *exactly* — with no orthogonality assumption. The two grid directions show
//! up as two distinct clusters in `[0, π)`, separated by whatever angle the
//! local perspective dictates.
//!
//! Under moderate projection, a second fact often makes the neighbour set useful: for a grid cell the axis
//! step is shorter than the diagonal step (`√(a² + b²) > max(a, b)`), and mild
//! perspective scales both by roughly the same local factor, so a corner's
//! four nearest neighbours are typically its four axis neighbours (`±u`,
//! `±v`). Severe projective compression, dropout, or clutter can violate that
//! ordering; nearest-neighbour selection is not projectively invariant.
//!
//! # Algorithm
//!
//! 1. Per corner: fold the chord angles to its `k` nearest neighbours into
//!    `[0, π)`. Generically these are two antipodal pairs collapsing to two
//!    directions.
//! 2. Pool every corner's folded nearest-edge angles into a **global
//!    distribution** and pick its two dominant modes `(g0, g1)`. This is a
//!    robust, image-wide prior — used only to *seed* the per-corner estimate
//!    and as a fallback, never as the answer.
//! 3. Per corner: run an undirected (mod-π) 2-means over the corner's folded
//!    chords, seeded at `(g0, g1)`. The two resulting centers are the corner's
//!    two local grid directions — **not** constrained to be orthogonal, so they
//!    track the local perspective. An empty cluster falls back to its global
//!    seed.
//!
//! # Precision contract
//!
//! Downstream axis, length, and fit gates are deliberately conservative, but
//! they do not prove a synthesized family or label correct. Callers should
//! validate precision and recall on their target domain; synthesized-axis
//! paths are currently evidence-limited/experimental as documented in the
//! crate README.
//!
//! # Undirected circular statistics
//!
//! Axis angles are undirected (period π): `θ` and `θ + π` are the same
//! direction. Every mean here accumulates `(cos 2θ, sin 2θ)` and halves the
//! `atan2` result; raw `(cos θ, sin θ)` accumulation would break at the 0/π
//! seam.

use nalgebra::Point2;

use crate::cluster::{angular_dist_pi as dist_pi, wrap_pi as fold_pi};
use crate::feature::{LocalAxis, OrientedFeature, PointFeature};
use crate::spatial::PointIndex;

/// Nearest neighbours used to estimate one corner's local grid directions.
/// Four is the principled count: an interior grid corner's four nearest
/// neighbours are exactly its axis neighbours (diagonals stay farther).
const K_AXIS_NEIGHBOURS: usize = 4;

/// Nearest neighbours used to estimate one hex corner's three local grid
/// directions. Six is the principled count: an interior hex node's six nearest
/// neighbours are exactly its six axial neighbours (`±` of the three axes).
const K_HEX_NEIGHBOURS: usize = 6;

/// Minimum chord length (pixels) for a neighbour edge to inform an estimate;
/// guards against coincident / duplicate points.
const MIN_CHORD_PX: f32 = 1e-3;

/// Histogram bins over `[0, π)` for the global mode search (2° per bin).
const GLOBAL_BINS: usize = 90;

/// Minimum separation (radians) required between the two global modes, and the
/// half-width suppressed around the first mode before searching for the second.
const MODE_MIN_SEPARATION: f32 = 0.349_065_85; // 20°

/// 2-means iterations per corner. The clusters are tiny (≈4 points) so this
/// converges immediately; a small fixed budget keeps it allocation-light.
const REFINE_ITERS: usize = 4;

/// Synthesize two local lattice axes for every point feature from neighbour
/// geometry, returning oriented-2 features that carry the same `source_index`
/// and position plus the recovered axes. The two axes are **not** constrained
/// to be orthogonal — they track the local projected grid directions.
///
/// The result is consumed by the internal topological assembler exactly like
/// caller-supplied oriented features.
pub fn synthesize_oriented2(features: &[PointFeature]) -> Vec<OrientedFeature<2>> {
    let positions: Vec<Point2<f32>> = features.iter().map(|f| f.position).collect();
    let n = positions.len();

    if n < 3 {
        // Not enough points to recover two directions; emit a benign default.
        // Such inputs cannot form a grid and are dropped downstream anyway.
        return features
            .iter()
            .map(|f| OrientedFeature::<2>::new(*f, ordered_axes(0.0, std::f32::consts::FRAC_PI_2)))
            .collect();
    }

    let tree = PointIndex::from_positions(&positions);

    // Per-corner folded chord angles to the k nearest neighbours, each
    // carrying an inverse-distance weight so a corner's closer (true axis)
    // neighbours dominate over a farther diagonal that sneaks into the k-set
    // at a grid boundary.
    let per_corner: Vec<Vec<(f32, f32)>> = (0..n)
        .map(|i| nearest_folded_chords(&tree, &positions, i))
        .collect();

    // Global two-mode prior over the pooled nearest-edge orientations.
    let (g0, g1) = global_two_modes(&per_corner);

    features
        .iter()
        .enumerate()
        .map(|(i, feat)| {
            let (a0, a1) = refine_axes(&per_corner[i], g0, g1);
            OrientedFeature::<2>::new(*feat, ordered_axes(a0, a1))
        })
        .collect()
}

/// Synthesize the *second* local lattice axis for every single-axis feature,
/// keeping the caller-supplied axis as the first.
///
/// `Evidence::Oriented1` carries one trusted direction per corner (e.g. a
/// detector that recovers a single dominant edge orientation but not the
/// orthogonal one). This recovers the missing direction from neighbour
/// geometry — the same perspective-invariant fold-mod-π / undirected-2-means
/// fold-mod-π / undirected-2-means machinery as [`synthesize_oriented2`] — but anchors one cluster at the
/// supplied axis instead of seeding both from the global modes. The supplied
/// axis is trusted as evidence and is *not* moved; only the second cluster is
/// recovered from the chords that fall closer to it than to the supplied axis.
///
/// The result is consumed by the internal topological assembler exactly like
/// caller-supplied [`OrientedFeature<2>`]. The public detection facade funnels
/// `Oriented1` through this synthesis and then runs the square pipeline,
/// mirroring the `Positions` path.
///
/// The synthesized path is evidence-limited; downstream gates are conservative
/// but do not constitute a universal wrong-label guarantee.
pub fn synthesize_oriented2_from_oriented1(
    features: &[OrientedFeature<1>],
) -> Vec<OrientedFeature<2>> {
    let positions: Vec<Point2<f32>> = features.iter().map(|f| f.point.position).collect();
    let n = positions.len();

    if n < 3 {
        // Too few points to recover a second direction from neighbours. Keep
        // the supplied axis and seed the second orthogonally; such inputs
        // cannot form a grid and are dropped downstream regardless.
        return features
            .iter()
            .map(|f| {
                let a0 = fold_pi(f.axes[0].angle_rad);
                OrientedFeature::<2>::new(
                    f.point,
                    ordered_axes(a0, fold_pi(a0 + std::f32::consts::FRAC_PI_2)),
                )
            })
            .collect();
    }

    let tree = PointIndex::from_positions(&positions);

    let per_corner: Vec<Vec<(f32, f32)>> = (0..n)
        .map(|i| nearest_folded_chords(&tree, &positions, i))
        .collect();

    // Global two-mode prior, used only to seed the *second* cluster when a
    // corner's chords don't clearly split — never as the answer.
    let (g0, g1) = global_two_modes(&per_corner);

    features
        .iter()
        .enumerate()
        .map(|(i, feat)| {
            let known = fold_pi(feat.axes[0].angle_rad);
            // Seed the free cluster at whichever global mode is farther from
            // the known axis (the likely "other" grid direction).
            let seed1 = if dist_pi(g0, known) >= dist_pi(g1, known) {
                g0
            } else {
                g1
            };
            let second = refine_second_axis(&per_corner[i], known, seed1);
            OrientedFeature::<2>::new(feat.point, ordered_axes(known, second))
        })
        .collect()
}

/// Synthesize three local lattice axes for every point feature from neighbour
/// geometry — the **hex** analogue of [`synthesize_oriented2`].
///
/// A hexagonal point lattice has three axis families. Viewed in perspective the
/// three projected directions are neither 60° apart nor fixed across the image,
/// but the same perspective-invariant fact holds per axis: three collinear hex
/// nodes `(q−1, r), (q, r), (q+1, r)` stay collinear, so a node's `+`/`−`
/// neighbour chords along each axis are antipodal and collapse (mod π) onto one
/// direction. Folding the six nearest-neighbour chords into `[0, π)` therefore
/// yields three clusters — the three local grid directions.
///
/// # Algorithm (k = 3 generalization of [`synthesize_oriented2`])
///
/// 1. Per corner: fold the chord angles to its six nearest neighbours into
///    `[0, π)`.
/// 2. Pool every corner's folded chords into a global distribution and pick its
///    three dominant modes — a robust image-wide prior used only to *seed* the
///    per-corner estimate.
/// 3. Per corner: run an undirected (mod-π) 3-means seeded at the global modes.
///    The three centers are the corner's three local grid directions, with no
///    inter-axis-angle constraint so they track the local perspective.
///
/// This hex path is experimental pending a real-image campaign. Its synthetic
/// regression fixtures cover geometry, noise, dropout, shuffle, modulo-π
/// seams, and clutter, but do not establish production readiness.
pub fn synthesize_oriented3(features: &[PointFeature]) -> Vec<OrientedFeature<3>> {
    let positions: Vec<Point2<f32>> = features.iter().map(|f| f.position).collect();
    let n = positions.len();

    if n < 4 {
        // Too few points to recover three directions from neighbours. Emit a
        // benign 0°/60°/120° default; such inputs cannot form a hex grid and
        // are dropped downstream regardless.
        let third = std::f32::consts::PI / 3.0;
        return features
            .iter()
            .map(|f| OrientedFeature::<3>::new(*f, ordered_axes3([0.0, third, 2.0 * third])))
            .collect();
    }

    let tree = PointIndex::from_positions(&positions);

    let per_corner: Vec<Vec<(f32, f32)>> = (0..n)
        .map(|i| nearest_folded_chords_k(&tree, &positions, i, K_HEX_NEIGHBOURS))
        .collect();

    let globals = global_k_modes::<3>(&per_corner);

    features
        .iter()
        .enumerate()
        .map(|(i, feat)| {
            let axes = refine_axes_k::<3>(&per_corner[i], globals);
            OrientedFeature::<3>::new(*feat, ordered_axes3(axes))
        })
        .collect()
}

/// Expand one trusted physical hex-axis family into the three local axis
/// observations consumed by the hex topological classifier.
///
/// The measured axis is copied byte-for-byte into slot zero. The other two
/// directions are inferred from six-neighbour chords with the measured axis
/// held fixed during modulo-π clustering. Their circular RMS spread becomes
/// `sigma_rad`; an unsupported cluster receives `π` uncertainty and is
/// therefore ignored by the downstream informative-axis gate.
pub(crate) fn synthesize_oriented3_from_oriented1(
    features: &[OrientedFeature<1>],
) -> Vec<OrientedFeature<3>> {
    let positions: Vec<Point2<f32>> = features.iter().map(|f| f.point.position).collect();
    if positions.len() < 4 {
        return features
            .iter()
            .map(|feature| {
                OrientedFeature::<3>::new(
                    feature.point,
                    [
                        feature.axes[0],
                        LocalAxis::new(0.0, Some(std::f32::consts::PI)),
                        LocalAxis::new(std::f32::consts::FRAC_PI_2, Some(std::f32::consts::PI)),
                    ],
                )
            })
            .collect();
    }

    let tree = PointIndex::from_positions(&positions);
    let per_corner: Vec<Vec<(f32, f32)>> = (0..positions.len())
        .map(|index| nearest_folded_chords_k(&tree, &positions, index, K_HEX_NEIGHBOURS))
        .collect();
    let globals = global_k_modes::<3>(&per_corner);

    features
        .iter()
        .enumerate()
        .map(|(index, feature)| {
            let known = fold_pi(feature.axes[0].angle_rad);
            let seeds = free_mode_seeds(globals, known);
            let mut free = refine_free_axes_from_known(&per_corner[index], known, seeds);
            free.sort_by(|a, b| a.0.total_cmp(&b.0));
            OrientedFeature::<3>::new(
                feature.point,
                [
                    feature.axes[0],
                    LocalAxis::new(free[0].0, Some(free[0].1)),
                    LocalAxis::new(free[1].0, Some(free[1].1)),
                ],
            )
        })
        .collect()
}

fn free_mode_seeds(globals: [f32; 3], known: f32) -> [f32; 2] {
    let mut ranked = globals.map(|angle| (dist_pi(angle, known), angle));
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.total_cmp(&b.1)));
    [ranked[0].1, ranked[1].1]
}

fn refine_free_axes_from_known(
    folded: &[(f32, f32)],
    known: f32,
    seeds: [f32; 2],
) -> [(f32, f32); 2] {
    let mut centers = seeds;
    for _ in 0..REFINE_ITERS {
        let mut accumulators = [UndirectedMean::default(); 2];
        for &(angle, weight) in folded {
            let distances = [
                dist_pi(angle, known),
                dist_pi(angle, centers[0]),
                dist_pi(angle, centers[1]),
            ];
            let best = (1..3)
                .min_by(|&a, &b| distances[a].total_cmp(&distances[b]))
                .unwrap_or(1);
            if distances[best] < distances[0] {
                accumulators[best - 1].push(angle, weight);
            }
        }
        for slot in 0..2 {
            centers[slot] = accumulators[slot].mean().unwrap_or(centers[slot]);
        }
    }

    let mut squared = [0.0_f32; 2];
    let mut weights = [0.0_f32; 2];
    let mut counts = [0_usize; 2];
    for &(angle, weight) in folded {
        let distances = [
            dist_pi(angle, known),
            dist_pi(angle, centers[0]),
            dist_pi(angle, centers[1]),
        ];
        let best = (1..3)
            .min_by(|&a, &b| distances[a].total_cmp(&distances[b]))
            .unwrap_or(1);
        if distances[best] < distances[0] {
            squared[best - 1] += weight * distances[best] * distances[best];
            weights[best - 1] += weight;
            counts[best - 1] += 1;
        }
    }
    let separated = dist_pi(centers[0], centers[1]) >= MODE_MIN_SEPARATION;
    [0, 1].map(|slot| {
        // A single chord has zero sample spread but no corroboration. Treat it
        // as uninformative, just like an empty/collapsed cluster, rather than
        // fabricating high-confidence direction evidence.
        let sigma = if separated && counts[slot] >= 2 && weights[slot] > 0.0 {
            (squared[slot] / weights[slot]).sqrt()
        } else {
            std::f32::consts::PI
        };
        (centers[slot], sigma)
    })
}

/// Recover the second grid direction with the first cluster *pinned* at the
/// supplied `known` axis. Chords closer (mod π) to `known` are assigned to the
/// anchored cluster and ignored for the mean; chords closer to the free
/// cluster refine it via the undirected `(cos 2θ, sin 2θ)` mean. An empty free
/// cluster keeps its global seed.
fn refine_second_axis(folded: &[(f32, f32)], known: f32, seed1: f32) -> f32 {
    if folded.is_empty() {
        return seed1;
    }
    let mut c1 = seed1;
    for _ in 0..REFINE_ITERS {
        let mut acc1 = UndirectedMean::default();
        for &(a, w) in folded {
            // Assign to the free cluster only when it is the closer center;
            // the anchored `known` cluster absorbs the rest but stays fixed.
            if dist_pi(a, c1) < dist_pi(a, known) {
                acc1.push(a, w);
            }
        }
        c1 = acc1.mean().unwrap_or(c1);
    }
    c1
}

/// Folded (`[0, π)`) chord angles from corner `i` to its `K_AXIS_NEIGHBOURS`
/// nearest neighbours, each paired with an inverse-distance weight.
fn nearest_folded_chords(
    tree: &PointIndex,
    positions: &[Point2<f32>],
    i: usize,
) -> Vec<(f32, f32)> {
    nearest_folded_chords_k(tree, positions, i, K_AXIS_NEIGHBOURS)
}

/// Folded (`[0, π)`) chord angles from corner `i` to its `k` nearest
/// neighbours, each paired with an inverse-distance weight. The `k = 4`
/// specialization backs [`synthesize_oriented2`]; `k = 6` backs
/// [`synthesize_oriented3`] (an interior hex node's six nearest neighbours are
/// its six axial neighbours).
fn nearest_folded_chords_k(
    tree: &PointIndex,
    positions: &[Point2<f32>],
    i: usize,
    k: usize,
) -> Vec<(f32, f32)> {
    let p = positions[i];
    // `+ 1` because the nearest hit is the query point itself.
    let hits = tree.nearest_n(p, k + 1);

    let mut out = Vec::with_capacity(k);
    for nn in hits {
        let j = nn.item;
        if j == i {
            continue;
        }
        let q = positions[j];
        let dx = q.x - p.x;
        let dy = q.y - p.y;
        let d = (dx * dx + dy * dy).sqrt();
        if d <= MIN_CHORD_PX {
            continue;
        }
        out.push((fold_pi(dy.atan2(dx)), 1.0 / d));
    }
    out
}

/// Find the two dominant grid-edge orientations across the whole image from a
/// smoothed circular (mod-π) histogram of every corner's nearest-edge angles.
///
/// Returns `(g0, g1)` with `g1` at least [`MODE_MIN_SEPARATION`] from `g0`.
/// Falls back to an orthogonal pair only when the data exposes a single
/// direction — and only as a *seed* for the per-corner refinement, which still
/// adapts to the true local angle.
fn global_two_modes(per_corner: &[Vec<(f32, f32)>]) -> (f32, f32) {
    let bin_w = std::f32::consts::PI / GLOBAL_BINS as f32;
    let mut hist = [0.0_f32; GLOBAL_BINS];
    let mut total = 0usize;
    for chords in per_corner {
        for &(a, w) in chords {
            let mut b = (a / bin_w) as usize;
            if b >= GLOBAL_BINS {
                b = GLOBAL_BINS - 1;
            }
            hist[b] += w;
            total += 1;
        }
    }
    if total == 0 {
        return (0.0, std::f32::consts::FRAC_PI_2);
    }

    let smoothed = smooth_circular(&hist);
    let g0_bin = argmax(&smoothed);
    let g0 = (g0_bin as f32 + 0.5) * bin_w;

    // Suppress a window around g0 (mod π) and find the next peak.
    let suppress = (MODE_MIN_SEPARATION / bin_w).ceil() as i32;
    let mut best_bin = None;
    let mut best_val = 0.0_f32;
    for (b, &v) in smoothed.iter().enumerate() {
        let circ = circular_bin_distance(b as i32, g0_bin as i32, GLOBAL_BINS as i32);
        if circ <= suppress {
            continue;
        }
        if v > best_val {
            best_val = v;
            best_bin = Some(b);
        }
    }
    let g1 = match best_bin {
        Some(b) if best_val > 0.0 => (b as f32 + 0.5) * bin_w,
        // Only one direction is globally visible. Seed the second axis
        // orthogonally; per-corner refinement still recovers the true local
        // (non-orthogonal) angle wherever the data supports it.
        _ => fold_pi(g0 + std::f32::consts::FRAC_PI_2),
    };
    (g0, g1)
}

/// Find the `K` dominant grid-edge orientations across the whole image from a
/// smoothed circular (mod-π) histogram of every corner's nearest-edge angles.
///
/// Generalizes [`global_two_modes`] to `K` modes via greedy non-maximum
/// suppression: pick the global peak, suppress a [`MODE_MIN_SEPARATION`] window
/// around it, repeat. Returns `K` centers sorted ascending in `[0, π)`. When
/// fewer than `K` distinct peaks exist, the remaining slots are filled by
/// spreading the found modes uniformly — they only *seed* the per-corner
/// refinement, which still adapts to the true local angle.
fn global_k_modes<const K: usize>(per_corner: &[Vec<(f32, f32)>]) -> [f32; K] {
    let pi = std::f32::consts::PI;
    let bin_w = pi / GLOBAL_BINS as f32;
    let mut hist = [0.0_f32; GLOBAL_BINS];
    let mut total = 0usize;
    for chords in per_corner {
        for &(a, w) in chords {
            let mut b = (a / bin_w) as usize;
            if b >= GLOBAL_BINS {
                b = GLOBAL_BINS - 1;
            }
            hist[b] += w;
            total += 1;
        }
    }
    // Uniform fallback when there is no data.
    let mut out = [0.0_f32; K];
    for (k, slot) in out.iter_mut().enumerate() {
        *slot = fold_pi(k as f32 * pi / K as f32);
    }
    if total == 0 {
        return out;
    }

    let smoothed = smooth_circular(&hist);
    let suppress = (MODE_MIN_SEPARATION / bin_w).ceil() as i32;
    let mut chosen_bins: Vec<i32> = Vec::with_capacity(K);
    for slot in 0..K {
        let mut best_bin: Option<usize> = None;
        let mut best_val = 0.0_f32;
        for (b, &v) in smoothed.iter().enumerate() {
            // Skip bins too close to any already-chosen mode.
            if chosen_bins
                .iter()
                .any(|&c| circular_bin_distance(b as i32, c, GLOBAL_BINS as i32) <= suppress)
            {
                continue;
            }
            if v > best_val {
                best_val = v;
                best_bin = Some(b);
            }
        }
        match best_bin {
            Some(b) if best_val > 0.0 => {
                chosen_bins.push(b as i32);
                out[slot] = (b as f32 + 0.5) * bin_w;
            }
            // No further distinct peak: spread the remaining slots uniformly
            // off the first found mode so every slot stays defined.
            _ => {
                let base = out[0];
                out[slot] = fold_pi(base + slot as f32 * pi / K as f32);
            }
        }
    }
    out.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// Undirected (mod-π) `K`-means over a corner's folded chord angles, seeded at
/// the global modes. Returns the `K` cluster centers — the corner's `K` local
/// grid directions, unconstrained in their separation. An empty cluster keeps
/// its global seed so every slot stays defined.
fn refine_axes_k<const K: usize>(folded: &[(f32, f32)], seeds: [f32; K]) -> [f32; K] {
    let mut centers = seeds;
    if folded.is_empty() {
        return centers;
    }
    for _ in 0..REFINE_ITERS {
        let mut acc = [UndirectedMean::default(); K];
        for &(a, w) in folded {
            // Assign to the nearest center under the undirected (mod-π) metric;
            // ties go to the lowest index for determinism.
            let mut best = 0usize;
            let mut best_d = dist_pi(a, centers[0]);
            for (k, &c) in centers.iter().enumerate().skip(1) {
                let d = dist_pi(a, c);
                if d < best_d {
                    best_d = d;
                    best = k;
                }
            }
            acc[best].push(a, w);
        }
        for (k, a) in acc.iter().enumerate() {
            centers[k] = a.mean().unwrap_or(centers[k]);
        }
    }
    centers
}

/// Order three directions ascending into `[0, π)` and wrap into the
/// `OrientedFeature<3>` axis array.
fn ordered_axes3(mut a: [f32; 3]) -> [LocalAxis; 3] {
    a.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    [
        LocalAxis::new(a[0], None),
        LocalAxis::new(a[1], None),
        LocalAxis::new(a[2], None),
    ]
}

/// Undirected (mod-π) 2-means over a corner's folded chord angles, seeded at
/// the global modes. Returns the two cluster centers — the corner's two local
/// grid directions, unconstrained in their separation.
fn refine_axes(folded: &[(f32, f32)], g0: f32, g1: f32) -> (f32, f32) {
    if folded.is_empty() {
        return (g0, g1);
    }
    let (mut c0, mut c1) = (g0, g1);
    for _ in 0..REFINE_ITERS {
        let mut acc0 = UndirectedMean::default();
        let mut acc1 = UndirectedMean::default();
        for &(a, w) in folded {
            if dist_pi(a, c0) <= dist_pi(a, c1) {
                acc0.push(a, w);
            } else {
                acc1.push(a, w);
            }
        }
        // An empty cluster keeps its global seed so the slot stays defined.
        c0 = acc0.mean().unwrap_or(c0);
        c1 = acc1.mean().unwrap_or(c1);
    }
    (c0, c1)
}

/// Order two directions into the workspace axis convention:
/// `axes[0] ∈ [0, π)`, `axes[1] ∈ (axes[0], axes[0] + π)`. Since both inputs
/// are folded to `[0, π)`, this is an ascending sort.
fn ordered_axes(a: f32, b: f32) -> [LocalAxis; 2] {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    [LocalAxis::new(lo, None), LocalAxis::new(hi, None)]
}

/// Weighted undirected circular-mean accumulator over `[0, π)` via
/// `(Σ w cos 2θ, Σ w sin 2θ)`.
#[derive(Clone, Copy, Default)]
struct UndirectedMean {
    sum_cos: f32,
    sum_sin: f32,
    count: usize,
}

impl UndirectedMean {
    fn push(&mut self, theta: f32, weight: f32) {
        self.sum_cos += weight * (2.0 * theta).cos();
        self.sum_sin += weight * (2.0 * theta).sin();
        self.count += 1;
    }

    fn mean(&self) -> Option<f32> {
        if self.count == 0 || self.sum_cos.hypot(self.sum_sin) < 1e-6 {
            return None;
        }
        Some(fold_pi(0.5 * self.sum_sin.atan2(self.sum_cos)))
    }
}

/// Circular box-smooth (mod the histogram length) with a ±2-bin window.
fn smooth_circular(hist: &[f32; GLOBAL_BINS]) -> [f32; GLOBAL_BINS] {
    let mut out = [0.0_f32; GLOBAL_BINS];
    let n = GLOBAL_BINS as i32;
    for (i, slot) in out.iter_mut().enumerate() {
        let mut s = 0.0_f32;
        for d in -2..=2 {
            let idx = ((i as i32 + d) % n + n) % n;
            s += hist[idx as usize];
        }
        *slot = s;
    }
    out
}

fn argmax(v: &[f32; GLOBAL_BINS]) -> usize {
    let mut best = 0usize;
    let mut best_val = v[0];
    for (i, &x) in v.iter().enumerate() {
        if x > best_val {
            best_val = x;
            best = i;
        }
    }
    best
}

/// Circular distance between two bin indices over `n` bins.
#[inline]
fn circular_bin_distance(a: i32, b: i32, n: i32) -> i32 {
    let d = (a - b).abs() % n;
    d.min(n - d)
}

#[cfg(test)]
mod tests;
