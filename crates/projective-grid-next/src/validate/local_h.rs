//! Per-corner local homography reprojection residual.
//!
//! For every labelled corner with at least 4 non-collinear labelled neighbours
//! in `(i, j)`-space, fits a 4-point local homography from the 4 grid-closest
//! neighbours, projects the corner's grid coordinate through that homography,
//! and returns the pixel residual to the corner's actual position.

use std::collections::HashMap;

use nalgebra::Point2;

use crate::float::{lit, Float};
use crate::geometry::homography_from_4pt;

use super::LabelledEntry;

/// Pick up to 4 labelled neighbours of `c_idx` at `pos` from the local
/// `[-2, 2]` grid window that form a non-degenerate quad (not all collinear in
/// grid coordinates).
pub(super) fn pick_local_h_base<F: Float>(
    by_grid: &HashMap<(i32, i32), usize>,
    c_idx: usize,
    pos: (i32, i32),
) -> Vec<usize> {
    let mut cands: Vec<((i32, i32), usize, F)> = Vec::new();
    for dj in -2..=2_i32 {
        for di in -2..=2_i32 {
            if di == 0 && dj == 0 {
                continue;
            }
            let neigh = (pos.0 + di, pos.1 + dj);
            if let Some(&idx) = by_grid.get(&neigh) {
                if idx == c_idx {
                    continue;
                }
                let dist_sq = lit::<F>((di * di + dj * dj) as f32);
                cands.push((neigh, idx, dist_sq.sqrt()));
            }
        }
    }
    cands.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));

    let mut chosen: Vec<((i32, i32), usize)> = Vec::new();
    for (ij, idx, _) in &cands {
        chosen.push((*ij, *idx));
        if chosen.len() == 4 && !are_collinear_grid(&chosen) {
            return chosen.iter().map(|(_, i)| *i).collect();
        }
        if chosen.len() >= 4 {
            chosen.pop();
        }
    }
    chosen.iter().map(|(_, i)| *i).collect()
}

fn are_collinear_grid(pts: &[((i32, i32), usize)]) -> bool {
    if pts.len() < 3 {
        return false;
    }
    let (i0, j0) = pts[0].0;
    let (i1, j1) = pts[1].0;
    let dx1 = i1 - i0;
    let dy1 = j1 - j0;
    for &((i, j), _) in &pts[2..] {
        let dx = i - i0;
        let dy = j - j0;
        if dx1 * dy - dy1 * dx != 0 {
            return false;
        }
    }
    true
}

/// Compute the 4-point local homography from `base` (4 labelled neighbours)
/// onto pixel space, then return the residual between the predicted and
/// actual pixel position of corner `c_idx` at `c_grid`.
///
/// Returns `None` when fewer than 4 base corners are available, when the
/// homography fit fails, or when any base corner cannot be resolved back to
/// its grid coordinate.
pub(super) fn local_h_residual<F: Float>(
    by_idx: &HashMap<usize, &LabelledEntry<F>>,
    c_idx: usize,
    c_grid: (i32, i32),
    base: &[usize],
    by_grid: &HashMap<(i32, i32), usize>,
) -> Option<F> {
    if base.len() < 4 {
        return None;
    }
    // Resolve each base index back to its grid coordinate. Base indices came
    // from the local neighbourhood scan, so each one is present in `by_grid`
    // under some unique coordinate key.
    let mut base_grid: [(i32, i32); 4] = [(0, 0); 4];
    let mut base_pixel: [Point2<F>; 4] = [Point2::new(F::zero(), F::zero()); 4];
    for (k, &b_idx) in base.iter().take(4).enumerate() {
        let grid = by_grid
            .iter()
            .find_map(|(&g, &v)| if v == b_idx { Some(g) } else { None })?;
        base_grid[k] = grid;
        base_pixel[k] = by_idx.get(&b_idx)?.position;
    }

    let grid_pts = [
        Point2::new(
            lit::<F>(base_grid[0].0 as f32),
            lit::<F>(base_grid[0].1 as f32),
        ),
        Point2::new(
            lit::<F>(base_grid[1].0 as f32),
            lit::<F>(base_grid[1].1 as f32),
        ),
        Point2::new(
            lit::<F>(base_grid[2].0 as f32),
            lit::<F>(base_grid[2].1 as f32),
        ),
        Point2::new(
            lit::<F>(base_grid[3].0 as f32),
            lit::<F>(base_grid[3].1 as f32),
        ),
    ];
    let h = homography_from_4pt(&grid_pts, &base_pixel)?;

    let c_pixel = by_idx.get(&c_idx)?.position;
    let pred = h.apply(Point2::new(
        lit::<F>(c_grid.0 as f32),
        lit::<F>(c_grid.1 as f32),
    ));
    let dx = pred.x - c_pixel.x;
    let dy = pred.y - c_pixel.y;
    Some((dx * dx + dy * dy).sqrt())
}
