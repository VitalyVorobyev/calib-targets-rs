//! BFS extension from an existing labelled grid.
//!
//! [`extend_from_labelled`] takes an existing [`GrowResult`] (already grown
//! from a seed) and walks its boundary with the same candidate-selection
//! pipeline as [`crate::square::grow::bfs_grow`], attaching newly-eligible
//! corners without disturbing the corners that are already labelled.
//!
//! This is a **non-destructive** extension: it never demotes or moves
//! existing labelled entries, so it is safe to call after a refit that
//! refined corner positions.

use crate::square::grow::{
    any_cardinal_edge_ok, choose_unambiguous, collect_candidates, collect_labelled_neighbours,
    enqueue_cardinal_neighbours, is_extrapolating, predict_from_neighbours, CandidateChoice,
    GrowParams, GrowResult, GrowValidator,
};
use kiddo::KdTree;
use nalgebra::Point2;
use std::collections::{HashSet, VecDeque};

/// Counters returned by [`extend_from_labelled`].
///
/// Mirrors the fields of
/// [`crate::square::grow_extension::ExtensionStats`], but covers the
/// simpler cardinal-BFS path (no homography, no local-H, just
/// `process_boundary_cell`).
#[non_exhaustive]
#[derive(Clone, Debug, Default)]
pub struct BfsExtensionStats {
    /// Number of corners successfully attached.
    pub attached: usize,
    /// Cells with no eligible candidate in the search radius.
    pub rejected_no_candidate: usize,
    /// Cells with multiple near-equidistant candidates.
    pub rejected_ambiguous: usize,
    /// Cells where the unique candidate failed the edge check.
    pub rejected_edge: usize,
    /// Corner indices of all attached corners (parallel to
    /// [`attached_cells`][BfsExtensionStats::attached_cells]).
    pub attached_indices: Vec<usize>,
    /// Grid cells at which each attached corner was placed (parallel to
    /// [`attached_indices`][BfsExtensionStats::attached_indices]).
    pub attached_cells: Vec<(i32, i32)>,
}

/// Extend an existing [`GrowResult`] by walking its boundary with the
/// cardinal-BFS candidate pipeline.
///
/// Builds a KD-tree over corners that are currently eligible (per the
/// validator) but not yet labelled, then drives the same
/// `process_boundary_cell` logic used by
/// [`crate::square::grow::bfs_grow`]. Already-labelled corners are
/// never moved or removed.
///
/// The extension uses `grow.grid_u` and `grow.grid_v` for direction; the
/// caller must ensure those fields are meaningful (they are set by
/// `bfs_grow`).
///
/// # Returns
///
/// A [`BfsExtensionStats`] summary. The caller is responsible for any
/// per-corner state updates (e.g., marking newly-attached corners as
/// "Labeled" in a local stage enum).
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "info",
        skip_all,
        fields(num_corners = positions.len(), num_labelled = grow.labelled.len(), cell_size = cell_size),
    )
)]
pub fn extend_from_labelled<V: GrowValidator>(
    positions: &[Point2<f32>],
    grow: &mut GrowResult,
    cell_size: f32,
    params: &GrowParams,
    validator: &V,
) -> BfsExtensionStats {
    // Build KD-tree over eligible, not-yet-labelled corners.
    let mut tree: KdTree<f32, 2> = KdTree::new();
    let mut tree_slot_to_corner: Vec<usize> = Vec::new();
    for (idx, pos) in positions.iter().enumerate() {
        if validator.is_eligible(idx) && !grow.by_corner.contains_key(&idx) {
            tree.add(&[pos.x, pos.y], tree_slot_to_corner.len() as u64);
            tree_slot_to_corner.push(idx);
        }
    }

    // Seed the BFS from the boundary of the current labelled set.
    let mut boundary: VecDeque<(i32, i32)> = VecDeque::new();
    let mut seen_boundary: HashSet<(i32, i32)> = HashSet::new();
    for &pos in grow.labelled.keys() {
        enqueue_cardinal_neighbours(pos, &grow.labelled, &mut boundary, &mut seen_boundary);
    }

    let mut stats = BfsExtensionStats::default();

    while let Some(pos) = boundary.pop_front() {
        if grow.labelled.contains_key(&pos) {
            continue;
        }

        let neighbours = collect_labelled_neighbours(pos, 1, &grow.labelled, positions);
        if neighbours.is_empty() {
            stats.rejected_no_candidate += 1;
            grow.holes.insert(pos);
            continue;
        }

        let prediction = predict_from_neighbours(
            pos,
            &neighbours,
            grow.grid_u,
            grow.grid_v,
            cell_size,
            &grow.labelled,
            positions,
        );

        let search_r = params.attach_search_rel * cell_size;
        let extrapolating = is_extrapolating(pos, &neighbours);
        let local_search_r = if extrapolating {
            search_r * params.boundary_search_factor
        } else {
            search_r
        };

        let required_label = validator.required_label_at(pos.0, pos.1);
        let candidates = collect_candidates(
            &tree,
            &tree_slot_to_corner,
            prediction,
            local_search_r,
            validator,
            required_label,
            &grow.by_corner,
        );

        let choice = choose_unambiguous(
            &candidates,
            params.attach_ambiguity_factor,
            prediction,
            positions,
            validator,
            pos,
            &neighbours,
        );

        match choice {
            CandidateChoice::None => {
                stats.rejected_no_candidate += 1;
                grow.holes.insert(pos);
            }
            CandidateChoice::Ambiguous => {
                stats.rejected_ambiguous += 1;
                grow.ambiguous.insert(pos);
            }
            CandidateChoice::Unique(c_idx) => {
                if !any_cardinal_edge_ok(c_idx, pos, &grow.labelled, validator) {
                    stats.rejected_edge += 1;
                    grow.holes.insert(pos);
                } else {
                    grow.labelled.insert(pos, c_idx);
                    grow.by_corner.insert(c_idx, pos);
                    enqueue_cardinal_neighbours(
                        pos,
                        &grow.labelled,
                        &mut boundary,
                        &mut seen_boundary,
                    );
                    stats.attached += 1;
                    stats.attached_indices.push(c_idx);
                    stats.attached_cells.push(pos);
                }
            }
        }
    }

    stats
}
