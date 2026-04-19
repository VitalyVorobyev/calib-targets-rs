//! Global cell-size estimation for the v2 detector.
//!
//! Computes the dominant cell size from **cross-cluster** nearest-
//! neighbor distances. Adjacent chessboard corners always flip
//! cluster (axes-slot swap), so filtering pair candidates to
//! cross-cluster only naturally rejects same-cluster pairs
//! (e.g., marker-interior corners that pass clustering by accident
//! but sit closer to each other than to real board corners).

use crate::corner::{ClusterLabel, CornerAug, CornerStage};
use crate::params::DetectorParams;
use kiddo::{KdTree, SquaredEuclidean};

/// Estimate the global cell size from the clustered corner set.
///
/// Only corners in stage `Clustered` or later contribute (per spec
/// §5.4). Returns `None` when fewer than two positions are
/// available.
///
/// The optional `cell_size_hint` in [`DetectorParams`] is consulted
/// and, if close enough, returned directly — this lets dataset-
/// specific callers lock in a known step.
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(level = "debug", skip_all, fields(num_corners = corners.len()))
)]
pub fn estimate_cell_size(corners: &[CornerAug], params: &DetectorParams) -> Option<f32> {
    let mut canonical_idx: Vec<usize> = Vec::new();
    let mut swapped_idx: Vec<usize> = Vec::new();
    for (i, c) in corners.iter().enumerate() {
        let label_opt = match c.stage {
            CornerStage::Clustered { label } => Some(label),
            CornerStage::Labeled { .. } => c.label,
            _ => None,
        };
        match label_opt {
            Some(ClusterLabel::Canonical) => canonical_idx.push(i),
            Some(ClusterLabel::Swapped) => swapped_idx.push(i),
            None => {}
        }
    }
    if canonical_idx.is_empty() || swapped_idx.is_empty() {
        return params.cell_size_hint;
    }

    // KD-tree over each cluster separately.
    let mut canon_tree: KdTree<f32, 2> = KdTree::new();
    for (slot, &idx) in canonical_idx.iter().enumerate() {
        let p = corners[idx].position;
        canon_tree.add(&[p.x, p.y], slot as u64);
    }
    let mut swap_tree: KdTree<f32, 2> = KdTree::new();
    for (slot, &idx) in swapped_idx.iter().enumerate() {
        let p = corners[idx].position;
        swap_tree.add(&[p.x, p.y], slot as u64);
    }

    // Collect cross-cluster nearest-neighbor distances.
    let mut dists: Vec<f32> = Vec::new();
    for &idx in &canonical_idx {
        let p = corners[idx].position;
        if let Some(nn) = swap_tree
            .nearest_n::<SquaredEuclidean>(&[p.x, p.y], 1)
            .into_iter()
            .next()
        {
            let d = nn.distance.sqrt();
            if d.is_finite() && d > 0.0 {
                dists.push(d);
            }
        }
    }
    for &idx in &swapped_idx {
        let p = corners[idx].position;
        if let Some(nn) = canon_tree
            .nearest_n::<SquaredEuclidean>(&[p.x, p.y], 1)
            .into_iter()
            .next()
        {
            let d = nn.distance.sqrt();
            if d.is_finite() && d > 0.0 {
                dists.push(d);
            }
        }
    }

    if dists.is_empty() {
        return params.cell_size_hint;
    }
    dists.sort_by(f32::total_cmp);

    // Find the densest mode via seed-+-mean-shift on three percentiles
    // (25, 50, 75). A bimodal distribution with marker-internal
    // false-cross-cluster pairs at the low end and true cell spacing
    // at the high end should yield the high-percentile mode.
    let mode = multimodal_mode(&dists);
    let est_value = mode.unwrap_or(dists[dists.len() / 2]);

    match params.cell_size_hint {
        Some(hint) => {
            if (est_value - hint).abs() / hint <= 0.3 {
                Some(hint)
            } else {
                log::warn!(
                    "cell-size hint {hint:.2} disagrees with cross-cluster estimate {est_value:.2}; using estimate"
                );
                Some(est_value)
            }
        }
        None => Some(est_value),
    }
}

/// Mean-shift over a sorted vector of scalar samples, seeded from
/// three percentiles. Picks the densest mode. Returns `None` when
/// fewer than 3 samples.
///
/// Used to pick a single "cell size" from a distribution that may
/// be bimodal (marker-internal distances at one mode, true cell
/// spacing at another).
fn multimodal_mode(sorted_dists: &[f32]) -> Option<f32> {
    let n = sorted_dists.len();
    if n < 3 {
        return None;
    }
    let pick = |q: f32| -> f32 {
        let idx = ((n as f32 - 1.0) * q).round() as usize;
        sorted_dists[idx.min(n - 1)]
    };
    let seeds = [pick(0.25), pick(0.5), pick(0.75)];
    let mut best: Option<(f32, usize)> = None;
    for &seed in &seeds {
        let mut center = seed;
        for _ in 0..20 {
            let bw = 0.15 * center;
            let lo = center - bw;
            let hi = center + bw;
            let mut sum = 0.0_f32;
            let mut count = 0_usize;
            for &d in sorted_dists {
                if d >= lo && d <= hi {
                    sum += d;
                    count += 1;
                }
            }
            if count == 0 {
                break;
            }
            let new_center = sum / count as f32;
            if (new_center - center).abs() < 1e-3 * bw {
                center = new_center;
                break;
            }
            center = new_center;
        }
        // Density at convergence.
        let bw = 0.15 * center;
        let lo = center - bw;
        let hi = center + bw;
        let support = sorted_dists.iter().filter(|&&d| d >= lo && d <= hi).count();
        if support == 0 {
            continue;
        }
        if best.map(|b| support > b.1).unwrap_or(true) {
            best = Some((center, support));
        }
    }
    best.map(|(c, _)| c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use calib_targets_core::{AxisEstimate, Corner};
    use nalgebra::Point2;

    fn make_corner(idx: usize, x: f32, y: f32, label: ClusterLabel) -> CornerAug {
        let c = Corner {
            position: Point2::new(x, y),
            orientation_cluster: None,
            axes: [
                AxisEstimate {
                    angle: 0.0,
                    sigma: 0.01,
                },
                AxisEstimate {
                    angle: std::f32::consts::FRAC_PI_2,
                    sigma: 0.01,
                },
            ],
            contrast: 10.0,
            fit_rms: 1.0,
            strength: 1.0,
        };
        let mut aug = CornerAug::from_corner(idx, &c);
        aug.stage = CornerStage::Clustered { label };
        aug.label = Some(label);
        aug
    }

    #[test]
    fn recovers_cell_size_on_regular_grid() {
        let mut corners = Vec::new();
        let mut k = 0;
        for j in 0..5 {
            for i in 0..5 {
                let label = if (i + j) % 2 == 0 {
                    ClusterLabel::Canonical
                } else {
                    ClusterLabel::Swapped
                };
                corners.push(make_corner(k, i as f32 * 20.0, j as f32 * 20.0, label));
                k += 1;
            }
        }
        let params = DetectorParams::default();
        let s = estimate_cell_size(&corners, &params).expect("cell size");
        assert!((s - 20.0).abs() < 1.0, "got {s}");
    }

    #[test]
    fn hint_close_to_estimate_is_returned() {
        let mut corners = Vec::new();
        for i in 0..10_i32 {
            let label = if i % 2 == 0 {
                ClusterLabel::Canonical
            } else {
                ClusterLabel::Swapped
            };
            corners.push(make_corner(i as usize, i as f32 * 20.0, 0.0, label));
        }
        let params = DetectorParams {
            cell_size_hint: Some(20.5),
            ..DetectorParams::default()
        };
        let s = estimate_cell_size(&corners, &params).expect("cell size");
        assert!((s - 20.5).abs() < 1e-3);
    }
}
