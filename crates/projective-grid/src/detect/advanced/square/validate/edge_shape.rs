//! Local edge-shape validation for completed labelled square grids.
//!
//! The checks are intentionally local: they compare only cardinal
//! neighbours and complete adjacent cells in image pixel coordinates.
//! No global homography or target-specific semantics are used.

use crate::detect::advanced::square::validate::{
    EdgeShapeDiagnostic, EdgeShapeParams, LabelledEntry,
};
use nalgebra::Vector2;
use std::collections::HashMap;

pub(super) fn evaluate_edge_shape(
    by_idx: &HashMap<usize, &LabelledEntry>,
    by_grid: &HashMap<(i32, i32), usize>,
    params: EdgeShapeParams,
) -> (
    HashMap<usize, EdgeShapeDiagnostic>,
    HashMap<usize, &'static str>,
) {
    let mut diagnostics = HashMap::with_capacity(by_idx.len());
    let mut reasons = HashMap::new();
    let component_bbox_by_coord = component_bboxes(by_grid);

    for (&idx, entry) in by_idx {
        let mut diag = EdgeShapeDiagnostic {
            cardinal_degree: cardinal_degree(by_grid, entry.grid),
            is_bbox_boundary: component_bbox_by_coord
                .get(&entry.grid)
                .is_some_and(|&bbox| is_bbox_boundary(entry.grid, bbox)),
            ..EdgeShapeDiagnostic::default()
        };

        update_cell_metrics(by_idx, by_grid, entry.grid, &params, &mut diag);
        let continuation = update_continuation_metrics(by_idx, by_grid, entry, &params, &mut diag);
        let weakly_supported = diag.cardinal_degree <= 3 || diag.valid_adjacent_cell_count <= 2;

        let reason = if diag.cardinal_degree < params.min_cardinal_degree {
            Some("low-cardinal-degree")
        } else if continuation.angle_failed
            || (weakly_supported && continuation.length_ratio_failed)
        {
            Some("bad-continuation")
        } else if diag.adjacent_cell_count > 0 && diag.valid_adjacent_cell_count == 0 {
            Some("no-valid-adjacent-cell")
        } else {
            None
        };
        if let Some(reason) = reason {
            reasons.insert(idx, reason);
        }
        diagnostics.insert(idx, diag);
    }

    (diagnostics, reasons)
}

#[derive(Clone, Copy, Debug, Default)]
struct ContinuationOutcome {
    angle_failed: bool,
    length_ratio_failed: bool,
}

fn component_bboxes(
    by_grid: &HashMap<(i32, i32), usize>,
) -> HashMap<(i32, i32), (i32, i32, i32, i32)> {
    let mut out = HashMap::with_capacity(by_grid.len());
    let mut visited = std::collections::HashSet::new();
    for &start in by_grid.keys() {
        if visited.contains(&start) {
            continue;
        }
        let mut stack = vec![start];
        let mut component = Vec::new();
        let (mut min_i, mut max_i, mut min_j, mut max_j) = (start.0, start.0, start.1, start.1);
        while let Some(coord) = stack.pop() {
            if !visited.insert(coord) {
                continue;
            }
            component.push(coord);
            min_i = min_i.min(coord.0);
            max_i = max_i.max(coord.0);
            min_j = min_j.min(coord.1);
            max_j = max_j.max(coord.1);
            for neigh in cardinal_neighbours(coord) {
                if by_grid.contains_key(&neigh) && !visited.contains(&neigh) {
                    stack.push(neigh);
                }
            }
        }
        let bbox = (min_i, max_i, min_j, max_j);
        for coord in component {
            out.insert(coord, bbox);
        }
    }
    out
}

fn is_bbox_boundary(
    (i, j): (i32, i32),
    (min_i, max_i, min_j, max_j): (i32, i32, i32, i32),
) -> bool {
    i == min_i || i == max_i || j == min_j || j == max_j
}

fn cardinal_neighbours((i, j): (i32, i32)) -> [(i32, i32); 4] {
    [(i - 1, j), (i + 1, j), (i, j - 1), (i, j + 1)]
}

fn cardinal_degree(by_grid: &HashMap<(i32, i32), usize>, (i, j): (i32, i32)) -> u8 {
    cardinal_neighbours((i, j))
        .into_iter()
        .filter(|coord| by_grid.contains_key(coord))
        .count() as u8
}

fn update_continuation_metrics(
    by_idx: &HashMap<usize, &LabelledEntry>,
    by_grid: &HashMap<(i32, i32), usize>,
    entry: &LabelledEntry,
    params: &EdgeShapeParams,
    diag: &mut EdgeShapeDiagnostic,
) -> ContinuationOutcome {
    let (i, j) = entry.grid;
    let mut outcome = ContinuationOutcome::default();
    for (prev, next) in [((i - 1, j), (i + 1, j)), ((i, j - 1), (i, j + 1))] {
        let Some(prev) = by_grid.get(&prev).and_then(|idx| by_idx.get(idx)) else {
            continue;
        };
        let Some(next) = by_grid.get(&next).and_then(|idx| by_idx.get(idx)) else {
            continue;
        };
        let before = entry.pixel - prev.pixel;
        let after = next.pixel - entry.pixel;
        let Some(angle) = angle_deg(before, after) else {
            continue;
        };
        let Some(ratio) = length_ratio(before, after) else {
            continue;
        };
        update_max(&mut diag.max_continuation_angle_deg, angle);
        update_max(&mut diag.max_continuation_length_ratio, ratio);
        if angle > params.continuation_angle_tol_deg {
            outcome.angle_failed = true;
        }
        if ratio > params.continuation_length_ratio_max {
            outcome.length_ratio_failed = true;
        }
    }
    outcome
}

fn update_cell_metrics(
    by_idx: &HashMap<usize, &LabelledEntry>,
    by_grid: &HashMap<(i32, i32), usize>,
    grid: (i32, i32),
    params: &EdgeShapeParams,
    diag: &mut EdgeShapeDiagnostic,
) {
    let (i, j) = grid;
    for top_left in [(i, j), (i - 1, j), (i - 1, j - 1), (i, j - 1)] {
        let Some([tl, tr, br, bl]) = cell_points(by_idx, by_grid, top_left) else {
            continue;
        };
        diag.adjacent_cell_count = diag.adjacent_cell_count.saturating_add(1);

        let top = tr.pixel - tl.pixel;
        let bottom = br.pixel - bl.pixel;
        let left = bl.pixel - tl.pixel;
        let right = br.pixel - tr.pixel;

        let mut cell_failed = false;
        for (a, b) in [(top, bottom), (left, right)] {
            let Some(angle) = angle_deg(a, b) else {
                cell_failed = true;
                continue;
            };
            let Some(ratio) = length_ratio(a, b) else {
                cell_failed = true;
                continue;
            };
            update_max(&mut diag.max_cell_opposite_angle_deg, angle);
            update_max(&mut diag.max_cell_opposite_length_ratio, ratio);
            if angle > params.cell_opposite_angle_tol_deg
                || ratio > params.cell_opposite_length_ratio_max
            {
                cell_failed = true;
            }
        }

        if !cell_failed {
            diag.valid_adjacent_cell_count = diag.valid_adjacent_cell_count.saturating_add(1);
        }
    }
}

fn cell_points<'a>(
    by_idx: &'a HashMap<usize, &LabelledEntry>,
    by_grid: &HashMap<(i32, i32), usize>,
    (i, j): (i32, i32),
) -> Option<[&'a LabelledEntry; 4]> {
    let tl = by_grid.get(&(i, j)).and_then(|idx| by_idx.get(idx))?;
    let tr = by_grid.get(&(i + 1, j)).and_then(|idx| by_idx.get(idx))?;
    let br = by_grid
        .get(&(i + 1, j + 1))
        .and_then(|idx| by_idx.get(idx))?;
    let bl = by_grid.get(&(i, j + 1)).and_then(|idx| by_idx.get(idx))?;
    Some([*tl, *tr, *br, *bl])
}

fn angle_deg(a: Vector2<f32>, b: Vector2<f32>) -> Option<f32> {
    let an = a.norm();
    let bn = b.norm();
    if an <= f32::EPSILON || bn <= f32::EPSILON {
        return None;
    }
    let cos = (a.dot(&b) / (an * bn)).clamp(-1.0, 1.0);
    Some(cos.acos().to_degrees())
}

fn length_ratio(a: Vector2<f32>, b: Vector2<f32>) -> Option<f32> {
    let an = a.norm();
    let bn = b.norm();
    let lo = an.min(bn);
    if lo <= f32::EPSILON {
        return None;
    }
    Some(an.max(bn) / lo)
}

fn update_max(slot: &mut Option<f32>, value: f32) {
    *slot = Some(slot.map_or(value, |current| current.max(value)));
}
