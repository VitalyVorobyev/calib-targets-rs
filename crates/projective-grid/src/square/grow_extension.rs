//! Boundary extension via fitted homography (Stage 6).
//!
//! Two strategies are available:
//!
//! - [`extend_via_global_homography`] — fits a single global H over the
//!   entire labelled set. Cheap and simple, but the residual gate
//!   refuses extrapolation under heavy radial distortion or
//!   multi-region perspective (where one global H cannot fit
//!   simultaneously). The labelled set must also be large enough for
//!   the global fit to dominate boundary noise.
//!
//! - [`extend_via_local_homography`] — fits a *per-candidate* H from
//!   the K nearest labelled corners (by grid distance). Each cell gets
//!   a local model that adapts to the local distortion regime; the
//!   per-candidate trust gate replaces the all-or-nothing global gate.
//!   Closer to APAP / moving-DLT in spirit. More compute (one DLT per
//!   candidate cell), but materially better recall on extreme-angle
//!   inputs and frames where a single H doesn't fit.
//!
//! Callers pick a strategy based on the expected input. The chessboard
//! detector uses `DetectorParams::stage6_local_h` to flip between
//! them; default global today, will flip to local once benchmarks
//! confirm parity / superset on every blessed image.
//!
//! # Single-strategy notes (global)
//!
//! After [`super::grow::bfs_grow`] produces a labelled set whose
//! geometry survives the validate stage, this pass fits a single
//! global homography `H : (i, j) → image_pixel` over the labels and
//! uses it to predict cells **outside** the labelled bounding box (and
//! interior holes the BFS missed). When the prediction lands within a
//! tight search radius of an eligible corner, *and* that corner
//! satisfies every gate the BFS validator imposes — parity, axis
//! cluster, soft per-edge invariant — the corner is attached.
//!
//! # Why this is necessary
//!
//! BFS-grow's prediction is **local**: it uses the per-neighbour
//! finite-difference of the labelled set as the grid step. At the
//! boundary, only one side of every neighbour is labelled — there's no
//! second-order information about how the cell pitch is changing. Under
//! perspective foreshortening, the actual cell pitch one step beyond
//! the labelled set is materially smaller than the local-step estimate.
//! BFS overshoots and growth terminates.
//!
//! A homography fitted from the labelled set captures perspective
//! exactly. The reprojection residuals on the labelled set then act
//! as a quantitative gate on whether the planar-target / pinhole
//! assumption is acceptable for the current frame.
//!
//! # Precision contract
//!
//! Stage 6 attachments must obey the same invariants as BFS attachments
//! (zero false-positive labels). Three layers of defence:
//!
//! 1. **Reprojection-residual gate.** Median and worst-case residual of
//!    `|H · (i, j) − pos(label)|` are measured on the labelled set; if
//!    either exceeds [`ExtensionParams::max_median_residual_rel`] /
//!    [`ExtensionParams::max_residual_rel`] (× `cell_size`), Stage 6
//!    refuses to extrapolate. This is the right knob for lens
//!    distortion: a moderate radial term inflates the residuals, the
//!    gate fires, and Stage 6 becomes a no-op.
//!
//! 2. **Same per-corner gates as BFS.** Candidate filtering uses the
//!    validator's `is_eligible` + `label_of` against
//!    `required_label_at` (parity), `accept_candidate` (axis-cluster
//!    match), AND `edge_ok` against at least one already-labelled
//!    cardinal neighbour (step length + axis-slot swap). Without these,
//!    Stage 6 could attach a wrong-parity or wrong-axis-slot corner that
//!    sits at the H prediction by accident.
//!
//! 3. **Single-claim guarantee.** Each attachment updates `by_corner`
//!    immediately, so a corner index can only be claimed by one cell.
//!    Two cells whose H predictions both land near the same physical
//!    corner cannot both attach it (the second sees `by_corner` already
//!    contains the index and falls through).
//!
//! Plus a *tighter* ambiguity gate ([`ExtensionParams::ambiguity_factor`]
//! default 2.5 vs BFS's 1.5): boundary errors are unrecoverable in
//! deeper iterations, so we'd rather miss than guess.
//!
//! # When this is a no-op
//!
//! - `labelled.len() < min_labels_for_h` (default 12).
//! - The DLT solver fails (degenerate quad) or the resulting H is
//!   numerically singular.
//! - The reprojection-residual gate fires.
//!
//! In all three cases the function returns without modifying the input
//! and `ExtensionStats::h_trusted` is `false`.

use std::collections::{HashMap, HashSet};

use kiddo::{KdTree, SquaredEuclidean};
use nalgebra::Point2;

use crate::homography::{estimate_homography_with_quality, HomographyQuality};
use crate::square::grow::{Admit, GrowResult, GrowValidator, LabelledNeighbour};

/// Tuning knobs for [`extend_via_global_homography`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct ExtensionParams {
    /// Minimum labelled count below which we refuse to fit a global H.
    /// 12 is enough for an over-determined 9-DOF DLT (3× over) on a
    /// non-degenerate quad layout.
    pub min_labels_for_h: usize,
    /// Maximum allowed *median* reprojection residual on the labelled
    /// set, expressed as a fraction of `cell_size`.
    pub max_median_residual_rel: f32,
    /// Maximum allowed *worst-case* reprojection residual on the
    /// labelled set, expressed as a fraction of `cell_size`. Set
    /// conservatively — moderate radial distortion can drive this over
    /// the threshold and disable extrapolation, which is the correct
    /// safety behaviour.
    pub max_residual_rel: f32,
    /// Search radius around `H · (cell)` predictions, expressed as a
    /// fraction of `cell_size`.
    pub search_rel: f32,
    /// Ambiguity gate: when the second-nearest candidate is within
    /// `factor × nearest`, the attachment is skipped. Tighter than
    /// BFS's 1.5 because boundary errors are unrecoverable.
    pub ambiguity_factor: f32,
    /// Per-pass cap on candidate cells to try. Each iteration also
    /// caps how many new attachments enable further extension.
    pub max_iters: u32,
}

impl Default for ExtensionParams {
    fn default() -> Self {
        Self {
            min_labels_for_h: 12,
            max_median_residual_rel: 0.10,
            max_residual_rel: 0.30,
            search_rel: 0.40,
            ambiguity_factor: 2.5,
            max_iters: 5,
        }
    }
}

/// Diagnostic counters returned by [`extend_via_global_homography`].
///
/// `attached_indices` lets callers identify Stage-6 attachments
/// distinct from Stage-5 BFS labels, e.g., for downstream blacklist
/// scoping or overlay rendering.
#[non_exhaustive]
#[derive(Clone, Debug, Default)]
pub struct ExtensionStats {
    pub iterations: u32,
    /// `None` when the H wasn't fit (too few labels or solver failure).
    /// Provided for diagnostics; not used as a trust gate (the
    /// reprojection-residual fields below are the right gate, since
    /// they're scale-aware in pixel units).
    pub h_quality: Option<HomographyQuality<f32>>,
    /// `None` when the H wasn't fit. Pixel units.
    pub h_residual_median_px: Option<f32>,
    pub h_residual_max_px: Option<f32>,
    /// `false` when the residual gate refused to extrapolate — the
    /// function is a no-op and `attached == 0`.
    pub h_trusted: bool,
    pub attached: u32,
    pub rejected_no_candidate: u32,
    pub rejected_ambiguous: u32,
    pub rejected_label: u32,
    pub rejected_validator: u32,
    pub rejected_edge: u32,
    /// Indices of the corners attached in this pass (provenance — useful
    /// for blacklist scope decisions when Stage 7 later rejects).
    pub attached_indices: Vec<usize>,
    /// `(i, j)` cells that survived to attachment.
    pub attached_cells: Vec<(i32, i32)>,
}

/// Try to extend the labelled grid outward (and into interior holes)
/// using a globally-fit homography. Mutates `grow.labelled` and
/// `grow.by_corner` in place.
pub fn extend_via_global_homography<V: GrowValidator>(
    positions: &[Point2<f32>],
    grow: &mut GrowResult,
    cell_size: f32,
    params: &ExtensionParams,
    validator: &V,
) -> ExtensionStats {
    let mut stats = ExtensionStats::default();

    if grow.labelled.len() < params.min_labels_for_h {
        return stats;
    }

    // Fit global H from the labelled set.
    let mut grid_pts: Vec<Point2<f32>> = Vec::with_capacity(grow.labelled.len());
    let mut img_pts: Vec<Point2<f32>> = Vec::with_capacity(grow.labelled.len());
    for (&(i, j), &idx) in &grow.labelled {
        grid_pts.push(Point2::new(i as f32, j as f32));
        img_pts.push(positions[idx]);
    }
    let Some((h, quality)) = estimate_homography_with_quality(&grid_pts, &img_pts) else {
        return stats;
    };
    stats.h_quality = Some(quality);

    // Reprojection residuals on the labelled set — the scale-aware
    // trust signal. Conditioning of the unnormalised 3×3 H matrix
    // (a ratio of singular values) varies with translation magnitude
    // and isn't a reliable gate on its own.
    let mut residuals: Vec<f32> = Vec::with_capacity(grid_pts.len());
    for k in 0..grid_pts.len() {
        let pred = h.apply(grid_pts[k]);
        let dx = pred.x - img_pts[k].x;
        let dy = pred.y - img_pts[k].y;
        residuals.push((dx * dx + dy * dy).sqrt());
    }
    residuals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_res = residuals[residuals.len() / 2];
    let max_res = *residuals.last().unwrap();
    stats.h_residual_median_px = Some(median_res);
    stats.h_residual_max_px = Some(max_res);

    let median_thresh = params.max_median_residual_rel * cell_size;
    let max_thresh = params.max_residual_rel * cell_size;
    if median_res > median_thresh || max_res > max_thresh {
        return stats;
    }
    stats.h_trusted = true;

    // KD-tree of un-labelled, eligible positions.
    let mut tree: KdTree<f32, 2> = KdTree::new();
    let mut tree_slot_to_corner: Vec<usize> = Vec::new();
    for (idx, pos) in positions.iter().enumerate() {
        if !grow.by_corner.contains_key(&idx) && validator.is_eligible(idx) {
            tree.add(&[pos.x, pos.y], tree_slot_to_corner.len() as u64);
            tree_slot_to_corner.push(idx);
        }
    }

    let search_r = params.search_rel * cell_size;
    let r2 = search_r * search_r;

    for iter in 0..params.max_iters {
        let cells = enumerate_extension_cells(&grow.labelled);
        let mut attached_this_iter = 0u32;

        for cell in cells {
            if grow.labelled.contains_key(&cell) {
                continue;
            }

            // Parity gate (same as BFS `collect_candidates`): if the
            // validator demands a label at this cell, candidates whose
            // own label disagrees are filtered out.
            let required_label = validator.required_label_at(cell.0, cell.1);

            let pred = h.apply(Point2::new(cell.0 as f32, cell.1 as f32));
            let mut hits: Vec<(usize, f32)> = Vec::new();
            let mut rejected_label_count = 0u32;
            for nn in tree
                .within_unsorted::<SquaredEuclidean>(&[pred.x, pred.y], r2)
                .into_iter()
            {
                let idx = tree_slot_to_corner[nn.item as usize];
                // Single-claim guarantee — must check `by_corner` every
                // candidate, since attachments inside this loop have
                // already updated it.
                if grow.by_corner.contains_key(&idx) {
                    continue;
                }
                if let Some(req) = required_label {
                    let Some(got) = validator.label_of(idx) else {
                        rejected_label_count += 1;
                        continue;
                    };
                    if got != req {
                        rejected_label_count += 1;
                        continue;
                    }
                }
                hits.push((idx, nn.distance.sqrt()));
            }
            stats.rejected_label += rejected_label_count;
            // Tie-break on corner index to make the chosen `hits[0]`
            // deterministic when two candidates sit equidistant from
            // the prediction.
            hits.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)));

            if hits.is_empty() {
                stats.rejected_no_candidate += 1;
                continue;
            }
            if hits.len() >= 2 {
                let d0 = hits[0].1.max(f32::EPSILON);
                let d1 = hits[1].1;
                if d1 / d0 < params.ambiguity_factor {
                    stats.rejected_ambiguous += 1;
                    continue;
                }
            }

            let candidate_idx = hits[0].0;
            let neighbours = collect_labelled_neighbours(cell, &grow.labelled, positions);
            // Validator gate: axis-cluster / parity check (chessboard's
            // accept_candidate verifies axes match the global cluster).
            if matches!(
                validator.accept_candidate(candidate_idx, cell, pred, &neighbours),
                Admit::Reject
            ) {
                stats.rejected_validator += 1;
                continue;
            }

            // Soft per-edge invariant gate: at least one cardinal
            // neighbour must accept the induced edge (step length +
            // axis-slot swap on chessboard). Without this, Stage 6 can
            // attach a corner whose axes match the cluster but whose
            // edge to its labelled neighbour fails the parity invariant.
            if !any_cardinal_edge_ok(candidate_idx, cell, &grow.labelled, validator) {
                stats.rejected_edge += 1;
                continue;
            }

            // Single-claim attachment: update `labelled` and
            // `by_corner` immediately so subsequent cells in this pass
            // cannot pick the same corner index.
            grow.labelled.insert(cell, candidate_idx);
            grow.by_corner.insert(candidate_idx, cell);
            grow.holes.remove(&cell);
            grow.ambiguous.remove(&cell);
            stats.attached += 1;
            stats.attached_indices.push(candidate_idx);
            stats.attached_cells.push(cell);
            attached_this_iter += 1;
        }

        stats.iterations = iter + 1;
        if attached_this_iter == 0 {
            return stats;
        }
    }
    stats
}

/// Cells worth trying: every position in the bbox not yet labelled, plus
/// one step beyond the bbox in each direction (with a labelled member
/// in the matching row / column).
fn enumerate_extension_cells(labelled: &HashMap<(i32, i32), usize>) -> Vec<(i32, i32)> {
    if labelled.is_empty() {
        return Vec::new();
    }
    let (mut min_i, mut max_i, mut min_j, mut max_j) = (i32::MAX, i32::MIN, i32::MAX, i32::MIN);
    let mut rows: HashSet<i32> = HashSet::new();
    let mut cols: HashSet<i32> = HashSet::new();
    for &(i, j) in labelled.keys() {
        min_i = min_i.min(i);
        max_i = max_i.max(i);
        min_j = min_j.min(j);
        max_j = max_j.max(j);
        cols.insert(i);
        rows.insert(j);
    }

    let mut out: HashSet<(i32, i32)> = HashSet::new();
    for j in min_j..=max_j {
        for i in min_i..=max_i {
            if !labelled.contains_key(&(i, j)) {
                out.insert((i, j));
            }
        }
    }
    for &j in &rows {
        out.insert((min_i - 1, j));
        out.insert((max_i + 1, j));
    }
    for &i in &cols {
        out.insert((i, min_j - 1));
        out.insert((i, max_j + 1));
    }
    // Sort for deterministic processing order — HashSet iteration is
    // unspecified, and attachment outcomes depend on order.
    let mut v: Vec<(i32, i32)> = out.into_iter().collect();
    v.sort_unstable();
    v
}

fn collect_labelled_neighbours(
    pos: (i32, i32),
    labelled: &HashMap<(i32, i32), usize>,
    positions: &[Point2<f32>],
) -> Vec<LabelledNeighbour> {
    let mut out = Vec::new();
    for dj in -1..=1_i32 {
        for di in -1..=1_i32 {
            if di == 0 && dj == 0 {
                continue;
            }
            let at = (pos.0 + di, pos.1 + dj);
            if let Some(&idx) = labelled.get(&at) {
                out.push(LabelledNeighbour {
                    idx,
                    at,
                    position: positions[idx],
                });
            }
        }
    }
    out
}

/// Mirrors the post-attach check in `bfs_grow::any_cardinal_edge_ok`.
/// When `pos` has at least one cardinal labelled neighbour, the attached
/// corner must satisfy `edge_ok` for at least one of them. When `pos`
/// has none (no cardinal labelled neighbour at all — shouldn't happen
/// for Stage 6 since `enumerate_extension_cells` only emits cells
/// adjacent to the labelled set), the test passes trivially.
fn any_cardinal_edge_ok<V: GrowValidator>(
    c_idx: usize,
    pos: (i32, i32),
    labelled: &HashMap<(i32, i32), usize>,
    validator: &V,
) -> bool {
    let mut found_any = false;
    for (di, dj) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let neigh = (pos.0 + di, pos.1 + dj);
        if let Some(&n_idx) = labelled.get(&neigh) {
            found_any = true;
            if validator.edge_ok(c_idx, n_idx, pos, neigh) {
                return true;
            }
        }
    }
    !found_any
}

// --- local-H extension ----------------------------------------------------

/// Tuning knobs for [`extend_via_local_homography`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct LocalExtensionParams {
    /// Number of nearest labelled corners (by grid Manhattan distance)
    /// used to fit each candidate cell's local H.
    pub k_nearest: usize,
    /// Minimum supports below which a candidate cell is skipped (the
    /// local H would be under-determined or noise-dominated). Must be
    /// `≥ 4` for DLT to be solvable.
    pub min_k: usize,
    /// Maximum allowed worst-case reprojection residual on the K
    /// supports, expressed as a fraction of `cell_size`. Per-candidate
    /// trust gate — a poor local fit aborts that candidate, not the
    /// whole pass. Default 0.30 matches the global-H worst-case
    /// threshold.
    pub max_residual_rel: f32,
    /// Search radius around each `H · (cell)` prediction, fraction of
    /// `cell_size`. Default 0.40 matches global Stage 6.
    pub search_rel: f32,
    /// Ambiguity gate: when the second-nearest candidate is within
    /// `factor × nearest`, the attachment is skipped. Tighter than
    /// BFS's 1.5 because boundary errors are unrecoverable.
    pub ambiguity_factor: f32,
    /// Per-pass cap on iterations. Each iter expands the labelled
    /// bbox by at most one cell in each direction, so `max_iters = 8`
    /// reaches up to 8 cells past the original bbox.
    pub max_iters: u32,
    /// Cell distance past the current bbox to enumerate per iter.
    /// `1` is the original behaviour (extend by one cell, iterate).
    /// Larger values let one iter reach further when the immediate
    /// neighbour cells are empty (no corner there) but cells further
    /// out have corners — common on heavy perspective with foreshortened
    /// boundary cells the seed didn't reach. Default `3` (matches the
    /// observed strip widths on the small3 / oblique-puzzleboard
    /// failure cases).
    pub extend_depth: u32,
}

impl Default for LocalExtensionParams {
    fn default() -> Self {
        Self {
            k_nearest: 12,
            min_k: 6,
            max_residual_rel: 0.30,
            search_rel: 0.40,
            ambiguity_factor: 2.5,
            max_iters: 8,
            extend_depth: 3,
        }
    }
}

/// Extend the labelled grid outward (and into interior holes) using a
/// **per-candidate local homography** fit from the K nearest labelled
/// corners (by grid Manhattan distance).
///
/// Each candidate cell gets its own H, fit from the labels closest to
/// it in `(i, j)`-space. The per-candidate trust gate is the worst-
/// case residual on the K supports relative to `cell_size`; a poor
/// local fit aborts that candidate alone, not the whole pass. This
/// is the right strategy when the global homography assumption breaks
/// — heavy radial distortion, multi-region perspective, or labelled
/// sets that span the camera's full distortion range.
///
/// Same per-corner gates as [`extend_via_global_homography`]: parity
/// (`required_label_at` × `label_of`), ambiguity, `accept_candidate`
/// (axis-cluster), `edge_ok` (per-edge invariant), single-claim.
///
/// `ExtensionStats::h_residual_median_px` and `h_residual_max_px`
/// aggregate residuals across **all** per-candidate fits in this pass
/// (median / worst across all supports). `h_trusted` is `true` if at
/// least one candidate's local fit passed its trust gate.
pub fn extend_via_local_homography<V: GrowValidator>(
    positions: &[Point2<f32>],
    grow: &mut GrowResult,
    cell_size: f32,
    params: &LocalExtensionParams,
    validator: &V,
) -> ExtensionStats {
    let mut stats = ExtensionStats::default();

    if grow.labelled.len() < params.min_k {
        return stats;
    }

    // KD-tree of un-labelled, eligible corners. Same structure as
    // global Stage 6 — built once, queried per candidate.
    let mut tree: KdTree<f32, 2> = KdTree::new();
    let mut tree_slot_to_corner: Vec<usize> = Vec::new();
    for (idx, pos) in positions.iter().enumerate() {
        if !grow.by_corner.contains_key(&idx) && validator.is_eligible(idx) {
            tree.add(&[pos.x, pos.y], tree_slot_to_corner.len() as u64);
            tree_slot_to_corner.push(idx);
        }
    }

    let search_r = params.search_rel * cell_size;
    let r2 = search_r * search_r;
    let max_residual_px = params.max_residual_rel * cell_size;

    let mut all_residuals: Vec<f32> = Vec::new();

    for iter in 0..params.max_iters {
        let cells =
            enumerate_extension_cells_deep(&grow.labelled, params.extend_depth.max(1) as i32);
        let mut attached_this_iter = 0u32;

        for cell in cells {
            if grow.labelled.contains_key(&cell) {
                continue;
            }

            // K nearest labelled corners by grid Manhattan distance.
            let nearest = nearest_labelled_by_grid(&grow.labelled, cell, params.k_nearest);
            if nearest.len() < params.min_k {
                stats.rejected_no_candidate += 1;
                continue;
            }

            // Fit local H from these labels.
            let grid_pts: Vec<Point2<f32>> = nearest
                .iter()
                .map(|&(i, j, _)| Point2::new(i as f32, j as f32))
                .collect();
            let img_pts: Vec<Point2<f32>> =
                nearest.iter().map(|&(_, _, idx)| positions[idx]).collect();
            let Some((h, _)) = estimate_homography_with_quality(&grid_pts, &img_pts) else {
                continue;
            };

            // Per-candidate trust gate: worst residual on the K supports.
            let mut max_resid: f32 = 0.0;
            for k in 0..grid_pts.len() {
                let pred = h.apply(grid_pts[k]);
                let dx = pred.x - img_pts[k].x;
                let dy = pred.y - img_pts[k].y;
                let r = (dx * dx + dy * dy).sqrt();
                if r > max_resid {
                    max_resid = r;
                }
                all_residuals.push(r);
            }
            if max_resid > max_residual_px {
                continue;
            }

            // Predict the candidate cell.
            let pred = h.apply(Point2::new(cell.0 as f32, cell.1 as f32));

            // Parity gate.
            let required_label = validator.required_label_at(cell.0, cell.1);
            let mut hits: Vec<(usize, f32)> = Vec::new();
            let mut rejected_label_count = 0u32;
            for nn in tree
                .within_unsorted::<SquaredEuclidean>(&[pred.x, pred.y], r2)
                .into_iter()
            {
                let idx = tree_slot_to_corner[nn.item as usize];
                if grow.by_corner.contains_key(&idx) {
                    continue;
                }
                if let Some(req) = required_label {
                    let Some(got) = validator.label_of(idx) else {
                        rejected_label_count += 1;
                        continue;
                    };
                    if got != req {
                        rejected_label_count += 1;
                        continue;
                    }
                }
                hits.push((idx, nn.distance.sqrt()));
            }
            stats.rejected_label += rejected_label_count;
            // Tie-break on corner index to make the chosen `hits[0]`
            // deterministic when two candidates sit equidistant from
            // the prediction.
            hits.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)));

            if hits.is_empty() {
                stats.rejected_no_candidate += 1;
                continue;
            }
            if hits.len() >= 2 {
                let d0 = hits[0].1.max(f32::EPSILON);
                let d1 = hits[1].1;
                if d1 / d0 < params.ambiguity_factor {
                    stats.rejected_ambiguous += 1;
                    continue;
                }
            }

            let candidate_idx = hits[0].0;
            let neighbours = collect_labelled_neighbours(cell, &grow.labelled, positions);
            if matches!(
                validator.accept_candidate(candidate_idx, cell, pred, &neighbours),
                Admit::Reject
            ) {
                stats.rejected_validator += 1;
                continue;
            }

            if !any_cardinal_edge_ok(candidate_idx, cell, &grow.labelled, validator) {
                stats.rejected_edge += 1;
                continue;
            }

            // Single-claim attachment.
            grow.labelled.insert(cell, candidate_idx);
            grow.by_corner.insert(candidate_idx, cell);
            grow.holes.remove(&cell);
            grow.ambiguous.remove(&cell);
            stats.attached += 1;
            stats.attached_indices.push(candidate_idx);
            stats.attached_cells.push(cell);
            attached_this_iter += 1;
        }

        stats.iterations = iter + 1;
        if attached_this_iter == 0 {
            break;
        }
    }

    if !all_residuals.is_empty() {
        all_residuals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        stats.h_residual_median_px = Some(all_residuals[all_residuals.len() / 2]);
        stats.h_residual_max_px = Some(*all_residuals.last().unwrap());
        stats.h_trusted = stats.attached > 0;
    }

    stats
}

/// Cells worth trying for the deeper local-H pass: every interior hole,
/// plus all cells within `depth` Manhattan distance past the labelled
/// bbox edge. The local-H per-cell trust gate is responsible for
/// rejecting cells whose K-nearest support gives a poor fit; the
/// search-radius gate (with the ambiguity factor) is responsible for
/// rejecting cells whose actual position has no corner. Together they
/// keep the wider enumeration safe.
fn enumerate_extension_cells_deep(
    labelled: &HashMap<(i32, i32), usize>,
    depth: i32,
) -> Vec<(i32, i32)> {
    if labelled.is_empty() || depth < 1 {
        return Vec::new();
    }
    let (mut min_i, mut max_i, mut min_j, mut max_j) = (i32::MAX, i32::MIN, i32::MAX, i32::MIN);
    let mut rows: HashSet<i32> = HashSet::new();
    let mut cols: HashSet<i32> = HashSet::new();
    for &(i, j) in labelled.keys() {
        min_i = min_i.min(i);
        max_i = max_i.max(i);
        min_j = min_j.min(j);
        max_j = max_j.max(j);
        cols.insert(i);
        rows.insert(j);
    }

    let mut out: HashSet<(i32, i32)> = HashSet::new();
    // Interior holes (cells in bbox not labelled).
    for j in min_j..=max_j {
        for i in min_i..=max_i {
            if !labelled.contains_key(&(i, j)) {
                out.insert((i, j));
            }
        }
    }
    // Side strips of width `depth`. Only emit cells aligned with
    // labelled rows / columns to keep the cell count bounded; cells
    // that diverge from the labelled set both in i and j (corners of
    // the extended bbox) are added too, so attached corners can seed
    // diagonal growth.
    for d in 1..=depth {
        for &j in &rows {
            out.insert((min_i - d, j));
            out.insert((max_i + d, j));
        }
        for &i in &cols {
            out.insert((i, min_j - d));
            out.insert((i, max_j + d));
        }
        for d2 in 1..=depth {
            out.insert((min_i - d, min_j - d2));
            out.insert((min_i - d, max_j + d2));
            out.insert((max_i + d, min_j - d2));
            out.insert((max_i + d, max_j + d2));
        }
    }
    // Sort for deterministic processing order — HashSet iteration is
    // unspecified, and Stage 6 attachments depend on order (earlier
    // attachments affect later predictions).
    let mut v: Vec<(i32, i32)> = out.into_iter().collect();
    v.sort_unstable();
    v
}

/// Find the K labelled corners closest to `target` by Manhattan
/// distance in `(i, j)`-space. Ties are broken deterministically by
/// `(i, j, idx)` to make local-H Stage 6 reproducible: HashMap
/// iteration order is unspecified, so without an explicit tie-breaker
/// the K-nearest set varies across runs and Stage 6 attachments
/// become non-deterministic. Returns `(i, j, idx)` triples.
fn nearest_labelled_by_grid(
    labelled: &HashMap<(i32, i32), usize>,
    target: (i32, i32),
    k: usize,
) -> Vec<(i32, i32, usize)> {
    let mut sorted: Vec<((i32, i32), usize, i32)> = labelled
        .iter()
        .map(|(&(i, j), &idx)| {
            let d = (i - target.0).abs() + (j - target.1).abs();
            ((i, j), idx, d)
        })
        .collect();
    sorted.sort_by(|a, b| {
        // Primary key: Manhattan distance to target. Tie-break on
        // grid coordinate, then on corner index — both deterministic.
        a.2.cmp(&b.2)
            .then_with(|| a.0 .0.cmp(&b.0 .0))
            .then_with(|| a.0 .1.cmp(&b.0 .1))
            .then_with(|| a.1.cmp(&b.1))
    });
    sorted
        .into_iter()
        .take(k)
        .map(|((i, j), idx, _)| (i, j, idx))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trivial validator: every corner eligible, no parity, accept every candidate.
    struct OpenValidator;

    impl GrowValidator for OpenValidator {
        fn is_eligible(&self, _idx: usize) -> bool {
            true
        }
        fn required_label_at(&self, _i: i32, _j: i32) -> Option<u8> {
            None
        }
        fn label_of(&self, _idx: usize) -> Option<u8> {
            None
        }
        fn accept_candidate(
            &self,
            _idx: usize,
            _at: (i32, i32),
            _prediction: Point2<f32>,
            _neighbours: &[LabelledNeighbour],
        ) -> Admit {
            Admit::Accept
        }
    }

    /// Parity-aware validator: enforces a (i+j) % 2 == 0 → label 0,
    /// otherwise label 1 contract. Used to test parity rejection.
    struct ParityValidator {
        /// Per-corner labels supplied by the test fixture.
        labels: Vec<u8>,
    }

    impl GrowValidator for ParityValidator {
        fn is_eligible(&self, _idx: usize) -> bool {
            true
        }
        fn required_label_at(&self, i: i32, j: i32) -> Option<u8> {
            Some(((i + j).rem_euclid(2)) as u8)
        }
        fn label_of(&self, idx: usize) -> Option<u8> {
            self.labels.get(idx).copied()
        }
        fn accept_candidate(
            &self,
            _idx: usize,
            _at: (i32, i32),
            _prediction: Point2<f32>,
            _neighbours: &[LabelledNeighbour],
        ) -> Admit {
            Admit::Accept
        }
    }

    /// Edge-aware validator: every edge involving `forbid_idx` is bad.
    /// `any_cardinal_edge_ok` therefore returns `false` whenever
    /// `forbid_idx` is the candidate, regardless of which labelled
    /// neighbour we look at.
    struct EdgeRejectingValidator {
        forbid_idx: usize,
    }

    impl GrowValidator for EdgeRejectingValidator {
        fn is_eligible(&self, _idx: usize) -> bool {
            true
        }
        fn required_label_at(&self, _i: i32, _j: i32) -> Option<u8> {
            None
        }
        fn label_of(&self, _idx: usize) -> Option<u8> {
            None
        }
        fn accept_candidate(
            &self,
            _idx: usize,
            _at: (i32, i32),
            _prediction: Point2<f32>,
            _neighbours: &[LabelledNeighbour],
        ) -> Admit {
            Admit::Accept
        }
        fn edge_ok(
            &self,
            candidate_idx: usize,
            neighbour_idx: usize,
            _at_candidate: (i32, i32),
            _at_neighbour: (i32, i32),
        ) -> bool {
            candidate_idx != self.forbid_idx && neighbour_idx != self.forbid_idx
        }
    }

    fn synthetic_grid(rows: i32, cols: i32, scale: f32) -> Vec<Point2<f32>> {
        let mut pts = Vec::with_capacity((rows * cols) as usize);
        for j in 0..rows {
            for i in 0..cols {
                pts.push(Point2::new(
                    i as f32 * scale + 100.0,
                    j as f32 * scale + 50.0,
                ));
            }
        }
        pts
    }

    fn label_subgrid(
        positions: &[Point2<f32>],
        cols: i32,
        i_range: std::ops::Range<i32>,
        j_range: std::ops::Range<i32>,
    ) -> GrowResult {
        let mut labelled = HashMap::new();
        let mut by_corner = HashMap::new();
        for j in j_range {
            for i in i_range.clone() {
                let idx = (j * cols + i) as usize;
                labelled.insert((i, j), idx);
                by_corner.insert(idx, (i, j));
            }
        }
        let _ = positions;
        GrowResult {
            labelled,
            by_corner,
            ..Default::default()
        }
    }

    #[test]
    fn extends_clean_perspective_grid() {
        let cols = 6_i32;
        let rows = 4_i32;
        let scale = 50.0_f32;
        let positions = synthetic_grid(rows, cols, scale);
        let mut grow = label_subgrid(&positions, cols, 1..5, 1..3);
        let starting_count = grow.labelled.len();
        assert_eq!(starting_count, 8);

        let stats = extend_via_global_homography(
            &positions,
            &mut grow,
            scale,
            &ExtensionParams {
                min_labels_for_h: 4,
                ..Default::default()
            },
            &OpenValidator,
        );

        assert!(stats.h_trusted, "H must be trusted on a clean affine grid");
        assert!(
            grow.labelled.len() > starting_count,
            "extension should add corners on a clean grid"
        );
    }

    #[test]
    fn refuses_to_extend_when_residuals_too_high() {
        let cols = 4_i32;
        let rows = 4_i32;
        let scale = 50.0_f32;
        let mut positions = synthetic_grid(rows, cols, scale);
        positions[(cols + 1) as usize].x += scale * 0.5;
        let mut grow = label_subgrid(&positions, cols, 0..4, 0..4);

        let stats = extend_via_global_homography(
            &positions,
            &mut grow,
            scale,
            &ExtensionParams {
                min_labels_for_h: 4,
                max_residual_rel: 0.30,
                ..Default::default()
            },
            &OpenValidator,
        );
        assert!(!stats.h_trusted);
        assert_eq!(stats.attached, 0);
    }

    #[test]
    fn no_op_when_too_few_labels() {
        let cols = 4_i32;
        let rows = 4_i32;
        let positions = synthetic_grid(rows, cols, 50.0);
        let mut grow = label_subgrid(&positions, cols, 0..2, 0..2);
        let stats = extend_via_global_homography(
            &positions,
            &mut grow,
            50.0,
            &ExtensionParams::default(),
            &OpenValidator,
        );
        assert_eq!(stats.attached, 0);
        assert!(stats.h_quality.is_none());
    }

    #[test]
    fn rejects_wrong_parity_corner_at_h_prediction() {
        // 4x4 grid. Label the central 4 cells with chessboard parity.
        // Place a parity-WRONG corner exactly at H · (-1, 1) — Stage 6
        // must reject it because `label_of` disagrees with
        // `required_label_at`.
        let cols = 4_i32;
        let rows = 4_i32;
        let scale = 50.0_f32;
        let positions = synthetic_grid(rows, cols, scale);
        // Corner at (-1, 1) exists in the grid at position
        // positions[1 * cols + (-1) ... wait, (-1) is out of bounds.
        // Build a fresh fixture.
        // Instead: 4x4 fixture, label the inner 2x2 (cells (1,1)..(2,2)).
        let mut grow = label_subgrid(&positions, cols, 1..3, 1..3);
        // Cell (0, 1) (parity 0+1=1, label 1) sits at positions[1*cols+0]=4.
        // Set its label to the wrong value (0). Cluster labels for the
        // rest must match parity.
        let labels: Vec<u8> = (0..(rows * cols))
            .map(|k| {
                let i = k % cols;
                let j = k / cols;
                ((i + j).rem_euclid(2)) as u8
            })
            .collect();
        // Corrupt the parity at (0, 1) so it has label 0 instead of 1.
        let bad_idx = cols as usize;
        let mut labels = labels;
        labels[bad_idx] = 0;
        let validator = ParityValidator { labels };

        let stats = extend_via_global_homography(
            &positions,
            &mut grow,
            scale,
            &ExtensionParams {
                min_labels_for_h: 4,
                ..Default::default()
            },
            &validator,
        );
        assert!(stats.h_trusted);
        // The corner at (0, 1) is rejected by the parity gate; it
        // should NOT appear in the extended labels.
        assert!(!grow.labelled.contains_key(&(0, 1)) || grow.labelled[&(0, 1)] != bad_idx);
        // Expect at least one parity rejection counter increment for
        // the (0, 1) cell trial.
        assert!(stats.rejected_label >= 1);
    }

    #[test]
    fn rejects_bad_edge_via_edge_ok_gate() {
        // Build a clean 4x4 grid and label the central 2x2. Pick a corner
        // adjacent to the labelled set — say (0, 1) — and forbid the
        // edge between it and its (1, 1) cardinal neighbour. Stage 6
        // must NOT attach (0, 1) because no cardinal edge satisfies
        // edge_ok.
        let cols = 4_i32;
        let rows = 4_i32;
        let scale = 50.0_f32;
        let positions = synthetic_grid(rows, cols, scale);
        let mut grow = label_subgrid(&positions, cols, 1..3, 1..3);

        let bad_candidate = cols as usize; // (0, 1)
        let validator = EdgeRejectingValidator {
            forbid_idx: bad_candidate,
        };

        let stats = extend_via_global_homography(
            &positions,
            &mut grow,
            scale,
            &ExtensionParams {
                min_labels_for_h: 4,
                ..Default::default()
            },
            &validator,
        );
        assert!(stats.h_trusted);
        assert!(stats.rejected_edge >= 1);
        assert!(!grow.labelled.contains_key(&(0, 1)));
    }

    #[test]
    fn single_claim_prevents_double_attach() {
        // Two cells whose H predictions both land on the same physical
        // corner — only one attachment must happen. Construct a fixture
        // where the candidate KD-tree contains just ONE eligible corner
        // sitting inside the search radius of two predicted cells.
        //
        // Layout: labelled corners at (0, 0), (1, 0), (2, 0), (0, 1),
        // (1, 1), (2, 1), (0, 2), (1, 2), (2, 2) — a 3x3 grid, all 9
        // labelled. Plus one extra un-labelled corner at the predicted
        // position of (3, 1), but ALSO within search radius of (2, -1).
        // This is awkward to engineer cleanly; an easier construction:
        // put a tiny grid of labels (3x3) plus a single un-labelled
        // corner at the predicted (3, 1) location. Two cells beyond the
        // bbox — (3, 1) and (3, 2) — both query within search radius of
        // that lone corner. Only one cell wins.
        //
        // Make sure cell_size is small enough that (3, 1) and (3, 2) are
        // both within search_rel × cell_size of the SAME un-labelled
        // corner. We control this by placing the corner at the midpoint.
        let scale = 50.0_f32;
        let mut positions = Vec::new();
        // 3x3 labelled grid:
        for j in 0..3_i32 {
            for i in 0..3_i32 {
                positions.push(Point2::new(
                    i as f32 * scale + 100.0,
                    j as f32 * scale + 50.0,
                ));
            }
        }
        // One extra un-labelled corner placed at H predicted (3, 1.5):
        // (3 * scale + 100, 1.5 * scale + 50) = (250, 125).
        positions.push(Point2::new(250.0, 125.0));

        let mut labelled = HashMap::new();
        let mut by_corner = HashMap::new();
        for j in 0..3_i32 {
            for i in 0..3_i32 {
                let idx = (j * 3 + i) as usize;
                labelled.insert((i, j), idx);
                by_corner.insert(idx, (i, j));
            }
        }
        let mut grow = GrowResult {
            labelled,
            by_corner,
            ..Default::default()
        };

        let stats = extend_via_global_homography(
            &positions,
            &mut grow,
            scale,
            &ExtensionParams {
                min_labels_for_h: 4,
                search_rel: 1.5, // wide enough for (3, 1) and (3, 2) to both reach (250, 125)
                ambiguity_factor: 1.01, // allow attachment even with one neighbour
                ..Default::default()
            },
            &OpenValidator,
        );
        assert!(stats.h_trusted);
        // The lone extra corner has index 9 (last in `positions`). It
        // can be attached to AT MOST one of the candidate cells.
        let attached_for_idx_9: Vec<&(i32, i32)> = grow
            .labelled
            .iter()
            .filter_map(|(k, &v)| if v == 9 { Some(k) } else { None })
            .collect();
        assert!(
            attached_for_idx_9.len() <= 1,
            "corner index 9 attached to {} cells: {:?}",
            attached_for_idx_9.len(),
            attached_for_idx_9
        );
        // by_corner is consistent with labelled (the invariant the bug used to violate).
        for (&cell, &idx) in &grow.labelled {
            assert_eq!(grow.by_corner.get(&idx), Some(&cell));
        }
    }

    // --- local-H Stage 6 tests ---------------------------------------------

    #[test]
    fn local_h_extends_clean_perspective_grid() {
        // Same fixture as the global-H test: a clean affine grid with
        // a 4×2 labelled subset; local-H should also extend it.
        let cols = 6_i32;
        let rows = 4_i32;
        let scale = 50.0_f32;
        let positions = synthetic_grid(rows, cols, scale);
        let mut grow = label_subgrid(&positions, cols, 1..5, 1..3);
        let starting_count = grow.labelled.len();
        assert_eq!(starting_count, 8);

        let stats = extend_via_local_homography(
            &positions,
            &mut grow,
            scale,
            &LocalExtensionParams {
                min_k: 4,
                k_nearest: 8,
                ..Default::default()
            },
            &OpenValidator,
        );

        assert!(stats.h_trusted);
        assert!(
            grow.labelled.len() > starting_count,
            "local-H extension should add corners on a clean grid"
        );
    }

    #[test]
    fn local_h_reaches_further_than_global() {
        // Build a perspective-foreshortened grid where actual corners exist
        // 2 cells past the labelled bbox. Global-H Stage 6 only enumerates
        // bbox+1 in iter 0; if predictions there miss, it stops. Local-H
        // iterates across bbox+1 → bbox+2 → … as labels grow, so it
        // reaches further.
        //
        // Layout: 8x4 affine grid, label inner 6x4 (cols 1..7). Local-H
        // should attach cols 0 and 7 (and beyond, but only 8 columns
        // exist here).
        let cols = 8_i32;
        let rows = 4_i32;
        let scale = 50.0_f32;
        let positions = synthetic_grid(rows, cols, scale);
        let mut grow_local = label_subgrid(&positions, cols, 2..6, 0..rows);
        assert_eq!(grow_local.labelled.len(), 16);

        let stats = extend_via_local_homography(
            &positions,
            &mut grow_local,
            scale,
            &LocalExtensionParams {
                min_k: 4,
                ..Default::default()
            },
            &OpenValidator,
        );

        // Should reach all 8 columns (32 corners) over multiple iterations.
        assert!(stats.iterations >= 2, "expected ≥ 2 iters");
        assert_eq!(
            grow_local.labelled.len(),
            (rows * cols) as usize,
            "local-H should reach every cell on a clean grid: {} of {}",
            grow_local.labelled.len(),
            rows * cols,
        );
    }

    #[test]
    fn local_h_no_op_when_too_few_labels() {
        let cols = 4_i32;
        let rows = 4_i32;
        let positions = synthetic_grid(rows, cols, 50.0);
        let mut grow = label_subgrid(&positions, cols, 0..2, 0..2);
        let stats = extend_via_local_homography(
            &positions,
            &mut grow,
            50.0,
            &LocalExtensionParams {
                min_k: 8,
                ..Default::default()
            },
            &OpenValidator,
        );
        // 4 labels < min_k = 8 → no-op.
        assert_eq!(stats.attached, 0);
        assert!(!stats.h_trusted);
    }

    #[test]
    fn local_h_rejects_wrong_parity() {
        // Same fixture as the global-H parity test: place a parity-WRONG
        // corner at the predicted (0, 1); local-H must reject it via
        // `required_label_at` × `label_of`.
        let cols = 4_i32;
        let rows = 4_i32;
        let scale = 50.0_f32;
        let positions = synthetic_grid(rows, cols, scale);
        let mut grow = label_subgrid(&positions, cols, 1..3, 1..3);
        let labels: Vec<u8> = (0..(rows * cols))
            .map(|k| {
                let i = k % cols;
                let j = k / cols;
                ((i + j).rem_euclid(2)) as u8
            })
            .collect();
        let bad_idx = cols as usize;
        let mut labels = labels;
        labels[bad_idx] = 0; // corrupt parity at (0, 1).
        let validator = ParityValidator { labels };

        let stats = extend_via_local_homography(
            &positions,
            &mut grow,
            scale,
            &LocalExtensionParams {
                min_k: 4,
                ..Default::default()
            },
            &validator,
        );
        assert!(!grow.labelled.contains_key(&(0, 1)) || grow.labelled[&(0, 1)] != bad_idx);
        assert!(stats.rejected_label >= 1);
    }
}
