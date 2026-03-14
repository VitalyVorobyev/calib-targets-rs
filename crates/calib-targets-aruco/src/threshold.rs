//! Thresholding utilities for marker decoding.

/// Compute Otsu threshold from a set of sample intensities.
pub(crate) fn otsu_threshold_from_samples(samples: &[u8]) -> u8 {
    if samples.is_empty() {
        return 127;
    }

    let mut min_v = 255u8;
    let mut max_v = 0u8;
    for &v in samples {
        min_v = min_v.min(v);
        max_v = max_v.max(v);
    }
    if min_v == max_v {
        return min_v;
    }

    let mut hist = [0u32; 256];
    for &v in samples {
        hist[v as usize] += 1;
    }
    let mut nonzero_bins = 0u32;
    for &h in &hist {
        if h > 0 {
            nonzero_bins += 1;
        }
    }
    if nonzero_bins <= 2 {
        return ((min_v as u16 + max_v as u16) / 2) as u8;
    }

    let total: f64 = samples.len() as f64;
    let mut sum_total = 0f64;
    for (i, &h) in hist.iter().enumerate() {
        sum_total += (i as f64) * (h as f64);
    }

    let mut sum_b = 0f64;
    let mut w_b = 0f64;
    let mut best_var = -1f64;
    let mut best_t = 127u8;

    for (t, &h) in hist.iter().enumerate() {
        w_b += h as f64;
        if w_b < 1.0 {
            continue;
        }
        let w_f = total - w_b;
        if w_f < 1.0 {
            break;
        }

        sum_b += (t as f64) * (h as f64);
        let m_b = sum_b / w_b;
        let m_f = (sum_total - sum_b) / w_f;

        let var_between = w_b * w_f * (m_b - m_f) * (m_b - m_f);
        if var_between > best_var {
            best_var = var_between;
            best_t = t as u8;
        }
    }

    best_t
}

/// Value at a given percentile (0.0..=1.0) of the sample distribution.
pub(crate) fn percentile_threshold(samples: &[u8], percentile: f32) -> u8 {
    if samples.is_empty() {
        return 127;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let idx = ((percentile * sorted.len() as f32) as usize).min(sorted.len() - 1);
    sorted[idx]
}

/// Threshold derived from the border/inner intensity split.
///
/// Uses the known marker structure: the outer ring should be black. Returns
/// `(median_border + median_inner) / 2`, or `None` if the structure is degenerate.
pub(crate) fn border_guided_threshold(samples: &[u8], cells: usize, border: usize) -> Option<u8> {
    if border == 0 || cells == 0 || samples.len() != cells * cells {
        return None;
    }
    let mut border_vals: Vec<u8> = Vec::new();
    let mut inner_vals: Vec<u8> = Vec::new();
    for cy in 0..cells {
        for cx in 0..cells {
            let v = samples[cy * cells + cx];
            let is_border =
                cx < border || cy < border || cx + border >= cells || cy + border >= cells;
            if is_border {
                border_vals.push(v);
            } else {
                inner_vals.push(v);
            }
        }
    }
    if border_vals.is_empty() || inner_vals.is_empty() {
        return None;
    }
    let median_border = percentile_threshold(&border_vals, 0.5);
    let median_inner = percentile_threshold(&inner_vals, 0.5);
    Some(((median_border as u16 + median_inner as u16) / 2) as u8)
}

/// Deduplicated set of threshold candidates for multi-threshold decoding.
///
/// Returns Otsu, Otsu±10, Otsu±15, percentile(0.35), percentile(0.45), and
/// an optional border-guided threshold — all sorted and deduplicated.
pub(crate) fn compute_threshold_candidates(
    otsu: u8,
    samples: &[u8],
    cells: usize,
    border: usize,
) -> Vec<u8> {
    let mut candidates = Vec::with_capacity(8);
    candidates.push(otsu);
    candidates.push(otsu.saturating_add(10));
    candidates.push(otsu.saturating_sub(10));
    candidates.push(otsu.saturating_add(15));
    candidates.push(otsu.saturating_sub(15));
    candidates.push(percentile_threshold(samples, 0.35));
    candidates.push(percentile_threshold(samples, 0.45));
    if let Some(bg) = border_guided_threshold(samples, cells, border) {
        candidates.push(bg);
    }
    candidates.sort_unstable();
    candidates.dedup();
    candidates
}
