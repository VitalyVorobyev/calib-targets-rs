//! Local-homography residual validation.
//!
//! For every labelled corner with ≥ 4 non-collinear labelled neighbours
//! in `(i, j)`-space, fits a 4-point local homography and measures the
//! reprojection residual.

use crate::geometry::homography_from_4pt;
use crate::shared::validate::LabelledEntry;
use nalgebra::Point2;
use std::collections::HashMap;

/// Pick the 4 grid-closest labelled neighbors of `c_idx` at `pos`
/// that form a non-degenerate quad (i.e., not all collinear in grid
/// coordinates).
///
/// Each base is returned as `(idx, grid)`. The grid coordinate is the
/// *neighbour cell it was found at*, which is authoritative even when the
/// labelled set is non-injective — i.e. when the topological walk has
/// labelled the same `source_index` at two different grid cells. Returning
/// the grid here (rather than re-deriving it later by inverting the
/// `grid -> idx` map) is a determinism contract: that inversion is
/// `HashMap`-iteration-order dependent for a duplicated index and was the
/// residual source of the topological->ChArUco recall flake (the same base
/// quad yielded a 0.5 px or a 57 px residual run-to-run depending on which
/// of a duplicated corner's two grid cells the inversion happened to pick).
pub(super) fn pick_local_h_base(
    by_grid: &HashMap<(i32, i32), usize>,
    c_idx: usize,
    pos: (i32, i32),
) -> Vec<(usize, (i32, i32))> {
    let mut cands: Vec<((i32, i32), usize, f32)> = Vec::new();
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
                let d = ((di * di + dj * dj) as f32).sqrt();
                cands.push((neigh, idx, d));
            }
        }
    }
    cands.sort_by(|a, b| a.2.total_cmp(&b.2));

    let mut chosen: Vec<((i32, i32), usize)> = Vec::new();
    for (ij, idx, _) in &cands {
        chosen.push((*ij, *idx));
        if chosen.len() == 4 && !are_collinear_grid(&chosen) {
            return chosen.iter().map(|(ij, i)| (*i, *ij)).collect();
        }
        if chosen.len() >= 4 {
            chosen.pop();
        }
    }
    chosen.iter().map(|(ij, i)| (*i, *ij)).collect()
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

pub(super) fn local_h_residual(
    by_idx: &HashMap<usize, &LabelledEntry>,
    c_idx: usize,
    c_grid: (i32, i32),
    base: &[(usize, (i32, i32))],
) -> Option<f32> {
    if base.len() < 4 {
        return None;
    }
    // Each base carries the grid cell it was found at (see
    // `pick_local_h_base`). Using that directly — rather than inverting the
    // `grid -> idx` map — keeps the fit deterministic when the labelled set
    // is non-injective (a `source_index` labelled at two grid cells): the
    // inversion would pick a grid cell in `HashMap` iteration order, which
    // varies per process and flipped this corner's residual run-to-run.
    let mut base_grid: [(i32, i32); 4] = [(0, 0); 4];
    let mut base_pixel: [Point2<f32>; 4] = [Point2::new(0.0, 0.0); 4];
    for (k, &(b_idx, b_grid)) in base.iter().take(4).enumerate() {
        base_grid[k] = b_grid;
        base_pixel[k] = by_idx.get(&b_idx)?.pixel;
    }

    let grid_pts = [
        Point2::new(base_grid[0].0 as f32, base_grid[0].1 as f32),
        Point2::new(base_grid[1].0 as f32, base_grid[1].1 as f32),
        Point2::new(base_grid[2].0 as f32, base_grid[2].1 as f32),
        Point2::new(base_grid[3].0 as f32, base_grid[3].1 as f32),
    ];
    let h = homography_from_4pt(&grid_pts, &base_pixel)?;

    let c_pixel = by_idx.get(&c_idx)?.pixel;
    let pred = h.apply(Point2::new(c_grid.0 as f32, c_grid.1 as f32));
    let dx = pred.x - c_pixel.x;
    let dy = pred.y - c_pixel.y;
    Some((dx * dx + dy * dy).sqrt())
}
