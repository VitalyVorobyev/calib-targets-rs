//! Row / column collinearity validation.
//!
//! For every grid row (`j = const`) and column (`i = const`) with at least
//! [`ValidationParams::line_min_members`] labelled members, fits a total-
//! least-squares line in pixel space and counts the collinearity violations
//! per corner.
//!
//! The fit is the legacy SVD-style symmetric 2x2 scatter eigendecomposition,
//! ported verbatim into `F`. The returned map is `corner idx -> flag count`
//! (a corner can appear on at most one row and one column, so the count is
//! `0`, `1`, or `2`).
//!
//! [`ValidationParams::line_min_members`]: super::ValidationParams::line_min_members

use std::collections::HashMap;

use nalgebra::{ComplexField, RealField};

use crate::float::{lit, Float};

use super::{LabelledEntry, ValidationParams};

/// For each labelled corner that violates at least one row or column line,
/// return the number of lines it violates.
pub(super) fn line_collinearity_flags<F: Float>(
    by_idx: &HashMap<usize, &LabelledEntry<F>>,
    by_grid: &HashMap<(i32, i32), usize>,
    params: &ValidationParams<F>,
    scale_at: &dyn Fn(usize) -> F,
) -> HashMap<usize, u32> {
    let mut flags: HashMap<usize, u32> = HashMap::new();

    // Group labelled corners by row (`j = const`) and column (`i = const`).
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

/// Fit a total-least-squares line to the member pixel positions and flag any
/// member whose perpendicular distance exceeds
/// `line_tol_rel * scale_at(member.idx)`.
fn flag_line<F: Float>(
    by_idx: &HashMap<usize, &LabelledEntry<F>>,
    members: &[(i32, usize)],
    line_tol_rel: F,
    scale_at: &dyn Fn(usize) -> F,
    flags: &mut HashMap<usize, u32>,
) {
    let n_count = members.len();
    if n_count < 2 {
        return;
    }
    let n: F = lit::<F>(n_count as f32);

    // Centroid in pixel space.
    let mut cx = F::zero();
    let mut cy = F::zero();
    for (_, idx) in members {
        let Some(e) = by_idx.get(idx) else { continue };
        cx += e.position.x;
        cy += e.position.y;
    }
    cx /= n;
    cy /= n;

    // 2x2 scatter matrix.
    let mut sxx = F::zero();
    let mut sxy = F::zero();
    let mut syy = F::zero();
    for (_, idx) in members {
        let Some(e) = by_idx.get(idx) else { continue };
        let dx = e.position.x - cx;
        let dy = e.position.y - cy;
        sxx += dx * dx;
        sxy += dx * dy;
        syy += dy * dy;
    }

    // Largest eigenvalue and its eigenvector direction.
    let trace = sxx + syy;
    let det = sxx * syy - sxy * sxy;
    let quarter = lit::<F>(0.25_f32);
    let half = lit::<F>(0.5_f32);
    let disc_sq = trace * trace * quarter - det;
    let disc = RealField::max(disc_sq, F::zero()).sqrt();
    let lambda = trace * half + disc;
    let eps = F::default_epsilon();
    let (vx, vy) = if ComplexField::abs(sxy) > eps {
        (sxy, lambda - sxx)
    } else if sxx >= syy {
        (F::one(), F::zero())
    } else {
        (F::zero(), F::one())
    };
    let vn = RealField::max((vx * vx + vy * vy).sqrt(), eps);
    let ux = vx / vn;
    let uy = vy / vn;

    // Flag members whose perpendicular residual exceeds the tolerance.
    for (_, idx) in members {
        let Some(e) = by_idx.get(idx) else { continue };
        let dx = e.position.x - cx;
        let dy = e.position.y - cy;
        let resid = ComplexField::abs(dx * (-uy) + dy * ux);
        let tol = line_tol_rel * scale_at(*idx);
        if resid > tol {
            *flags.entry(*idx).or_insert(0) += 1;
        }
    }
}
