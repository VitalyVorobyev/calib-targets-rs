//! Line collinearity validation.
//!
//! For every grid row (`j = const`) and column (`i = const`) with at
//! least `line_min_members` labelled members, fits a total-least-squares
//! line in pixel space and counts the collinearity violations per corner.

use crate::square::validate::{LabelledEntry, ValidationParams};
use std::collections::HashMap;

/// For each labelled corner that violates at least one row/column line,
/// return the number of lines it violates.
pub(super) fn line_collinearity_flags(
    by_idx: &HashMap<usize, &LabelledEntry>,
    by_grid: &HashMap<(i32, i32), usize>,
    params: &ValidationParams,
    scale_at: &dyn Fn(usize) -> f32,
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
    let line_tol_rel = params.line_tol_rel;

    for (_, mut members) in rows {
        if members.len() < line_min {
            continue;
        }
        members.sort_by_key(|(i, _)| *i);
        flag_line(by_idx, &members, line_tol_rel, scale_at, &mut flags);
    }
    for (_, mut members) in cols {
        if members.len() < line_min {
            continue;
        }
        members.sort_by_key(|(j, _)| *j);
        flag_line(by_idx, &members, line_tol_rel, scale_at, &mut flags);
    }
    flags
}

/// Fit a total-least-squares line to the member pixel positions; flag
/// any member whose perpendicular distance exceeds
/// `line_tol_rel × scale_at(member.idx)`.
fn flag_line(
    by_idx: &HashMap<usize, &LabelledEntry>,
    members: &[(i32, usize)],
    line_tol_rel: f32,
    scale_at: &dyn Fn(usize) -> f32,
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
        let tol = line_tol_rel * scale_at(*idx);
        if resid > tol {
            *flags.entry(*idx).or_insert(0) += 1;
        }
    }
}
