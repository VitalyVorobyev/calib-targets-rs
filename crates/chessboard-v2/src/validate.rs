//! Stage 7 — post-growth validation.
//!
//! Two independent checks run over the labelled set:
//!
//! 1. **Line collinearity.** For every row (`j = const`) and column
//!    (`i = const`) with `≥ line_min_members` labelled members, fit
//!    a straight line by least squares and record each member's
//!    perpendicular residual. Members with residual
//!    `> line_tol × s` are flagged.
//!
//! 2. **Local-H residual.** For every labelled corner with ≥ 4
//!    non-collinear labelled neighbors in `(i, j)`-space, fit a
//!    4-point local homography from the 4 grid-closest neighbors,
//!    predict the corner's pixel position, and measure the residual.
//!    Corners with residual `> local_h_tol × s` are flagged.
//!
//! Flags are combined into a **blacklist** via the attribution rules
//! in spec §5.7c:
//!
//! * A corner flagged in `≥ 2` lines is the outlier.
//! * A corner with a large local-H residual (`> 2 × tol`) AND at
//!   least one line flag is the outlier.
//! * A corner with a local-H flag but NO line flag, where at least
//!   one of its 4 base neighbors has `≥ 1` line flags, blames the
//!   worst-line-flagged base instead (the base is the outlier).
//! * Otherwise (isolated local-H flag with no supporting evidence),
//!   defer — no blacklist entry in this iteration.
//!
//! The detector re-runs Stages 5–7 after each blacklist update; the
//! loop is capped at `max_validation_iters`.

use crate::corner::CornerAug;
use crate::params::DetectorParams;
use nalgebra::Point2;
use projective_grid::homography_from_4pt;
use std::collections::{HashMap, HashSet};

/// Outcome of one validation pass.
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
    corners: &[CornerAug],
    labelled: &HashMap<(i32, i32), usize>,
    cell_size: f32,
    params: &DetectorParams,
) -> ValidationResult {
    let line_tol_px = params.line_tol_rel * cell_size;
    let local_h_tol_px = params.local_h_tol_rel * cell_size;
    let high_tol_px = 2.0 * local_h_tol_px;

    // --- 7a. Line collinearity ------------------------------------------
    let line_flags = line_collinearity_flags(corners, labelled, line_tol_px, params);

    // --- 7b. Local-H residual -------------------------------------------
    let mut residuals: HashMap<usize, f32> = HashMap::new();
    let mut local_h_flagged: HashMap<usize, f32> = HashMap::new();
    for (&(_i, _j), &c_idx) in labelled.iter() {
        let base = pick_local_h_base(labelled, c_idx, corners, (_i, _j));
        if base.len() < 4 {
            continue;
        }
        let Some(resid) = local_h_residual(corners, c_idx, &base) else {
            continue;
        };
        residuals.insert(c_idx, resid);
        if resid > local_h_tol_px {
            local_h_flagged.insert(c_idx, resid);
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
        // Re-collect base for attribution.
        let (&at, _) = labelled.iter().find(|(_, &v)| v == idx).unwrap();
        let base = pick_local_h_base(labelled, idx, corners, at);
        // Pick worst-line-flagged base.
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
        // Otherwise: defer, no blacklist.
    }

    ValidationResult {
        blacklist,
        local_h_residuals: residuals,
    }
}

// --- line collinearity ----------------------------------------------------

fn line_collinearity_flags(
    corners: &[CornerAug],
    labelled: &HashMap<(i32, i32), usize>,
    tol_px: f32,
    params: &DetectorParams,
) -> HashMap<usize, u32> {
    let mut flags: HashMap<usize, u32> = HashMap::new();

    // Group by row (j = const) and column (i = const).
    let mut rows: HashMap<i32, Vec<(i32, usize)>> = HashMap::new();
    let mut cols: HashMap<i32, Vec<(i32, usize)>> = HashMap::new();
    for (&(i, j), &idx) in labelled {
        rows.entry(j).or_default().push((i, idx));
        cols.entry(i).or_default().push((j, idx));
    }

    let line_min = params.line_min_members;

    for (_j, mut members) in rows {
        if members.len() < line_min {
            continue;
        }
        members.sort_by_key(|(i, _)| *i);
        flag_line(corners, &members, tol_px, &mut flags);
    }
    for (_i, mut members) in cols {
        if members.len() < line_min {
            continue;
        }
        members.sort_by_key(|(j, _)| *j);
        flag_line(corners, &members, tol_px, &mut flags);
    }
    flags
}

/// Fit a least-squares line to the member pixel positions; flag any
/// member whose perpendicular distance exceeds `tol_px`.
///
/// `members`: `(along_coord_in_grid, corner_idx)` sorted by
/// `along_coord`. The along-coord is used only for sort / context; the
/// line fit is done in pixel space.
fn flag_line(
    corners: &[CornerAug],
    members: &[(i32, usize)],
    tol_px: f32,
    flags: &mut HashMap<usize, u32>,
) {
    // 2D line fit via total least squares: find the line minimising
    // sum of squared perpendicular distances. Compute centroid, then
    // the principal axis from the 2×2 covariance.
    let n = members.len() as f32;
    let mut cx = 0.0_f32;
    let mut cy = 0.0_f32;
    for (_, idx) in members {
        cx += corners[*idx].position.x;
        cy += corners[*idx].position.y;
    }
    cx /= n;
    cy /= n;
    let mut sxx = 0.0_f32;
    let mut sxy = 0.0_f32;
    let mut syy = 0.0_f32;
    for (_, idx) in members {
        let dx = corners[*idx].position.x - cx;
        let dy = corners[*idx].position.y - cy;
        sxx += dx * dx;
        sxy += dx * dy;
        syy += dy * dy;
    }
    // Principal eigenvector of [[sxx, sxy], [sxy, syy]].
    let trace = sxx + syy;
    let det = sxx * syy - sxy * sxy;
    let disc = (trace * trace * 0.25 - det).max(0.0).sqrt();
    let lambda = trace * 0.5 + disc;
    // Eigenvector: if sxy != 0, (lambda - syy, sxy); else (1, 0) or (0, 1).
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
    // Perpendicular residual per member.
    for (_, idx) in members {
        let dx = corners[*idx].position.x - cx;
        let dy = corners[*idx].position.y - cy;
        // Perpendicular component = projection onto n = rotated (ux, uy) by π/2.
        let resid = (dx * -uy + dy * ux).abs();
        if resid > tol_px {
            *flags.entry(*idx).or_insert(0) += 1;
        }
    }
}

// --- local-H residual -----------------------------------------------------

/// Pick the 4 grid-closest labelled neighbors of `c_idx` at `(i, j)`
/// to serve as base for a 4-point local homography.
///
/// Searches an expanding window and prefers neighbors that form a
/// non-degenerate quad (i.e., not all on one line).
fn pick_local_h_base(
    labelled: &HashMap<(i32, i32), usize>,
    c_idx: usize,
    _corners: &[CornerAug],
    pos: (i32, i32),
) -> Vec<usize> {
    // Candidates within a 2-cell window, sorted by grid distance.
    let mut cands: Vec<((i32, i32), usize, f32)> = Vec::new();
    for dj in -2..=2_i32 {
        for di in -2..=2_i32 {
            if di == 0 && dj == 0 {
                continue;
            }
            let neigh = (pos.0 + di, pos.1 + dj);
            if let Some(&idx) = labelled.get(&neigh) {
                if idx == c_idx {
                    continue;
                }
                let d = ((di * di + dj * dj) as f32).sqrt();
                cands.push((neigh, idx, d));
            }
        }
    }
    cands.sort_by(|a, b| a.2.total_cmp(&b.2));

    // Greedily pick 4 that are non-collinear. A set of 4 points is
    // non-collinear if at least one cross product is nonzero.
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

fn local_h_residual(corners: &[CornerAug], c_idx: usize, base: &[usize]) -> Option<f32> {
    if base.len() < 4 {
        return None;
    }
    // Take the first 4 base points for the 4-point homography fit.
    // (The caller already picked them for non-degeneracy.)
    let mut grid_pts = [Point2::new(0.0_f32, 0.0); 4];
    let mut img_pts = [Point2::new(0.0_f32, 0.0); 4];
    for k in 0..4 {
        let b_idx = base[k];
        let b = &corners[b_idx];
        let at = match b.stage {
            crate::corner::CornerStage::Labeled { at, .. } => at,
            _ => return None,
        };
        grid_pts[k] = Point2::new(at.0 as f32, at.1 as f32);
        img_pts[k] = b.position;
    }

    let h = homography_from_4pt(&grid_pts, &img_pts)?;

    let c = &corners[c_idx];
    let at = match c.stage {
        crate::corner::CornerStage::Labeled { at, .. } => at,
        _ => return None,
    };
    let pred = h.apply(Point2::new(at.0 as f32, at.1 as f32));
    let dx = pred.x - c.position.x;
    let dy = pred.y - c.position.y;
    Some((dx * dx + dy * dy).sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::cluster_axes;
    use crate::grow::grow_from_seed;
    use crate::seed::find_seed;
    use calib_targets_core::{AxisEstimate, Corner};
    use nalgebra::Point2;

    fn make_corner(idx: usize, x: f32, y: f32, swapped: bool) -> CornerAug {
        let (a0, a1) = if swapped {
            (std::f32::consts::FRAC_PI_2, 0.0)
        } else {
            (0.0, std::f32::consts::FRAC_PI_2)
        };
        let c = Corner {
            position: Point2::new(x, y),
            orientation_cluster: None,
            axes: [
                AxisEstimate {
                    angle: a0,
                    sigma: 0.01,
                },
                AxisEstimate {
                    angle: a1,
                    sigma: 0.01,
                },
            ],
            contrast: 10.0,
            fit_rms: 1.0,
            strength: 1.0,
        };
        let mut aug = CornerAug::from_corner(idx, &c);
        aug.stage = crate::corner::CornerStage::Strong;
        aug
    }

    fn build_clean_grid(rows: i32, cols: i32, s: f32) -> Vec<CornerAug> {
        let mut out = Vec::new();
        let mut idx = 0;
        for j in 0..rows {
            for i in 0..cols {
                let x = i as f32 * s + 50.0;
                let y = j as f32 * s + 50.0;
                let swapped = (i + j).rem_euclid(2) == 1;
                out.push(make_corner(idx, x, y, swapped));
                idx += 1;
            }
        }
        out
    }

    #[test]
    fn clean_grid_produces_no_blacklist() {
        let mut corners = build_clean_grid(7, 7, 20.0);
        let params = DetectorParams::default();
        let centers = cluster_axes(&mut corners, &params).expect("centers");
        let seed = find_seed(&corners, centers, &params).expect("seed").seed;
        let blacklist = HashSet::new();
        let res = grow_from_seed(&mut corners, seed, centers, 20.0, &blacklist, &params);
        assert_eq!(res.labelled.len(), 49);
        let v = validate(&corners, &res.labelled, 20.0, &params);
        assert!(
            v.blacklist.is_empty(),
            "clean grid should produce no blacklist, got {:?}",
            v.blacklist
        );
    }

    #[test]
    fn mislabeled_corner_is_blacklisted() {
        let mut corners = build_clean_grid(7, 7, 20.0);
        let params = DetectorParams::default();
        let centers = cluster_axes(&mut corners, &params).expect("centers");
        let seed = find_seed(&corners, centers, &params).expect("seed").seed;
        let blacklist = HashSet::new();
        let res = grow_from_seed(&mut corners, seed, centers, 20.0, &blacklist, &params);
        assert_eq!(res.labelled.len(), 49);

        // Displace one interior corner by a large amount so both its
        // row and column collinearity fail AND local-H predicts its
        // original spot.
        let mid_idx = 3 * 7 + 3; // (3, 3) interior
        corners[mid_idx].position.x += 6.0;
        corners[mid_idx].position.y += 6.0;

        let v = validate(&corners, &res.labelled, 20.0, &params);
        assert!(
            v.blacklist.contains(&mid_idx),
            "expected {mid_idx} to be blacklisted, got {:?}",
            v.blacklist
        );
    }
}
