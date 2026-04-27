//! Per-corner local step estimation and step-deviation flagging.
//!
//! Provides [`local_step_per_corner`] for computing a per-corner scale
//! from labelled-grid finite differences, and [`flag_step_deviations`]
//! for detecting corners whose step deviates from the set median.

use crate::square::validate::LabelledEntry;
use std::collections::{HashMap, HashSet};

/// Compute per-corner local grid step from labelled grid neighbours.
///
/// Uses central finite differences when both `(i±1, j)` (or `(i, j±1)`)
/// neighbours are labelled, falls back to one-sided difference, and
/// returns nothing when neither axis has enough labelled neighbours.
/// The returned value is `(|i_step| + |j_step|) / 2` in pixels — the
/// same scalar metric used by `find_inconsistent_corners_step_aware`.
///
/// See also: [`crate::local_step::estimate_local_steps`] — a public,
/// axes-aware alternative with outlier handling and confidence scoring.
pub(super) fn local_step_per_corner(
    by_idx: &HashMap<usize, &LabelledEntry>,
    by_grid: &HashMap<(i32, i32), usize>,
) -> HashMap<usize, f32> {
    let mut out = HashMap::with_capacity(by_idx.len());
    for (&idx, entry) in by_idx {
        let (i, j) = entry.grid;
        let here = entry.pixel;
        let i_step = match (
            by_grid
                .get(&(i - 1, j))
                .and_then(|k| by_idx.get(k))
                .map(|e| e.pixel),
            by_grid
                .get(&(i + 1, j))
                .and_then(|k| by_idx.get(k))
                .map(|e| e.pixel),
        ) {
            (Some(l), Some(r)) => Some((r - l).norm() * 0.5),
            (Some(l), None) => Some((here - l).norm()),
            (None, Some(r)) => Some((r - here).norm()),
            (None, None) => None,
        };
        let j_step = match (
            by_grid
                .get(&(i, j - 1))
                .and_then(|k| by_idx.get(k))
                .map(|e| e.pixel),
            by_grid
                .get(&(i, j + 1))
                .and_then(|k| by_idx.get(k))
                .map(|e| e.pixel),
        ) {
            (Some(u), Some(d)) => Some((d - u).norm() * 0.5),
            (Some(u), None) => Some((here - u).norm()),
            (None, Some(d)) => Some((d - here).norm()),
            (None, None) => None,
        };
        let step = match (i_step, j_step) {
            (Some(a), Some(b)) => Some((a + b) * 0.5),
            (Some(a), None) | (None, Some(a)) => Some(a),
            (None, None) => None,
        };
        if let Some(s) = step {
            if s.is_finite() && s > 0.0 {
                out.insert(idx, s);
            }
        }
    }
    out
}

/// Flag corners whose local step deviates from the labelled-set
/// median by more than `deviation_thresh_rel`.
///
/// A step `s` is flagged when `s < median / (1 + thresh)` or
/// `s > median * (1 + thresh)`. Returns an empty set when there are
/// fewer than 3 step values (median is meaningless).
pub(super) fn flag_step_deviations(
    local_steps: &HashMap<usize, f32>,
    deviation_thresh_rel: f32,
) -> HashSet<usize> {
    if deviation_thresh_rel <= 0.0 || local_steps.len() < 3 {
        return HashSet::new();
    }
    let mut steps: Vec<f32> = local_steps.values().copied().collect();
    steps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = steps[steps.len() / 2];
    if median <= 0.0 || !median.is_finite() {
        return HashSet::new();
    }
    let lo = median / (1.0 + deviation_thresh_rel);
    let hi = median * (1.0 + deviation_thresh_rel);
    local_steps
        .iter()
        .filter_map(|(&idx, &s)| if s < lo || s > hi { Some(idx) } else { None })
        .collect()
}
