//! Generic, target-agnostic output cleanup for a labelled square grid.
//!
//! These helpers operate on a bare `(i, j) → corner_idx` map plus the
//! corner position table — no chessboard / ChArUco / marker vocabulary.
//! They are the post-detection cleanup that
//! [`detect_regular_grid`](crate::detect_regular_grid) runs internally,
//! and they are exposed individually so other pipelines can reuse the
//! exact same canonicalisation.
//!
//! These helpers are *not* re-exported at the crate root — reach for
//! them at their fully-qualified path, `projective_grid::square::cleanup::*`.
//!
//! | Helper | Responsibility |
//! |---|---|
//! | [`rebase_to_origin`] | Shift labels so the bbox minimum is `(0, 0)`. |
//! | [`prune_to_main_component`] | Drop cells not 4-connected to the largest component. |
//! | [`canonicalize_top_left`] | Apply a D4 transform so `+i` → right, `+j` → down in pixel space. |
//! | [`sorted_grid_points`] | Flatten the map into a `(j, i)`-sorted vector. |
//!
//! All four are pure functions of their inputs; none consult image
//! data or pattern rules.

use std::collections::{HashMap, HashSet, VecDeque};

use nalgebra::Point2;

use crate::square::alignment::{GridTransform, GRID_TRANSFORMS_D4};

/// Rebase a labelled map so the bounding-box minimum `(i, j)` is `(0, 0)`.
///
/// This is the non-negative-label invariant every grid consumer in the
/// workspace relies on. Returns the rebased map; the input is consumed.
/// A degenerate empty map round-trips unchanged.
pub fn rebase_to_origin(labelled: HashMap<(i32, i32), usize>) -> HashMap<(i32, i32), usize> {
    if labelled.is_empty() {
        return labelled;
    }
    let (min_i, min_j) = labelled
        .keys()
        .fold((i32::MAX, i32::MAX), |(a, b), &(i, j)| (a.min(i), b.min(j)));
    if min_i == 0 && min_j == 0 {
        return labelled;
    }
    labelled
        .into_iter()
        .map(|((i, j), idx)| ((i - min_i, j - min_j), idx))
        .collect()
}

/// Drop every cell not 4-connected (via cardinal `(i, j)` steps) to the
/// largest connected component of the labelled map.
///
/// Spurious off-grid corners and bridged sub-grids both manifest as
/// extra components; keeping only the dominant one is a pattern-
/// agnostic precision guard. Ties on component size are broken by the
/// component containing the lexicographically smallest cell, so the
/// result is deterministic.
///
/// Returns the pruned map. An empty input round-trips unchanged.
pub fn prune_to_main_component(labelled: HashMap<(i32, i32), usize>) -> HashMap<(i32, i32), usize> {
    if labelled.len() < 2 {
        return labelled;
    }

    let mut unvisited: HashSet<(i32, i32)> = labelled.keys().copied().collect();
    let mut best: Vec<(i32, i32)> = Vec::new();

    while let Some(&start) = unvisited.iter().min() {
        let mut component: Vec<(i32, i32)> = Vec::new();
        let mut queue: VecDeque<(i32, i32)> = VecDeque::new();
        unvisited.remove(&start);
        queue.push_back(start);
        while let Some(cell @ (i, j)) = queue.pop_front() {
            component.push(cell);
            for next in [(i + 1, j), (i - 1, j), (i, j + 1), (i, j - 1)] {
                if unvisited.remove(&next) {
                    queue.push_back(next);
                }
            }
        }
        // Strictly-greater keeps the first (lexicographically-earliest
        // start) component on a size tie.
        if component.len() > best.len() {
            best = component;
        }
    }

    let keep: HashSet<(i32, i32)> = best.into_iter().collect();
    labelled
        .into_iter()
        .filter(|(cell, _)| keep.contains(cell))
        .collect()
}

/// Pick the D4 grid transform that orients the labelled grid to a
/// visual top-left origin: `+i` points right (`+x`) and `+j` points
/// down (`+y`) in pixel space.
///
/// The transform is chosen by examining the pixel displacement of the
/// `+i` and `+j` grid steps inferred from a least-squares fit over the
/// labelled set, then selecting the unique D4 element that maps those
/// onto the `+x` / `+y` half-planes. Returns the identity transform
/// when the grid is too small or degenerate to infer an orientation.
pub fn top_left_transform(
    labelled: &HashMap<(i32, i32), usize>,
    positions: &[Point2<f32>],
) -> GridTransform {
    // Mean pixel displacement per +i step and per +j step, from every
    // labelled cardinal pair.
    let mut di = (0.0_f64, 0.0_f64, 0_u32);
    let mut dj = (0.0_f64, 0.0_f64, 0_u32);
    for (&(i, j), &idx) in labelled {
        let p = positions[idx];
        if let Some(&n) = labelled.get(&(i + 1, j)) {
            let q = positions[n];
            di.0 += (q.x - p.x) as f64;
            di.1 += (q.y - p.y) as f64;
            di.2 += 1;
        }
        if let Some(&n) = labelled.get(&(i, j + 1)) {
            let q = positions[n];
            dj.0 += (q.x - p.x) as f64;
            dj.1 += (q.y - p.y) as f64;
            dj.2 += 1;
        }
    }
    if di.2 == 0 || dj.2 == 0 {
        return GridTransform::IDENTITY;
    }
    let u = (di.0 / di.2 as f64, di.1 / di.2 as f64);
    let v = (dj.0 / dj.2 as f64, dj.1 / dj.2 as f64);

    // Pick the D4 transform T such that T applied to the grid basis
    // makes the +i axis point in +x and the +j axis point in +y.
    // T maps grid coords; the pixel image of T's +i column is
    // `a*u + c*v`, of its +j column `b*u + d*v`. We want the +i image
    // to have the largest +x component and the +j image the largest
    // +y component, with a right-handed (non-mirrored) basis preferred.
    let mut best: Option<(f64, GridTransform)> = None;
    for t in GRID_TRANSFORMS_D4 {
        // Pixel image of the transformed +i and +j unit grid steps.
        // (i', j') = T·(i, j); we need the inverse to know which grid
        // step lands on the new +i — but D4 elements are orthogonal,
        // so T's transpose is its inverse. Equivalent: the new +i grid
        // direction is the row of T mapping to output i.
        let inv = t.inverse().unwrap_or(GridTransform::IDENTITY);
        // New +i (output) is produced by grid step `inv·(1,0)`.
        let gi = inv.apply(1, 0);
        let gj = inv.apply(0, 1);
        let new_i_px = (
            gi.i as f64 * u.0 + gi.j as f64 * v.0,
            gi.i as f64 * u.1 + gi.j as f64 * v.1,
        );
        let new_j_px = (
            gj.i as f64 * u.0 + gj.j as f64 * v.0,
            gj.i as f64 * u.1 + gj.j as f64 * v.1,
        );
        // Score: +i should point +x, +j should point +y.
        let score = new_i_px.0 + new_j_px.1;
        if best.map(|b| score > b.0).unwrap_or(true) {
            best = Some((score, t));
        }
    }
    best.map(|b| b.1).unwrap_or(GridTransform::IDENTITY)
}

/// Apply a D4 [`GridTransform`] to every label, then rebase so the
/// bounding-box minimum is `(0, 0)`.
///
/// Use [`top_left_transform`] to choose the transform; this function
/// applies it. Splitting the choice from the application lets callers
/// inspect or override the transform.
pub fn apply_transform(
    labelled: HashMap<(i32, i32), usize>,
    transform: GridTransform,
) -> HashMap<(i32, i32), usize> {
    let transformed: HashMap<(i32, i32), usize> = labelled
        .into_iter()
        .map(|((i, j), idx)| {
            let g = transform.apply(i, j);
            ((g.i, g.j), idx)
        })
        .collect();
    rebase_to_origin(transformed)
}

/// Canonicalise a labelled grid to a visual top-left origin.
///
/// Convenience composition of [`top_left_transform`] +
/// [`apply_transform`]: orients the grid so `+i` points right and
/// `+j` points down in pixel space, then rebases to `(0, 0)`. Returns
/// the canonicalised map and the transform that was applied (so the
/// caller can map any auxiliary per-cell data through the same
/// transform).
pub fn canonicalize_top_left(
    labelled: HashMap<(i32, i32), usize>,
    positions: &[Point2<f32>],
) -> (HashMap<(i32, i32), usize>, GridTransform) {
    let transform = top_left_transform(&labelled, positions);
    (apply_transform(labelled, transform), transform)
}

/// Flatten a labelled map into a vector of `(grid, corner_idx)` pairs
/// sorted by `(j, i)` — row-major, top-to-bottom then left-to-right.
///
/// Downstream overlay and calibration consumers want a stable order;
/// this provides it without imposing a result struct.
pub fn sorted_grid_points(labelled: &HashMap<(i32, i32), usize>) -> Vec<((i32, i32), usize)> {
    let mut out: Vec<((i32, i32), usize)> = labelled.iter().map(|(&k, &v)| (k, v)).collect();
    out.sort_by_key(|&((i, j), _)| (j, i));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebase_shifts_bbox_minimum_to_origin() {
        let mut m = HashMap::new();
        m.insert((3, 5), 0);
        m.insert((4, 5), 1);
        m.insert((3, 6), 2);
        let r = rebase_to_origin(m);
        assert!(r.contains_key(&(0, 0)));
        assert!(r.contains_key(&(1, 0)));
        assert!(r.contains_key(&(0, 1)));
    }

    #[test]
    fn rebase_empty_is_noop() {
        let r = rebase_to_origin(HashMap::new());
        assert!(r.is_empty());
    }

    #[test]
    fn prune_drops_disconnected_singleton() {
        let mut m = HashMap::new();
        // 3-cell connected component.
        m.insert((0, 0), 0);
        m.insert((1, 0), 1);
        m.insert((0, 1), 2);
        // Disconnected singleton far away.
        m.insert((50, 50), 3);
        let r = prune_to_main_component(m);
        assert_eq!(r.len(), 3);
        assert!(!r.values().any(|&v| v == 3));
    }

    #[test]
    fn sorted_grid_points_is_row_major() {
        let mut m = HashMap::new();
        m.insert((1, 0), 1);
        m.insert((0, 0), 0);
        m.insert((0, 1), 2);
        let s = sorted_grid_points(&m);
        assert_eq!(s, vec![((0, 0), 0), ((1, 0), 1), ((0, 1), 2)]);
    }

    #[test]
    fn canonicalize_orients_plus_i_right_plus_j_down() {
        // A grid laid out so +i runs DOWN and +j runs LEFT in pixel
        // space. Canonicalisation must rotate it back.
        let mut positions = Vec::new();
        let mut labelled = HashMap::new();
        let mut idx = 0;
        for j in 0..3 {
            for i in 0..3 {
                // +i → +y (down), +j → -x (left).
                let x = 100.0 - j as f32 * 20.0;
                let y = 100.0 + i as f32 * 20.0;
                positions.push(Point2::new(x, y));
                labelled.insert((i, j), idx);
                idx += 1;
            }
        }
        let (canon, _t) = canonicalize_top_left(labelled, &positions);
        // After canonicalisation, +i must point +x and +j must point +y.
        let p00 = positions[canon[&(0, 0)]];
        let p10 = positions[canon[&(1, 0)]];
        let p01 = positions[canon[&(0, 1)]];
        assert!(p10.x > p00.x, "+i should point right: {p00:?} -> {p10:?}");
        assert!(p01.y > p00.y, "+j should point down: {p00:?} -> {p01:?}");
    }
}
