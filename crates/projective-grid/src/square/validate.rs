//! Post-growth validation for a labelled square grid.
//!
//! Two independent checks run over the labelled set:
//!
//! 1. **Line collinearity.** For every row (`j = const`) and column
//!    (`i = const`) with `≥ line_min_members` labelled members, fit
//!    a least-squares line in pixel space and flag any member whose
//!    perpendicular residual exceeds `line_tol_rel × cell_size`.
//!
//! 2. **Local-H residual.** For every labelled corner with ≥ 4
//!    non-collinear labelled neighbors in `(i, j)`-space, fit a 4-point
//!    local homography from the 4 grid-closest neighbors, predict the
//!    corner's pixel position, and measure the residual. Corners whose
//!    residual exceeds `local_h_tol_rel × cell_size` are flagged.
//!
//! Flags are combined via the attribution rules below into a
//! blacklist of **indices into the input slice**:
//!
//! * A corner flagged in `≥ 2` lines is the outlier.
//! * A corner with a *large* local-H residual (`> 2 × local_h_tol`) AND
//!   at least one line flag is the outlier.
//! * A corner with a local-H flag but NO line flag, where at least one
//!   of its 4 base neighbors has `≥ 1` line flags, blames the worst-
//!   line-flagged base instead (the base is the outlier).
//! * Otherwise (isolated local-H flag with no supporting evidence),
//!   defer — no blacklist entry in this iteration.
//!
//! The caller is expected to re-run the seed/grow/validate loop after
//! updating its blacklist.
//!
//! # Pattern-agnostic
//!
//! This module has no dependency on chessboard-specific vocabulary
//! (parity, axis clusters, label enums). Any caller that can produce
//! a `(corner_index, pixel_position, grid_coord)` slice can use it.
//! Consumers that carry per-stage metadata should pre-filter to the
//! "labelled" subset before calling.

use crate::homography::homography_from_4pt;
use nalgebra::Point2;
use std::collections::{HashMap, HashSet};

/// Tolerances for the validation pass.
///
/// All spatial tolerances are expressed as ratios of the grid's cell
/// size; `validate` multiplies them by the caller-supplied
/// `cell_size` at runtime.
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct ValidationParams {
    /// Straight-line fit collinearity tolerance (fraction of
    /// `cell_size`).
    pub line_tol_rel: f32,
    /// Projective-line fit collinearity tolerance (fraction of
    /// `cell_size`). Looser than `line_tol_rel` to accommodate lens
    /// distortion. Currently reserved for a projective-line check;
    /// not yet consumed, but kept in the param surface so chessboard's
    /// `DetectorParams` maps 1:1.
    pub projective_line_tol_rel: f32,
    /// Minimum members required to fit a line / column.
    pub line_min_members: usize,
    /// Local-H prediction tolerance (fraction of `cell_size`).
    pub local_h_tol_rel: f32,
}

impl Default for ValidationParams {
    fn default() -> Self {
        // Matches `calib_targets_chessboard::DetectorParams`'s
        // validation defaults so the thin chessboard wrapper stays a
        // pure forward of the same numbers.
        Self {
            line_tol_rel: 0.15,
            projective_line_tol_rel: 0.25,
            line_min_members: 3,
            local_h_tol_rel: 0.20,
        }
    }
}

impl ValidationParams {
    /// Construct fully-specified tolerances. Use this from outside the
    /// crate since the struct is [`#[non_exhaustive]`] — new optional
    /// tolerances added later get sensible defaults via the returned
    /// instance.
    pub fn new(
        line_tol_rel: f32,
        projective_line_tol_rel: f32,
        line_min_members: usize,
        local_h_tol_rel: f32,
    ) -> Self {
        Self {
            line_tol_rel,
            projective_line_tol_rel,
            line_min_members,
            local_h_tol_rel,
        }
    }
}

/// A single labelled corner fed into [`validate`]: its caller-chosen
/// index (carried back in `ValidationResult::blacklist`), its pixel
/// position, and its integer grid coordinate.
///
/// The index is opaque to this module — callers may pick any scheme
/// (direct slice indices, corner struct fields, etc.) as long as the
/// same scheme maps `blacklist` entries back to their originals.
#[derive(Clone, Copy, Debug)]
pub struct LabelledEntry {
    pub idx: usize,
    pub pixel: Point2<f32>,
    pub grid: (i32, i32),
}

/// Outcome of one validation pass.
#[derive(Debug, Default)]
pub struct ValidationResult {
    /// Corner indices to blacklist (attribution has been applied).
    pub blacklist: HashSet<usize>,
    /// For each labelled corner, its local-H residual in pixels
    /// (`None` when fewer than 4 non-collinear neighbors were
    /// available).
    pub local_h_residuals: HashMap<usize, f32>,
}

/// Run both validation passes and produce a blacklist.
pub fn validate(
    entries: &[LabelledEntry],
    cell_size: f32,
    params: &ValidationParams,
) -> ValidationResult {
    let line_tol_px = params.line_tol_rel * cell_size;
    let local_h_tol_px = params.local_h_tol_rel * cell_size;
    let high_tol_px = 2.0 * local_h_tol_px;

    // Quick lookup maps (built once per call).
    let by_idx: HashMap<usize, &LabelledEntry> = entries.iter().map(|e| (e.idx, e)).collect();
    let by_grid: HashMap<(i32, i32), usize> = entries.iter().map(|e| (e.grid, e.idx)).collect();

    // --- 7a. Line collinearity ------------------------------------------
    let line_flags = line_collinearity_flags(&by_idx, &by_grid, line_tol_px, params);

    // --- 7b. Local-H residual -------------------------------------------
    let mut residuals: HashMap<usize, f32> = HashMap::new();
    let mut local_h_flagged: HashMap<usize, f32> = HashMap::new();
    for entry in entries {
        let base = pick_local_h_base(&by_grid, entry.idx, entry.grid);
        if base.len() < 4 {
            continue;
        }
        let Some(resid) = local_h_residual(&by_idx, entry.idx, entry.grid, &base, &by_grid) else {
            continue;
        };
        residuals.insert(entry.idx, resid);
        if resid > local_h_tol_px {
            local_h_flagged.insert(entry.idx, resid);
        }
    }

    // --- 7c. Attribution -------------------------------------------------
    let mut blacklist: HashSet<usize> = HashSet::new();
    // Rule 1: ≥ 2 line flags → outlier.
    for (&idx, &count) in &line_flags {
        if count >= 2 {
            blacklist.insert(idx);
        }
    }
    // Rule 2: high local-H residual AND ≥ 1 line flag → outlier.
    for (&idx, &resid) in &local_h_flagged {
        if resid > high_tol_px && line_flags.get(&idx).copied().unwrap_or(0) >= 1 {
            blacklist.insert(idx);
        }
    }
    // Rule 3: local-H flag with no line flag BUT base neighbor flagged
    // in a line → blacklist the worst base instead.
    for &idx in local_h_flagged.keys() {
        if line_flags.get(&idx).copied().unwrap_or(0) >= 1 {
            continue;
        }
        if blacklist.contains(&idx) {
            continue;
        }
        let Some(entry) = by_idx.get(&idx) else {
            continue;
        };
        let base = pick_local_h_base(&by_grid, idx, entry.grid);
        let mut worst: Option<(usize, u32)> = None;
        for base_idx in &base {
            if let Some(&flags) = line_flags.get(base_idx) {
                if flags >= 1 && worst.map(|w| flags > w.1).unwrap_or(true) {
                    worst = Some((*base_idx, flags));
                }
            }
        }
        if let Some((base_idx, _)) = worst {
            blacklist.insert(base_idx);
        }
    }

    ValidationResult {
        blacklist,
        local_h_residuals: residuals,
    }
}

// --- line collinearity ----------------------------------------------------

fn line_collinearity_flags(
    by_idx: &HashMap<usize, &LabelledEntry>,
    by_grid: &HashMap<(i32, i32), usize>,
    tol_px: f32,
    params: &ValidationParams,
) -> HashMap<usize, u32> {
    let mut flags: HashMap<usize, u32> = HashMap::new();

    // Group by row (j = const) and column (i = const).
    let mut rows: HashMap<i32, Vec<(i32, usize)>> = HashMap::new();
    let mut cols: HashMap<i32, Vec<(i32, usize)>> = HashMap::new();
    for (&(i, j), &idx) in by_grid {
        rows.entry(j).or_default().push((i, idx));
        cols.entry(i).or_default().push((j, idx));
    }

    let line_min = params.line_min_members;

    for (_, mut members) in rows {
        if members.len() < line_min {
            continue;
        }
        members.sort_by_key(|(i, _)| *i);
        flag_line(by_idx, &members, tol_px, &mut flags);
    }
    for (_, mut members) in cols {
        if members.len() < line_min {
            continue;
        }
        members.sort_by_key(|(j, _)| *j);
        flag_line(by_idx, &members, tol_px, &mut flags);
    }
    flags
}

/// Fit a total-least-squares line to the member pixel positions; flag
/// any member whose perpendicular distance exceeds `tol_px`.
fn flag_line(
    by_idx: &HashMap<usize, &LabelledEntry>,
    members: &[(i32, usize)],
    tol_px: f32,
    flags: &mut HashMap<usize, u32>,
) {
    let n = members.len() as f32;
    let mut cx = 0.0_f32;
    let mut cy = 0.0_f32;
    for (_, idx) in members {
        let Some(e) = by_idx.get(idx) else { continue };
        cx += e.pixel.x;
        cy += e.pixel.y;
    }
    cx /= n;
    cy /= n;
    let mut sxx = 0.0_f32;
    let mut sxy = 0.0_f32;
    let mut syy = 0.0_f32;
    for (_, idx) in members {
        let Some(e) = by_idx.get(idx) else { continue };
        let dx = e.pixel.x - cx;
        let dy = e.pixel.y - cy;
        sxx += dx * dx;
        sxy += dx * dy;
        syy += dy * dy;
    }
    let trace = sxx + syy;
    let det = sxx * syy - sxy * sxy;
    let disc = (trace * trace * 0.25 - det).max(0.0).sqrt();
    let lambda = trace * 0.5 + disc;
    let (vx, vy) = if sxy.abs() > f32::EPSILON {
        (sxy, lambda - sxx)
    } else if sxx >= syy {
        (1.0, 0.0)
    } else {
        (0.0, 1.0)
    };
    let vn = (vx * vx + vy * vy).sqrt().max(f32::EPSILON);
    let ux = vx / vn;
    let uy = vy / vn;
    for (_, idx) in members {
        let Some(e) = by_idx.get(idx) else { continue };
        let dx = e.pixel.x - cx;
        let dy = e.pixel.y - cy;
        let resid = (dx * -uy + dy * ux).abs();
        if resid > tol_px {
            *flags.entry(*idx).or_insert(0) += 1;
        }
    }
}

// --- local-H residual -----------------------------------------------------

/// Pick the 4 grid-closest labelled neighbors of `c_idx` at `pos`
/// that form a non-degenerate quad (i.e., not all collinear in grid
/// coordinates).
fn pick_local_h_base(
    by_grid: &HashMap<(i32, i32), usize>,
    c_idx: usize,
    pos: (i32, i32),
) -> Vec<usize> {
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

fn local_h_residual(
    by_idx: &HashMap<usize, &LabelledEntry>,
    c_idx: usize,
    c_grid: (i32, i32),
    base: &[usize],
    by_grid: &HashMap<(i32, i32), usize>,
) -> Option<f32> {
    if base.len() < 4 {
        return None;
    }
    // Resolve each base index back to its grid coordinate. The base
    // list came from neighbourhood enumeration so each one is present
    // in `by_grid` under some unique key.
    let mut base_grid: [(i32, i32); 4] = [(0, 0); 4];
    let mut base_pixel: [Point2<f32>; 4] = [Point2::new(0.0, 0.0); 4];
    for (k, &b_idx) in base.iter().take(4).enumerate() {
        let grid = by_grid
            .iter()
            .find_map(|(&g, &v)| if v == b_idx { Some(g) } else { None })?;
        base_grid[k] = grid;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(idx: usize, x: f32, y: f32, i: i32, j: i32) -> LabelledEntry {
        LabelledEntry {
            idx,
            pixel: Point2::new(x, y),
            grid: (i, j),
        }
    }

    fn clean_grid(rows: i32, cols: i32, s: f32) -> Vec<LabelledEntry> {
        let mut out = Vec::new();
        let mut idx = 0;
        for j in 0..rows {
            for i in 0..cols {
                out.push(entry(idx, i as f32 * s + 50.0, j as f32 * s + 50.0, i, j));
                idx += 1;
            }
        }
        out
    }

    #[test]
    fn clean_grid_empty_blacklist() {
        let entries = clean_grid(7, 7, 20.0);
        let res = validate(&entries, 20.0, &ValidationParams::default());
        assert!(res.blacklist.is_empty(), "{:?}", res.blacklist);
    }

    #[test]
    fn displaced_interior_is_blacklisted() {
        let mut entries = clean_grid(7, 7, 20.0);
        // Displace (3, 3) by ~6px in both directions — failing both
        // line fits and the local-H residual check.
        let target = entries
            .iter_mut()
            .find(|e| e.grid == (3, 3))
            .expect("(3,3) present");
        target.pixel.x += 6.0;
        target.pixel.y += 6.0;
        let target_idx = target.idx;
        let res = validate(&entries, 20.0, &ValidationParams::default());
        assert!(
            res.blacklist.contains(&target_idx),
            "expected {target_idx} blacklisted, got {:?}",
            res.blacklist
        );
    }

    #[test]
    fn too_few_members_per_line_is_ignored() {
        let entries = vec![entry(0, 0.0, 0.0, 0, 0), entry(1, 20.0, 0.0, 1, 0)];
        let res = validate(&entries, 20.0, &ValidationParams::default());
        assert!(res.blacklist.is_empty());
    }
}
