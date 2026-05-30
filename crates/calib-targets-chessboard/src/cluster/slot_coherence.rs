//! Axis-slot coherence repair passes.
//!
//! chess-corners 0.9's `DiskFit` axes-fitter can pick the wrong antipodal
//! dark sector, leaving a corner's `(axes[0], axes[1])` ordering reversed
//! relative to the rest of the chessboard. Both passes here detect that and
//! recover by swapping the two `AxisEstimate` slots:
//!
//! - [`fix_axis_slot_coherence`] — the **whole-image** case, run as a
//!   post-pass inside [`super::cluster_axes_debug`]. Gated on a gross global
//!   label imbalance + a spatial 2-colouring quality check.
//! - [`fix_partial_slot_flips_post_stage6`] — the **partial** case, run after
//!   the Stage-6 BFS has produced a labelled set that serves as parity
//!   ground truth.
//!
//! The slot swap is the load-bearing mutation: every downstream consumer
//! (`edge_ok`, `assign_corner`, the rescue validator) reads `axes[0]` vs
//! `axes[1]`, so swapping is equivalent to re-clustering with the corrected
//! slot ordering.

use crate::corner::{ClusterLabel, CornerAug, CornerStage};
use kiddo::{KdTree, SquaredEuclidean};

/// Spatial-coherence post-pass for [`super::cluster_axes_debug`].
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
/// with the 2-colouring (swapping their two [`calib_targets_core::AxisEstimate`] slots).
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
pub(super) fn fix_axis_slot_coherence(corners: &mut [CornerAug]) {
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
///    swap its [`calib_targets_core::AxisEstimate`] slots (which flips the cluster label
///    by construction in [`super::assign_corner`]).
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
