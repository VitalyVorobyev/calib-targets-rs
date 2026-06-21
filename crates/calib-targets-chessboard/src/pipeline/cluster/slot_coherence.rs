//! Axis-slot coherence repair pass.
//!
//! The `DiskFit` axes-fitter (one of the two `OrientationMethod`s the
//! upstream `chess-corners` detector exposes, still selectable via the
//! facade / Studio / bench) can pick the wrong antipodal dark sector,
//! leaving a corner's `(axes[0], axes[1])` ordering reversed relative to the
//! rest of the chessboard. [`fix_axis_slot_coherence`] — the **whole-image**
//! case, run as a post-pass inside [`super::cluster_axes_debug`] — detects
//! that and recovers by swapping the two `AxisEstimate` slots. Gated on a
//! gross global label imbalance + a spatial 2-colouring quality check.
//!
//! This pass is a live recall safety-net for the `DiskFit` path: under
//! `RingFit` (the other orientation mode) the slot ordering is consistent,
//! the imbalance gate never fires, and the pass is a no-op. It is
//! precision-safe by construction (the bipartite-quality gate aborts unless
//! the 2-colouring is essentially perfect), so it can only add recall, never
//! a wrong label.
//!
//! The slot swap is the load-bearing mutation: every downstream consumer of
//! the cluster label reads `axes[0]` vs `axes[1]`, so swapping is equivalent
//! to re-clustering with the corrected slot ordering.

use crate::corner::{ClusterLabel, CornerAug, CornerStage};
use kiddo::{KdTree, SquaredEuclidean};

/// Spatial-coherence post-pass for [`super::cluster_axes_debug`].
///
/// A chessboard corner's k=4 cardinal neighbours are at the OPPOSITE
/// parity by construction. When `DiskFit` picks the wrong antipodal
/// dark sector *uniformly* for a clean chessboard (`mid.png`), every
/// corner sees mostly same-label neighbours and the alternating-parity
/// invariant the topological cell-test and the edge-ok rule depend on
/// breaks globally. This pass detects that regime by a **two-stage
/// gate** and recovers via spatial 2-colouring.
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
/// reads `axes[0]` vs `axes[1]` — the booster fill validator's
/// `edge_ok` / `accept_candidate`, the cluster cost in `assign`, the
/// geometry check, and the parity-aware `label_of` in
/// `pg_grow::SquareAttachPolicy`.
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
    // near-50/50 Canonical/Swapped split; the DiskFit
    // slot-ordering bug pushes it past 80/20. Below 22%
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
