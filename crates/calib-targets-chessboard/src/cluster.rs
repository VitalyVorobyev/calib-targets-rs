//! Axes-based orientation clustering for the detector.
//!
//! Computes the two global grid-direction centers `(Θ₀, Θ₁)` from the
//! per-corner `axes[0]` and `axes[1]` angles, then labels every
//! corner by matching its two axes against those two centers.
//!
//! # Why this differs from the workspace-level `cluster_orientations`
//!
//! `calib_targets_core::cluster_orientations` (post Phase-0 migration)
//! also clusters using axes. This module reuses its per-corner
//! `axes[0]` / `axes[1]` contributions but is **self-contained** —
//! This module keeps its own histogram + 2-means implementation so its
//! per-stage debug surface is decoupled from the shared helper. The
//! algorithm is identical (double-angle circular mean over per-axis
//! votes; the double-angle trick is mandatory for undirected angles
//! modulo π).
//!
//! # Inputs / outputs
//!
//! * Input: a slice of [`CornerAug`] whose `axes` field is
//!   populated. Axes with sigma equal to the no-info sentinel (π)
//!   are skipped.
//! * Output:
//!   - `ClusterCenters { theta0, theta1 }` in `[0, π)` with
//!     `theta0 < theta1`.
//!   - A per-corner [`AxisCluster`] assignment.
//!
//! # Algorithm
//!
//! 1. Build a smoothed circular histogram on `[0, π)` with
//!    `num_bins` bins. For every corner and every axis `k ∈ {0, 1}`,
//!    add a vote at `wrap_pi(axes[k].angle)` with weight
//!    `strength / (1 + axes[k].sigma)`.
//! 2. Smooth with a `[1, 4, 6, 4, 1] / 16` circular kernel.
//! 3. Find local maxima. Keep peaks with total weight ≥
//!    `min_peak_weight_fraction × total`. Pick the two strongest
//!    peaks separated by at least `peak_min_separation_deg`.
//! 4. Refine centers via **double-angle** 2-means over per-axis
//!    votes. Each axis vote `θ` is mapped to `2θ` before averaging;
//!    the average is halved back — this is the correct undirected-
//!    angle (mod π) circular mean. Iterate up to `max_iters`.
//! 5. Per-corner label: for each corner, compute the two possible
//!    axis assignments (canonical vs swapped) and pick the cheaper.
//!    Require the LARGER distance in the winning assignment to be
//!    within `cluster_tol_deg`; otherwise the corner is unclustered.

use crate::circular_stats as cs;
use crate::corner::{ClusterLabel, CornerAug, CornerStage};
use crate::params::DetectorParams;
use kiddo::{KdTree, SquaredEuclidean};
use serde::Serialize;
use std::f32::consts::PI;

// Re-export the hoisted angle helpers under their old local names so
// sibling modules (`seed`, `grow`, `boosters`) keep their existing
// `use crate::cluster::{angular_dist_pi, wrap_pi, ...}` imports.
pub(crate) use crate::circular_stats::{angular_dist_pi, wrap_pi};

/// Result of clustering: two grid-direction centers in `[0, π)`
/// with `theta0 < theta1`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct ClusterCenters {
    pub theta0: f32,
    pub theta1: f32,
}

/// Per-corner assignment produced by [`cluster_axes`].
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum AxisCluster {
    /// Axes matched both centers within `cluster_tol_deg`, with the
    /// given slot assignment.
    Labeled {
        label: ClusterLabel,
        /// Worst per-axis distance to its matched center (radians).
        max_d_rad: f32,
    },
    /// The best assignment still left one axis further than
    /// `cluster_tol_deg` from its matched center.
    Unclustered { max_d_rad: f32 },
}

/// Stage-3 introspection captured during a single `cluster_axes_debug`
/// run. Surfaced through `DebugFrame` so an offline tool can plot the
/// histogram and check whether 2-means refinement walked off the
/// visible peaks. Local-only: never serialized into a public report.
#[derive(Clone, Debug, Serialize)]
pub struct ClusterDebug {
    /// Number of histogram bins spanning the `[0, π)` axis-angle range.
    pub num_bins: usize,
    /// Raw per-bin weighted vote counts before smoothing.
    pub histogram: Vec<f32>,
    /// The histogram after circular smoothing — the curve peak-picking runs on.
    pub smoothed: Vec<f32>,
    /// Sum of all bin weights — the normalizer for the peak-weight floor.
    pub total_weight: f32,
    /// Peak seeds picked from the smoothed histogram, in radians (`[0, π)`),
    /// before 2-means refinement. `None` when peak picking failed.
    pub peak_seeds_rad: Option<[f32; 2]>,
    /// Centers after 2-means refinement, in radians. `None` when peak
    /// picking failed (refinement isn't run).
    pub refined_centers_rad: Option<[f32; 2]>,
}

/// Run clustering over a slice of [`CornerAug`]. Mutates each
/// corner's `stage` and `label` fields in place.
///
/// Returns `Some(centers)` on success, `None` when fewer than two
/// qualifying peaks were found (the detector should return no
/// detection in that case).
///
/// Thin wrapper over [`cluster_axes_debug`]; callers wanting the
/// histogram + peak seeds should call `cluster_axes_debug` directly.
pub fn cluster_axes(corners: &mut [CornerAug], params: &DetectorParams) -> Option<ClusterCenters> {
    cluster_axes_debug(corners, params).0
}

/// Same as [`cluster_axes`] but also returns a [`ClusterDebug`] payload
/// with the smoothed histogram and the peak seeds — useful for offline
/// triage of clustering failures. The caller pays the cost of carrying
/// the histogram (a few KB).
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(level = "debug", skip_all, fields(num_corners = corners.len()))
)]
pub fn cluster_axes_debug(
    corners: &mut [CornerAug],
    params: &DetectorParams,
) -> (Option<ClusterCenters>, ClusterDebug) {
    let tuning = params.effective_tuning();
    let mut debug = ClusterDebug {
        num_bins: tuning.num_bins,
        histogram: Vec::new(),
        smoothed: Vec::new(),
        total_weight: 0.0,
        peak_seeds_rad: None,
        refined_centers_rad: None,
    };

    if corners.is_empty() || tuning.num_bins < 4 {
        return (None, debug);
    }

    let hist = build_histogram(corners, params);
    debug.histogram = hist.bins.clone();
    debug.total_weight = hist.total_weight;
    if hist.total_weight <= 0.0 {
        return (None, debug);
    }

    let smoothed = cs::smooth_circular_5(&hist.bins);
    debug.smoothed = smoothed.clone();

    let peak_opts = cs::PeakPickOptions::new(
        tuning.min_peak_weight_fraction,
        tuning.peak_min_separation_deg.to_radians(),
    );
    let Some((theta0_seed, theta1_seed)) =
        cs::pick_two_peaks(&smoothed, hist.total_weight, &peak_opts)
    else {
        return (None, debug);
    };
    debug.peak_seeds_rad = Some([theta0_seed, theta1_seed]);

    let votes = collect_axis_votes(corners);
    let (theta0, theta1) =
        cs::refine_2means_double_angle(&votes, [theta0_seed, theta1_seed], tuning.max_iters_2means);
    debug.refined_centers_rad = Some([theta0, theta1]);

    let (a, b) = if theta0 <= theta1 {
        (theta0, theta1)
    } else {
        (theta1, theta0)
    };
    let centers = ClusterCenters {
        theta0: a,
        theta1: b,
    };

    // Assign per-corner label. The effective per-corner tolerance is
    // `cluster_tol_rad + cluster_sigma_k * max(σ_a0, σ_a1)` so noisy
    // axes get proportional slack — see `AdvancedTuning::cluster_sigma_k`.
    let base_tol_rad = tuning.cluster_tol_deg.to_radians();
    let sigma_k = tuning.cluster_sigma_k;
    for corner in corners.iter_mut() {
        if !matches!(corner.stage, CornerStage::Strong) {
            continue;
        }
        let tol_rad = effective_tol_rad(corner, base_tol_rad, sigma_k);
        let assign = assign_corner(corner, centers, tol_rad);
        match assign {
            AxisCluster::Labeled { label, .. } => {
                corner.label = Some(label);
                corner.stage = CornerStage::Clustered { label };
            }
            AxisCluster::Unclustered { max_d_rad } => {
                corner.label = None;
                corner.stage = CornerStage::NoCluster {
                    max_d_deg: max_d_rad.to_degrees(),
                };
            }
        }
    }

    // Spatial-coherence pass: chess-corners 0.9's DiskFit can pick the
    // wrong antipodal dark sector for some chessboard corners, leaving
    // adjacent corners with the SAME axis-slot ordering instead of the
    // alternating pattern the BFS / seed / edge-invariant relies on. The
    // bug shows up as a same-label cluster of neighbours where a
    // chessboard demands opposite labels. Detect the offenders by
    // spatial majority vote and recover by swapping their two
    // `AxisEstimate` slots (which also flips the cluster label).
    //
    // Gated on a heavy label imbalance (one class < ~22% of the
    // total). RingFit produces ~50/50 balanced labels by construction
    // and is unaffected by this gate. DiskFit produces ~50/50 on
    // ChArUco-style images (small0..small5, target_7) where the
    // existing parity convention is fine — also unaffected by the
    // gate. The gate fires on clean-chessboard scenes where DiskFit's
    // antipodal-sector pick collapses to the same physical axis for
    // most corners (mid.png 62/15 = 80% Canonical pre-fix).
    fix_axis_slot_coherence(corners);

    (Some(centers), debug)
}

/// Spatial-coherence post-pass for [`cluster_axes_debug`].
///
/// A chessboard corner's k=4 cardinal neighbours are at the OPPOSITE
/// parity by construction. When chess-corners 0.9 DiskFit picks the
/// wrong antipodal dark sector *uniformly* for a clean chessboard
/// (`mid.png`), every corner sees mostly same-label neighbours and
/// the alternating-parity invariant the BFS / seed / edge-ok rule
/// depends on breaks globally. This pass detects that regime by a
/// **two-stage gate** and recovers via spatial 2-colouring.
///
/// Stage 1 — gross-imbalance gate. The cluster step's per-corner
/// Canonical/Swapped split is computed before any rebalancing pass
/// runs. A real, correctly-clustered chessboard scene produces a
/// near-50/50 split; the slot-ordering bug pushes it to ≥ 80/20. We
/// only enter Stage 2 when the minority class is below 22% of the
/// total.
///
/// Stage 2 — spatial 2-colouring. Build an adjacency graph between
/// Clustered corners at cell-spacing distance, BFS-2-colour from the
/// strongest corner, and flip whichever Clustered corners disagree
/// with the 2-colouring (swapping their two [`AxisEstimate`] slots).
/// The slot swap propagates through every downstream consumer that
/// reads `axes[0]` vs `axes[1]` — `edge_ok` (BFS, rescue, seed,
/// geometry check), `assign_corner`'s canonical / swapped cost, and
/// the parity-aware `label_of` in `pg_grow::SquareAttachPolicy`.
///
/// On RingFit (where axis-slot ordering is consistent) Stage 1
/// always passes (~50/50 split) and Stage 2 never runs. On DiskFit
/// applied to ChArUco / multi-board scenes where marker-internal
/// corners skew the global cluster split (e.g.
/// `puzzleboard_reference/example6.png` reaches 84/16 RingFit)
/// Stage 2 still runs but the 2-colouring's adjacency graph is
/// dominated by chessboard intersections (cardinal-distance gate
/// excludes marker-internal cross-pairs); chessboard intersections
/// satisfy the alternating invariant against the anchor's parity
/// already, so the bipartite-quality gate (Stage 3) aborts the
/// pass. The clean-chessboard slot-flip case (`mid.png`) is where
/// the pass actually flips most labels.
fn fix_axis_slot_coherence(corners: &mut [CornerAug]) {
    // Snapshot positions and the original ClusterLabel of every
    // Clustered corner. `clustered_indices[slot]` gives the index into
    // the caller's `corners` slice so we can mutate the right entry
    // when we flip.
    let mut clustered_indices: Vec<usize> = Vec::new();
    let mut clustered_pos: Vec<[f32; 2]> = Vec::new();
    let mut clustered_label: Vec<ClusterLabel> = Vec::new();
    let mut clustered_strength: Vec<f32> = Vec::new();
    for (idx, c) in corners.iter().enumerate() {
        if let CornerStage::Clustered { label } = c.stage {
            clustered_indices.push(idx);
            clustered_pos.push([c.position.x, c.position.y]);
            clustered_label.push(label);
            clustered_strength.push(c.strength);
        }
    }
    // < 5 corners is too small for the chessboard pipeline to do
    // anything useful with anyway (well below `min_labeled_corners`).
    let n = clustered_indices.len();
    if n < 5 {
        return;
    }

    // Stage 1 — gross-imbalance gate. Real chessboards produce a
    // near-50/50 Canonical/Swapped split; the chess-corners 0.9
    // DiskFit slot-ordering bug pushes it past 80/20. Below 22%
    // minority class is the regime we recover from.
    let canonical_count = clustered_label
        .iter()
        .filter(|&&l| l == ClusterLabel::Canonical)
        .count();
    let min_class = canonical_count.min(n - canonical_count);
    let imbalance_floor_frac = 0.22_f32;
    if (min_class as f32) >= imbalance_floor_frac * (n as f32) {
        return;
    }

    // Stage 2 — spatial 2-colouring.
    //
    // Build a kd-tree of clustered positions for the cell-spacing
    // estimate and the adjacency lookup.
    let mut tree: KdTree<f32, 2> = KdTree::new();
    for (slot, pos) in clustered_pos.iter().enumerate() {
        tree.add(pos, slot as u64);
    }

    // Estimate cell spacing as the median nearest-neighbour distance
    // across clustered corners. On a real chessboard, cardinal
    // neighbours dominate the nearest-neighbour distribution (diagonals
    // are √2× farther). On multi-board scenes the median can pick up
    // marker-internal pairs at smaller scale — but Stage 1's
    // imbalance gate already ruled out that regime.
    let mut nn_dists: Vec<f32> = Vec::with_capacity(n);
    for pos in &clustered_pos {
        let nbrs = tree.nearest_n::<SquaredEuclidean>(pos, 2);
        // The kd-tree returns the corner itself first; we want the
        // second closest.
        if let Some(nn) = nbrs.into_iter().nth(1) {
            nn_dists.push(nn.distance.sqrt());
        }
    }
    if nn_dists.is_empty() {
        return;
    }
    nn_dists.sort_by(|a, b| a.total_cmp(b));
    let median_nn = nn_dists[nn_dists.len() / 2];
    if !median_nn.is_finite() || median_nn <= 0.0 {
        return;
    }

    // Adjacency: a slot is connected to slots within
    // `[median_nn × 0.55, median_nn × 1.20]`. The upper bound is
    // below √2 ≈ 1.414× to keep diagonal pairs out; the lower bound
    // tolerates mild perspective foreshortening.
    let r_max = median_nn * 1.20;
    let r2_max = r_max * r_max;
    let r_min2 = (median_nn * 0.55) * (median_nn * 0.55);

    let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); n];
    for slot in 0..n {
        let pos = &clustered_pos[slot];
        let nbrs = tree.within_unsorted::<SquaredEuclidean>(pos, r2_max);
        for nbr in nbrs {
            let other_slot = nbr.item as usize;
            if other_slot == slot {
                continue;
            }
            if nbr.distance < r_min2 {
                continue;
            }
            adjacency[slot].push(other_slot);
        }
    }

    // BFS-2-colour from the strongest corner (most reliable label)
    // outward through the adjacency graph. `parity[slot]` ∈ {Some(0),
    // Some(1), None}; None means "not yet reached by BFS".
    let mut anchor = 0usize;
    let mut anchor_strength = clustered_strength[0];
    for (slot, &s) in clustered_strength.iter().enumerate().skip(1) {
        if s > anchor_strength {
            anchor = slot;
            anchor_strength = s;
        }
    }

    let mut parity: Vec<Option<u8>> = vec![None; n];
    parity[anchor] = Some(0);
    let mut queue: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
    queue.push_back(anchor);
    while let Some(slot) = queue.pop_front() {
        let p = parity[slot].expect("slot enqueued only after parity set");
        let opp = 1 - p;
        for &nbr in &adjacency[slot] {
            if parity[nbr].is_none() {
                parity[nbr] = Some(opp);
                queue.push_back(nbr);
            }
        }
    }

    // Stage 3 — bipartite-quality gate. Compute the proposed expected
    // label per slot from the 2-colouring, then check how cleanly the
    // adjacency graph 2-colours. On a clean chessboard with the
    // slot-ordering bug, every adjacency edge connects opposite
    // expected labels (perfect bipartite). On multi-board / ChArUco
    // scenes that slipped past Stage 1, the marker corners' adjacency
    // edges connect same-expected-label slots and the bipartite ratio
    // is far from 1.0; abort the fix to preserve the existing
    // (correct) labels.
    let anchor_label = clustered_label[anchor];
    let expected_label_of = |slot: usize| -> Option<ClusterLabel> {
        let p = parity[slot]?;
        Some(if p == 0 {
            anchor_label
        } else {
            anchor_label.flipped()
        })
    };

    let mut bipartite_edges = 0usize;
    let mut total_edges = 0usize;
    for (slot, nbrs) in adjacency.iter().enumerate() {
        let Some(my) = expected_label_of(slot) else {
            continue;
        };
        for &nbr in nbrs {
            // Count each edge once.
            if nbr <= slot {
                continue;
            }
            let Some(other) = expected_label_of(nbr) else {
                continue;
            };
            total_edges += 1;
            if my != other {
                bipartite_edges += 1;
            }
        }
    }
    if total_edges == 0 {
        return;
    }
    // Threshold at 0.92: a clean chessboard with the slot-ordering
    // bug uniformly applied has bipartite ratio = 1.0 (every cardinal
    // edge connects opposite parities by construction). ChArUco /
    // multi-board scenes drop into the 0.5–0.85 range because
    // cardinal-distance edges between marker-internal corners /
    // boundary regions are not bipartite. The 0.92 cut sits
    // comfortably above the pathological scenes and below the clean
    // case.
    let bipartite_ratio = (bipartite_edges as f32) / (total_edges as f32);
    if bipartite_ratio < 0.92 {
        return;
    }

    // All gates passed — apply the flips. Corners at the SAME parity
    // as the anchor share its label, opposite parities take the
    // flipped label. Disconnected corners (no path from anchor
    // through cell-spaced edges) are left untouched — flipping them
    // by guess would violate the precision contract.
    for (slot, &p) in parity.iter().enumerate() {
        let Some(p) = p else {
            continue;
        };
        let expected = if p == 0 {
            anchor_label
        } else {
            anchor_label.flipped()
        };
        if clustered_label[slot] != expected {
            let idx = clustered_indices[slot];
            let c = &mut corners[idx];
            // Swap the two AxisEstimate slots. Downstream consumers
            // (edge_ok, assign_corner, seed_validator) all read
            // axes[0] vs axes[1], so the swap is the load-bearing
            // mutation; we additionally flip the cached cluster
            // label so it stays consistent with the new axis-slot
            // order.
            c.axes.swap(0, 1);
            c.stage = CornerStage::Clustered { label: expected };
            c.label = Some(expected);
        }
    }
}

/// Post-Stage-6 partial slot-flip correction.
///
/// chess-corners 0.9's `DiskFit` axes-fitter occasionally picks the
/// opposite antipodal dark sector for a small subset of clean
/// chessboard corners (~1–8% of clustered corners on real photos),
/// producing a per-corner `(axes[0], axes[1])` ordering that's the
/// reverse of what the rest of the chessboard expects. Such corners
/// have the right physical line directions but the wrong slot label,
/// so [`crate::grow::ChessboardSquareAttachPolicy::edge_ok`]'s alternating-
/// parity rule (`slot_c != slot_n`) rejects every edge from the
/// flipped corner to a labelled neighbour, and the corner can't be
/// attached to the grid.
///
/// [`fix_axis_slot_coherence`] handles the **whole-image** slot-flip
/// case (e.g. `mid.png` under DiskFit, where 80% of corners flip
/// together). It's gated on a gross global imbalance (Stage 1) and
/// uses a 2-colouring of the cell-spaced adjacency graph (Stages 2–3).
/// That gate (rightly) does not fire when the slot-flips are partial:
/// on `large.png` under DiskFit, only 22 of 585 clustered corners
/// are flipped and the global Canonical/Swapped split stays at ~50/50.
///
/// This function fills the gap by detecting partial slot-flips
/// **after Stage 6 BFS** has built a labelled set. The labelled set
/// provides ground-truth `(i, j)` parity at every attached cell, so
/// each remaining `Clustered` orphan can be checked against the
/// expected parity at its predicted cell.
///
/// # Algorithm
///
/// For each `Clustered` corner not in the labelled set:
///
/// 1. Find the K nearest **labelled** corners by euclidean distance
///    (`labelled_set_k_nearest`, default 12).
/// 2. Estimate a local homography `H: (i, j) → pixel` from those K
///    correspondences (DLT). Reject if the per-support residual cap
///    is exceeded (the local-H is unreliable in this region).
/// 3. Apply `H⁻¹` to the orphan's pixel position; round to integer
///    `(i_pred, j_pred)`.
/// 4. Position-match gate: `||orphan_pos - H(i_pred, j_pred)|| ≤
///    0.4 × cell_size`.
/// 5. Cell-empty gate: `(i_pred, j_pred)` not already labelled.
/// 6. Determine the **expected slot label** at `(i_pred, j_pred)` by
///    parity-extrapolation from the K labelled supports. Each support
///    `s` contributes a vote: same parity ⇒ same label as `s`,
///    opposite parity ⇒ opposite label. Take the majority.
/// 7. If the orphan's current label disagrees with the expected,
///    swap its [`AxisEstimate`] slots (which flips the cluster label
///    by construction in [`assign_corner`]).
///
/// The slot swap is the load-bearing mutation; downstream consumers
/// (`edge_ok`, `accept_candidate` in the rescue validator) read
/// `axes[0]` vs `axes[1]` and the cached `label`, so swapping is
/// equivalent to re-clustering with the corrected slot ordering.
/// After this pass, Stage 6.5 picks up the corrected orphans and
/// attaches them via the standard local-H rescue path.
///
/// # Precision argument
///
/// The five gates (3 nearest labelled supports, residual cap, position
/// match, empty cell, parity match) compose to require that:
///
/// - The orphan sits at a chessboard lattice cell predicted by the
///   labelled set's local-H, within 0.4 cell of the prediction.
/// - The predicted cell isn't already attached.
/// - The orphan's slot ordering, **after a flip**, matches the
///   parity expected by the labelled set's majority vote.
///
/// A non-chessboard-lattice corner (marker corner, partial-cell
/// detection, image-border noise) fails the residual or position
/// gate. A chessboard corner whose slot ordering already agrees with
/// the labelled set passes step 6 with the correct label and is left
/// untouched. The only corners flipped are those whose physical
/// position matches a real chessboard cell AND whose slot disagrees
/// with the rest of the chessboard. By construction they cannot
/// introduce wrong `(i, j)` labels — the cell at which they'd be
/// attached has unambiguous parity defined by the surrounding
/// labelled set.
///
/// # Returns
///
/// The number of orphans whose axis slots were swapped.
pub(crate) fn fix_partial_slot_flips_post_stage6(
    corners: &mut [CornerAug],
    labelled: &std::collections::HashMap<(i32, i32), usize>,
    cell_size: f32,
    k_nearest: usize,
) -> u32 {
    use projective_grid::detect::advanced::square::homography::estimate_homography;

    if labelled.len() < 4 || cell_size <= 0.0 || k_nearest < 4 {
        return 0;
    }

    // Snapshot labelled positions for fast nearest-neighbour search.
    // (corner_idx, [x, y], (i, j), cluster_label)
    type LabelledRecord = (usize, [f32; 2], (i32, i32), ClusterLabel);
    let mut labelled_data: Vec<LabelledRecord> = Vec::new();
    for (&ij, &idx) in labelled {
        let c = &corners[idx];
        if let Some(label) = c.label {
            labelled_data.push((idx, [c.position.x, c.position.y], ij, label));
        }
    }
    if labelled_data.len() < 4 {
        return 0;
    }

    // Build a kd-tree over labelled positions for K-nearest search.
    let mut tree: KdTree<f32, 2> = KdTree::new();
    for (slot, (_, pos, _, _)) in labelled_data.iter().enumerate() {
        tree.add(pos, slot as u64);
    }

    // Collect orphan Clustered corners (not in the labelled set).
    let mut orphan_indices: Vec<usize> = Vec::new();
    for (idx, c) in corners.iter().enumerate() {
        if !matches!(c.stage, CornerStage::Clustered { .. }) {
            continue;
        }
        if labelled.values().any(|&v| v == idx) {
            // Defensive: if the BFS labelled it, stage should be Labeled,
            // not Clustered. But check anyway in case of ordering bugs.
            continue;
        }
        orphan_indices.push(idx);
    }

    let max_pos_resid_px = 0.4 * cell_size;
    let max_h_support_resid_px = 0.4 * cell_size;
    let mut flipped_count: u32 = 0;

    for &orphan_idx in &orphan_indices {
        let orphan_pos = corners[orphan_idx].position;
        let Some(orphan_label) = corners[orphan_idx].label else {
            continue;
        };

        // K nearest labelled corners by euclidean pixel distance.
        let knn = tree.nearest_n::<SquaredEuclidean>(&[orphan_pos.x, orphan_pos.y], k_nearest);
        if knn.len() < 4 {
            continue;
        }

        // Estimate H: (i, j) → pixel from K supports.
        let grid_pts: Vec<nalgebra::Point2<f32>> = knn
            .iter()
            .map(|nn| {
                let (_, _, ij, _) = labelled_data[nn.item as usize];
                nalgebra::Point2::new(ij.0 as f32, ij.1 as f32)
            })
            .collect();
        let img_pts: Vec<nalgebra::Point2<f32>> = knn
            .iter()
            .map(|nn| {
                let (_, pos, _, _) = labelled_data[nn.item as usize];
                nalgebra::Point2::new(pos[0], pos[1])
            })
            .collect();

        let Some(h) = estimate_homography(&grid_pts, &img_pts) else {
            continue;
        };

        // Per-support residual gate: every K-NN support must be
        // predicted within `max_h_support_resid_px` by H. A bad fit
        // (corner near image edge, distorted region, sparse labelled
        // neighbourhood) fails this and we skip the orphan.
        let mut max_resid: f32 = 0.0;
        for k in 0..grid_pts.len() {
            let pred = h.apply(grid_pts[k]);
            let dx = pred.x - img_pts[k].x;
            let dy = pred.y - img_pts[k].y;
            let r = (dx * dx + dy * dy).sqrt();
            if r > max_resid {
                max_resid = r;
            }
        }
        if max_resid > max_h_support_resid_px {
            continue;
        }

        // Predict (i, j) by inverting H. We don't have H⁻¹ in closed
        // form on `Homography`, but we can solve the implicit equation
        // by Newton iteration on a 2-D root: we want (i*, j*) such
        // that H(i*, j*) = orphan_pos. Initialise from the nearest
        // labelled support and step using the Jacobian estimated from
        // the H entries.
        //
        // Cheaper alternative used here: enumerate (i, j) candidates
        // around the nearest support's grid cell within ±2 in each
        // direction, compute H(i, j), and pick the one closest to
        // orphan_pos. This is robust and bounds search to 25 H-applies
        // per orphan.
        let nearest_support = labelled_data[knn[0].item as usize];
        let (ni, nj) = nearest_support.2;
        let mut best: Option<((i32, i32), f32, nalgebra::Point2<f32>)> = None;
        for di in -3i32..=3 {
            for dj in -3i32..=3 {
                let ij = (ni + di, nj + dj);
                let pred = h.apply(nalgebra::Point2::new(ij.0 as f32, ij.1 as f32));
                let dx = pred.x - orphan_pos.x;
                let dy = pred.y - orphan_pos.y;
                let r = (dx * dx + dy * dy).sqrt();
                match best {
                    None => best = Some((ij, r, pred)),
                    Some((_, br, _)) if r < br => best = Some((ij, r, pred)),
                    _ => {}
                }
            }
        }
        let Some((ij_pred, pos_resid, _pred_pos)) = best else {
            continue;
        };

        // Gate 4: position match against the predicted lattice point.
        if pos_resid > max_pos_resid_px {
            continue;
        }

        // Gate 5: predicted cell must be empty (not yet labelled).
        if labelled.contains_key(&ij_pred) {
            continue;
        }

        // Gate 6: expected parity at the predicted cell, by majority
        // vote among the K labelled supports' parity-extrapolated
        // labels. A support at `ij_s` with label `L_s` votes for label
        // `L_s` at `ij_pred` if `(ij_pred - ij_s)` has even parity, else
        // `L_s.flipped()`.
        let mut canon_votes: u32 = 0;
        let mut swap_votes: u32 = 0;
        for nn in &knn {
            let (_, _, ij_s, label_s) = labelled_data[nn.item as usize];
            let dij = (ij_pred.0 - ij_s.0) + (ij_pred.1 - ij_s.1);
            let same_parity = dij.rem_euclid(2) == 0;
            let expected = if same_parity {
                label_s
            } else {
                label_s.flipped()
            };
            match expected {
                ClusterLabel::Canonical => canon_votes += 1,
                ClusterLabel::Swapped => swap_votes += 1,
            }
        }
        // Strict majority required: < 2/3 supports agreeing means the
        // labelled set itself isn't parity-self-consistent in this
        // region (probably from a different component or a Stage 6.5
        // mis-attachment). Skip the orphan.
        let total_votes = canon_votes + swap_votes;
        let majority = canon_votes.max(swap_votes);
        if (majority as f32) < (total_votes as f32) * 2.0 / 3.0 {
            continue;
        }
        let expected_label = if canon_votes > swap_votes {
            ClusterLabel::Canonical
        } else {
            ClusterLabel::Swapped
        };

        // If the orphan's current label already matches, nothing to fix.
        if orphan_label == expected_label {
            continue;
        }

        // Swap. The swap propagates through every downstream consumer
        // that reads `axes[0]` vs `axes[1]` (edge_ok, accept_candidate
        // in the rescue validator) plus the cached cluster label. The
        // BFS already finished — we're not changing any existing label
        // — so the precision invariants on the labelled set are
        // unaffected.
        let c = &mut corners[orphan_idx];
        c.axes.swap(0, 1);
        c.stage = CornerStage::Clustered {
            label: expected_label,
        };
        c.label = Some(expected_label);
        flipped_count += 1;
    }

    flipped_count
}

/// Per-corner cluster admission threshold in radians.
///
/// `min(cluster_tol_rad + cluster_sigma_k * max(σ_a0, σ_a1),
///      cluster_tol_rad + max_sigma_bonus_rad)` — sigma bonus is
/// capped so a single noisy corner cannot blow open the gate. Sigmas
/// at the no-info sentinel (≈ π) are clamped to a finite ceiling.
///
/// The cap exists because admitting too many borderline corners
/// destabilises the seed finder: with more clustered candidates the
/// first-rank seed quad can land on a sub-grid spaced 1.4× cell
/// (sqrt(2)×, the diagonal step), which then grows a sparse,
/// inconsistent (i, j) frame. Confirmed empirically on
/// `puzzleboard_reference/example2.png` — uncapped bonus turned a
/// 134-label / 32-pixel-cell detection into a 127-label /
/// 45-pixel-cell detection with `SHIFT-INCONSISTENT` errors.
#[inline]
pub(crate) fn effective_tol_rad(corner: &CornerAug, base_tol_rad: f32, sigma_k: f32) -> f32 {
    if sigma_k <= 0.0 {
        return base_tol_rad;
    }
    // Hard cap on sigma input. The no-info sentinel is π, and any
    // sigma above ~10° on a real corner is already noise-dominated.
    let sigma_cap = 10.0_f32.to_radians();
    let s0 = corner.axes[0].sigma.clamp(0.0, sigma_cap);
    let s1 = corner.axes[1].sigma.clamp(0.0, sigma_cap);
    let bonus = sigma_k * s0.max(s1);
    // Hard cap on the bonus itself: never exceed +3° over the base
    // tolerance. This keeps the effective gate within `[base_tol,
    // base_tol + 3°]` regardless of sigma.
    let max_bonus = 3.0_f32.to_radians();
    base_tol_rad + bonus.min(max_bonus)
}

/// Refit cluster centres from the labelled set's axes only.
///
/// Stage-3 clustering on the full ChESS corner set produces centres
/// biased by marker-internal corners whose local axes don't agree
/// with the global chessboard grid (see CLAUDE.md "Evidence-driven
/// detector debugging" for the small3.png case study). After Stage 5
/// BFS, the labelled set is guaranteed to consist of true chessboard
/// intersections; their axes give an unbiased estimate of the grid
/// directions.
///
/// For each labelled corner, pick the slot assignment (Canonical /
/// Swapped) that minimises the cost under `old_centers` — same
/// tie-break rule as [`assign_corner`] — to determine which of its two
/// axes belongs to slot 0 vs slot 1. Accumulate `(cos 2θ, sin 2θ)`
/// per slot (undirected circular mean — mandated by the workspace
/// "axes-only" contract; see CLAUDE.md "Corner orientation contract"),
/// halve the atan2, wrap to `[0, π)`, and order so `θ0 < θ1`.
///
/// Returns `None` if `labelled_indices.len() < min_samples` (the
/// caller should keep the original centres).
pub fn refit_centers_from_labelled(
    corners: &[CornerAug],
    labelled_indices: &[usize],
    old_centers: ClusterCenters,
    min_samples: usize,
) -> Option<ClusterCenters> {
    if labelled_indices.len() < min_samples {
        return None;
    }
    let mut s0_re = 0.0_f32;
    let mut s0_im = 0.0_f32;
    let mut s1_re = 0.0_f32;
    let mut s1_im = 0.0_f32;
    for &idx in labelled_indices {
        let c = &corners[idx];
        let a0 = wrap_pi(c.axes[0].angle);
        let a1 = wrap_pi(c.axes[1].angle);
        let d_a0_t0 = angular_dist_pi(a0, old_centers.theta0);
        let d_a0_t1 = angular_dist_pi(a0, old_centers.theta1);
        let d_a1_t0 = angular_dist_pi(a1, old_centers.theta0);
        let d_a1_t1 = angular_dist_pi(a1, old_centers.theta1);
        let canon_cost = d_a0_t0 + d_a1_t1;
        let swap_cost = d_a0_t1 + d_a1_t0;
        let (a_to_t0, a_to_t1) = if canon_cost <= swap_cost {
            (a0, a1)
        } else {
            (a1, a0)
        };
        s0_re += (2.0 * a_to_t0).cos();
        s0_im += (2.0 * a_to_t0).sin();
        s1_re += (2.0 * a_to_t1).cos();
        s1_im += (2.0 * a_to_t1).sin();
    }
    let mut t0 = 0.5 * s0_im.atan2(s0_re);
    let mut t1 = 0.5 * s1_im.atan2(s1_re);
    while t0 < 0.0 {
        t0 += PI;
    }
    while t0 >= PI {
        t0 -= PI;
    }
    while t1 < 0.0 {
        t1 += PI;
    }
    while t1 >= PI {
        t1 -= PI;
    }
    if t0 > t1 {
        std::mem::swap(&mut t0, &mut t1);
    }
    Some(ClusterCenters {
        theta0: t0,
        theta1: t1,
    })
}

/// Pure assignment of one corner to a label given known centers —
/// exposed for tests and for the Stage-3 re-check in boosters.
pub fn assign_corner(corner: &CornerAug, centers: ClusterCenters, tol_rad: f32) -> AxisCluster {
    let a0 = wrap_pi(corner.axes[0].angle);
    let a1 = wrap_pi(corner.axes[1].angle);

    let d_a0_t0 = angular_dist_pi(a0, centers.theta0);
    let d_a0_t1 = angular_dist_pi(a0, centers.theta1);
    let d_a1_t0 = angular_dist_pi(a1, centers.theta0);
    let d_a1_t1 = angular_dist_pi(a1, centers.theta1);

    // Canonical: axes[0] → Θ₀, axes[1] → Θ₁. Cost = d(0,0)+d(1,1).
    let canon_cost = d_a0_t0 + d_a1_t1;
    let canon_max = d_a0_t0.max(d_a1_t1);
    // Swapped: axes[0] → Θ₁, axes[1] → Θ₀.
    let swap_cost = d_a0_t1 + d_a1_t0;
    let swap_max = d_a0_t1.max(d_a1_t0);

    let (label, max_d) = if canon_cost <= swap_cost {
        (ClusterLabel::Canonical, canon_max)
    } else {
        (ClusterLabel::Swapped, swap_max)
    };

    if max_d <= tol_rad {
        AxisCluster::Labeled {
            label,
            max_d_rad: max_d,
        }
    } else {
        AxisCluster::Unclustered { max_d_rad: max_d }
    }
}

// --- internals ------------------------------------------------------------

struct Histogram {
    bins: Vec<f32>,
    total_weight: f32,
}

fn build_histogram(corners: &[CornerAug], params: &DetectorParams) -> Histogram {
    let n = params.effective_tuning().num_bins;
    let mut bins = vec![0.0_f32; n];
    let mut total = 0.0_f32;
    for corner in corners {
        if !matches!(corner.stage, CornerStage::Strong) {
            continue;
        }
        for axis in &corner.axes {
            if !axis.sigma.is_finite() || axis.sigma >= PI - f32::EPSILON {
                // No-info sentinel → skip this axis.
                continue;
            }
            let w = weight(corner.strength, axis.sigma);
            if w <= 0.0 {
                continue;
            }
            let bin = cs::angle_to_bin(cs::wrap_pi(axis.angle), n);
            bins[bin] += w;
            total += w;
        }
    }
    Histogram {
        bins,
        total_weight: total,
    }
}

#[inline]
fn weight(strength: f32, sigma: f32) -> f32 {
    let s = strength.max(0.0);
    let base = if s > 0.0 { s } else { 1.0 };
    base / (1.0 + sigma.max(0.0))
}

/// Materialise per-axis votes in the shape expected by the hoisted
/// [`cs::refine_2means_double_angle`] helper.
fn collect_axis_votes(corners: &[CornerAug]) -> Vec<cs::AngleVote> {
    let mut votes: Vec<cs::AngleVote> = Vec::new();
    for corner in corners {
        if !matches!(corner.stage, CornerStage::Strong) {
            continue;
        }
        for axis in &corner.axes {
            if !axis.sigma.is_finite() || axis.sigma >= PI - f32::EPSILON {
                continue;
            }
            let w = weight(corner.strength, axis.sigma);
            if w <= 0.0 {
                continue;
            }
            votes.push(cs::AngleVote {
                angle: cs::wrap_pi(axis.angle),
                weight: w,
            });
        }
    }
    votes
}

// --- tests ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corner::ChessCorner;
    use calib_targets_core::AxisEstimate;
    use nalgebra::Point2;

    fn make_corner(
        input_index: usize,
        x: f32,
        y: f32,
        axis0_deg: f32,
        sigma_deg: f32,
        strength: f32,
    ) -> CornerAug {
        let a0 = axis0_deg.to_radians();
        let a1 = a0 + std::f32::consts::FRAC_PI_2;
        let sigma = sigma_deg.to_radians();
        let c = ChessCorner {
            position: Point2::new(x, y),
            axes: [
                AxisEstimate {
                    angle: wrap_pi(a0),
                    sigma,
                },
                AxisEstimate {
                    angle: wrap_pi(a1),
                    sigma,
                },
            ],
            contrast: 10.0,
            fit_rms: 1.0,
            strength,
        };
        let mut aug = CornerAug::from_chess_corner(input_index, &c);
        aug.stage = CornerStage::Strong;
        aug
    }

    // Deterministic pseudo-random jitter without pulling in `rand` as a
    // test dep — a small wrapping-linear-congruential generator is
    // plenty for tests that just need symmetric noise.
    fn jitter(i: usize, amp_deg: f32) -> f32 {
        // Hash-ish: multiply, shift, wrap to [-0.5, 0.5], scale.
        let x = (i as u32).wrapping_mul(2_654_435_761);
        let frac = ((x >> 8) as f32) / ((1u32 << 24) as f32); // [0,1)
        (frac - 0.5) * amp_deg
    }

    #[test]
    fn recovers_centers_30_120() {
        let mut corners = Vec::new();
        // Half parity-0 corners (axes[0] ≈ 30°, axes[1] ≈ 120°).
        for i in 0..50 {
            let j = jitter(i, 10.0);
            corners.push(make_corner(
                i,
                i as f32,
                0.0,
                30.0 + j,
                0.05_f32.to_radians(),
                1.0,
            ));
        }
        // Half parity-1 corners (axes[0] ≈ 120°, axes[1] ≈ 30°ish).
        for i in 0..50 {
            let j = jitter(i + 1000, 10.0);
            corners.push(make_corner(
                50 + i,
                i as f32,
                1.0,
                120.0 + j,
                0.05_f32.to_radians(),
                1.0,
            ));
        }

        let params = DetectorParams::default();
        let centers = cluster_axes(&mut corners, &params).expect("centers");
        // Expect peaks near 30° and 120° (Θ₀ < Θ₁ sort), with the
        // tightness of the jitter.
        let expected_low = 30.0_f32.to_radians();
        let expected_high = 120.0_f32.to_radians();
        assert!(
            angular_dist_pi(centers.theta0, expected_low) < 2.0_f32.to_radians(),
            "Θ₀ = {:.2}° off from 30°",
            centers.theta0.to_degrees()
        );
        assert!(
            angular_dist_pi(centers.theta1, expected_high) < 2.0_f32.to_radians(),
            "Θ₁ = {:.2}° off from 120°",
            centers.theta1.to_degrees()
        );
        // All strong corners should get a label.
        assert!(corners
            .iter()
            .all(|c| matches!(c.stage, CornerStage::Clustered { .. })));
    }

    #[test]
    fn parity_0_gets_canonical_parity_1_gets_swapped() {
        let mut corners = Vec::new();
        for i in 0..30 {
            // Parity-0: axes[0] at 0°, axes[1] at 90°.
            corners.push(make_corner(i, i as f32, 0.0, 0.0, 0.01, 1.0));
        }
        for i in 0..30 {
            // Parity-1: axes[0] at 90°, axes[1] at 180°→0°.
            corners.push(make_corner(30 + i, i as f32, 1.0, 90.0, 0.01, 1.0));
        }
        let params = DetectorParams::default();
        cluster_axes(&mut corners, &params).expect("centers");

        // Half Canonical, half Swapped.
        let canon = corners
            .iter()
            .filter(|c| matches!(c.label, Some(ClusterLabel::Canonical)))
            .count();
        let swap = corners
            .iter()
            .filter(|c| matches!(c.label, Some(ClusterLabel::Swapped)))
            .count();
        assert_eq!(canon + swap, 60, "every corner labeled");
        // At least half in each bucket — the exact split depends on
        // which peak sorts as Θ₀ (smaller angle wins).
        assert!(canon >= 25 && swap >= 25);
    }

    #[test]
    fn corner_far_from_both_centers_is_unclustered() {
        let mut corners = Vec::new();
        // 40 corners at 0°/90°.
        for i in 0..40 {
            corners.push(make_corner(i, i as f32, 0.0, 0.0, 0.01, 1.0));
        }
        // 1 misaligned corner — axes[0] at 25° (not matching any
        // cluster center within 12°).
        corners.push(make_corner(99, 0.0, 0.0, 25.0, 0.01, 1.0));

        let params = DetectorParams::default();
        cluster_axes(&mut corners, &params).expect("centers");

        let last = corners.last().expect("corners is non-empty");
        match &last.stage {
            CornerStage::NoCluster { .. } => {}
            other => unreachable!(
                "a corner with axes 25° off both centers must end in NoCluster, got {other:?}"
            ),
        }
        assert!(last.label.is_none());
    }

    #[test]
    fn empty_input_returns_none() {
        let mut corners: Vec<CornerAug> = Vec::new();
        let params = DetectorParams::default();
        assert!(cluster_axes(&mut corners, &params).is_none());
    }
}

#[cfg(test)]
mod plateau_peak_regression {
    //! Regression: when a physical direction falls on the `π` wrap
    //! boundary (ChESS reports `3.1415925 ≈ π − ε`, which `wrap_pi`
    //! leaves near `π` instead of folding to 0), the smoothed
    //! histogram gains two equal-height adjacent bins at 0 and
    //! `n − 1`. A strict `here > prev && here > next` peak check
    //! misses the flat-top plateau and `cluster_axes` returns
    //! `None`. This happens in practice on perfectly rectilinear
    //! synthetic puzzleboards (testdata example8/example9).
    //!
    //! See the plateau-aware branch in `pick_two_peaks`.
    use super::*;
    use crate::corner::{ChessCorner, CornerAug};
    use calib_targets_core::AxisEstimate;
    use nalgebra::Point2;

    #[test]
    fn near_pi_wrap_still_clusters() {
        // Use 3.1415925 (what the real ChESS adapter reports on the
        // synthetic puzzleboard) rather than f32::consts::PI, so the
        // wrap-boundary bug is reproduced exactly.
        const NEAR_PI: f32 = 3.1415925;
        let mut augs: Vec<CornerAug> = Vec::new();
        for j in 0..10_i32 {
            for i in 0..10_i32 {
                let swapped = (i + j).rem_euclid(2) == 1;
                let (a0, a1) = if swapped {
                    (std::f32::consts::FRAC_PI_2, NEAR_PI)
                } else {
                    (0.0_f32, std::f32::consts::FRAC_PI_2)
                };
                let c = ChessCorner {
                    position: Point2::new(i as f32 * 100.0 + 50.0, j as f32 * 100.0 + 50.0),
                    axes: [
                        AxisEstimate {
                            angle: a0,
                            sigma: 0.008,
                        },
                        AxisEstimate {
                            angle: a1,
                            sigma: 0.008,
                        },
                    ],
                    contrast: 136.0,
                    fit_rms: 4.7,
                    strength: 612.0,
                };
                let mut aug = CornerAug::from_chess_corner(augs.len(), &c);
                aug.stage = CornerStage::Strong;
                augs.push(aug);
            }
        }
        let params = DetectorParams::default();
        let centers =
            cluster_axes(&mut augs, &params).expect("near-π plateau must still yield two peaks");
        // Centers should settle at ≈0 and ≈π/2 after 2-means.
        assert!(
            angular_dist_pi(centers.theta0, 0.0) < 1.0_f32.to_radians(),
            "Θ₀ = {:.3}° too far from 0°",
            centers.theta0.to_degrees()
        );
        assert!(
            angular_dist_pi(centers.theta1, std::f32::consts::FRAC_PI_2) < 1.0_f32.to_radians(),
            "Θ₁ = {:.3}° too far from 90°",
            centers.theta1.to_degrees()
        );
        // Every input corner should now be clustered — on a perfect
        // grid there should be no stragglers.
        let n_clustered = augs
            .iter()
            .filter(|a| matches!(a.stage, CornerStage::Clustered { .. }))
            .count();
        assert_eq!(n_clustered, 100);
    }
}
