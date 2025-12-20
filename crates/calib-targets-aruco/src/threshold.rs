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
