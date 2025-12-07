//! Plain chessboard detector built on top of `calib-targets-core`.
//!
//! New algorithm (more robust to non-square grids and avoids graph fragmentation):
//! 1. Filter strong ChESS corners.
//! 2. Estimate global grid axes (u, v) from corner orientations.
//! 3. Estimate axis spacings su, sv from nearest-neighbor distances.
//! 4. Project all corners to (u, v) coordinates.
//! 5. 1D-cluster u and v into grid lines using su, sv.
//! 6. Assign each corner to nearest (u-line, v-line) → integer (i, j).
//! 7. Compute grid size, completeness, check against expected_rows/cols.

use std::cmp::Ordering;

use calib_targets_core::{
    estimate_grid_axes_from_orientations, Corner, GridCoords, GridSearchParams, LabeledCorner,
    TargetDetection, TargetKind,
};
use log::info;
use nalgebra::Vector2;

/// Parameters specific to the chessboard detector.
#[derive(Clone, Debug)]
pub struct ChessboardParams {
    pub grid_search: GridSearchParams,

    /// Expected number of *inner* corners in vertical direction (rows).
    /// If `None`, detector does not enforce a specific size.
    pub expected_rows: Option<u32>,

    /// Expected number of *inner* corners in horizontal direction (cols).
    /// If `None`, detector does not enforce a specific size.
    pub expected_cols: Option<u32>,

    /// Minimal completeness ratio (#detected corners / grid cells)
    /// when expected_rows/cols are provided.
    pub completeness_threshold: f32,

    /// Minimal number of corners per grid line (for u/v clustering).
    pub min_points_per_line: usize,
}

impl Default for ChessboardParams {
    fn default() -> Self {
        Self {
            grid_search: GridSearchParams::default(),
            expected_rows: None,
            expected_cols: None,
            completeness_threshold: 0.7,
            min_points_per_line: 2,
        }
    }
}

/// Simple chessboard detector using ChESS orientations + 1D clustering.
pub struct ChessboardDetector {
    pub params: ChessboardParams,
}

impl ChessboardDetector {
    pub fn new(params: ChessboardParams) -> Self {
        Self { params }
    }

    /// Main entry point: find chessboard(s) in a cloud of ChESS corners.
    ///
    /// This function expects corners already computed by your ChESS crate.
    /// For now it returns at most one detection (the best grid).
    pub fn detect_from_corners(&self, corners: &[Corner]) -> Vec<TargetDetection> {
        // 1. Filter by strength.
        let strong: Vec<Corner> = corners
            .iter()
            .cloned()
            .filter(|c| c.strength >= self.params.grid_search.min_strength)
            .collect();

        info!("found {} raw ChESS corners after strength filter", strong.len());

        if strong.len() < self.params.grid_search.min_corners {
            return Vec::new();
        }

        // 2. Estimate grid axes from orientations.
        let Some((u_axis_unit, v_axis_unit)) = estimate_grid_axes_from_orientations(&strong) else {
            info!("failed to estimate grid axes from orientations");
            return Vec::new();
        };
        let u_axis = u_axis_unit.into_inner();
        let v_axis = v_axis_unit.into_inner();

        // 3. Estimate neighbor spacings su, sv.
        let Some((su, sv)) = estimate_spacings(&strong, &u_axis, &v_axis) else {
            info!("failed to estimate grid spacings");
            return Vec::new();
        };
        info!("estimated spacings: su={:.2}, sv={:.2}", su, sv);

        // 4. Project corners to (u, v).
        let n = strong.len();
        let mut u_vals = Vec::with_capacity(n);
        let mut v_vals = Vec::with_capacity(n);
        for c in &strong {
            let p = c.as_vec2();
            u_vals.push(p.dot(&u_axis));
            v_vals.push(p.dot(&v_axis));
        }

        // 5. Cluster u and v into grid lines.
        let u_clust = cluster_1d(&u_vals, su, self.params.min_points_per_line);
        let v_clust = cluster_1d(&v_vals, sv, self.params.min_points_per_line);

        let nu = u_clust.num_lines();
        let nv = v_clust.num_lines();
        info!(
            "clustered into {} u-lines and {} v-lines (before filtering outliers)",
            nu, nv
        );

        if nu < 2 || nv < 2 {
            info!("not enough grid lines (u={}, v={})", nu, nv);
            return Vec::new();
        }

        // 6. Assign each corner to nearest (u-line, v-line).
        let mut coords = Vec::with_capacity(n);
        for i in 0..n {
            let iu = match u_clust.labels[i] {
                Some(k) => k as i32,
                None => continue, // outlier in u
            };
            let iv = match v_clust.labels[i] {
                Some(k) => k as i32,
                None => continue, // outlier in v
            };
            coords.push((i, iu, iv));
        }

        if coords.len() < self.params.grid_search.min_corners {
            info!(
                "too few corners after line assignment: {} < {}",
                coords.len(),
                self.params.grid_search.min_corners
            );
            return Vec::new();
        }

        // 7. Compute grid size, completeness, and check against expectations.
        let width = nu as i32;
        let height = nv as i32;
        let total_cells = (width * height) as usize;
        let completeness = coords.len() as f32 / total_cells as f32;

        info!(
            "chessboard candidate: size {}x{}, corners {}, completeness {:.3}",
            width,
            height,
            coords.len(),
            completeness
        );

        if let (Some(rows), Some(cols)) = (self.params.expected_rows, self.params.expected_cols) {
            let rows_i = rows as i32;
            let cols_i = cols as i32;

            let matches_size =
                (width == cols_i && height == rows_i) || (width == rows_i && height == cols_i);

            if !matches_size {
                info!(
                    "candidate {}x{} rejected: does not match expected {}x{} (or swapped)",
                    width, height, cols, rows
                );
                return Vec::new();
            }

            if completeness < self.params.completeness_threshold {
                info!(
                    "candidate {}x{} rejected: completeness {:.3} < {}",
                    width, height, completeness, self.params.completeness_threshold
                );
                return Vec::new();
            }
        }

        // 8. Build labeled corners.
        let mut labeled = Vec::with_capacity(coords.len());
        for (idx, iu, iv) in coords {
            let c = &strong[idx];
            labeled.push(LabeledCorner {
                position: c.position,
                grid: Some(GridCoords { i: iu, j: iv }),
                id: None,
                confidence: 1.0,
            });
        }

        vec![TargetDetection {
            kind: TargetKind::Chessboard,
            corners: labeled,
        }]
    }
}

/// Result of 1D line clustering.
struct LineClustering {
    /// Centers of kept clusters, in ascending coordinate order.
    centers: Vec<f32>,
    /// For each point, index of its cluster (0..centers.len()-1) or None if discarded.
    labels: Vec<Option<usize>>,
}

impl LineClustering {
    fn num_lines(&self) -> usize {
        self.centers.len()
    }
}

/// Estimate base spacing and axis spacings su, sv from ChESS corners.
fn estimate_spacings(
    corners: &[Corner],
    u_axis: &Vector2<f32>,
    v_axis: &Vector2<f32>,
) -> Option<(f32, f32)> {
    let n = corners.len();
    if n < 2 {
        return None;
    }

    // 1. Base spacing from nearest neighbor distances (Euclidean).
    let mut nn_dists = Vec::with_capacity(n);
    for i in 0..n {
        let pi = corners[i].as_vec2();
        let mut min_d2 = f32::INFINITY;
        for j in 0..n {
            if i == j {
                continue;
            }
            let pj = corners[j].as_vec2();
            let d2 = (pj - pi).norm_squared();
            if d2 < min_d2 {
                min_d2 = d2;
            }
        }
        if min_d2.is_finite() && min_d2 > 0.0 {
            nn_dists.push(min_d2.sqrt());
        }
    }

    if nn_dists.is_empty() {
        return None;
    }

    let base_spacing = median(&mut nn_dists);
    if !base_spacing.is_finite() || base_spacing <= 0.0 {
        return None;
    }

    // 2. Collect axis-aligned spacing samples limited to a band around base_spacing.
    let min_d = 0.5 * base_spacing;
    let max_d = 2.0 * base_spacing;

    let cross_tol = 0.5f32; // |perp| <= cross_tol * |along| to consider "axis-aligned"
    let mut su_samples = Vec::new();
    let mut sv_samples = Vec::new();

    for i in 0..n {
        let pi = corners[i].as_vec2();
        for j in (i + 1)..n {
            let pj = corners[j].as_vec2();
            let d = pj - pi;
            let dist = d.norm();
            if dist < min_d || dist > max_d {
                continue;
            }

            let du = d.dot(u_axis);
            let dv = d.dot(v_axis);
            let adu = du.abs();
            let adv = dv.abs();

            if adu > adv && adv <= cross_tol * adu {
                su_samples.push(adu);
            } else if adv > adu && adu <= cross_tol * adv {
                sv_samples.push(adv);
            }
        }
    }

    let su = if su_samples.is_empty() {
        base_spacing
    } else {
        median(&mut su_samples)
    };

    let sv = if sv_samples.is_empty() {
        base_spacing
    } else {
        median(&mut sv_samples)
    };

    if su <= 0.0 || sv <= 0.0 {
        None
    } else {
        Some((su, sv))
    }
}

/// 1D clustering of coordinates into approximately equally-spaced lines.
fn cluster_1d(values: &[f32], spacing_est: f32, min_points_per_line: usize) -> LineClustering {
    let n = values.len();
    let mut labels_raw: Vec<Option<usize>> = vec![None; n];
    let mut centers_raw: Vec<f32> = Vec::new();
    let mut sizes_raw: Vec<usize> = Vec::new();

    if n == 0 || !spacing_est.is_finite() || spacing_est <= 0.0 {
        return LineClustering {
            centers: Vec::new(),
            labels: labels_raw,
        };
    }

    let mut idxs: Vec<usize> = (0..n).collect();
    idxs.sort_by(|&a, &b| {
        values[a]
            .partial_cmp(&values[b])
            .unwrap_or_else(|| a.cmp(&b))
    });

    let cluster_tol = spacing_est * 0.5;

    for idx in idxs {
        let val = values[idx];

        if centers_raw.is_empty() {
            centers_raw.push(val);
            sizes_raw.push(1);
            labels_raw[idx] = Some(0);
        } else {
            let k = centers_raw.len() - 1;
            let center = centers_raw[k];
            if (val - center).abs() <= cluster_tol {
                let new_size = sizes_raw[k] + 1;
                let new_center = (center * sizes_raw[k] as f32 + val) / new_size as f32;
                centers_raw[k] = new_center;
                sizes_raw[k] = new_size;
                labels_raw[idx] = Some(k);
            } else {
                centers_raw.push(val);
                sizes_raw.push(1);
                let new_k = centers_raw.len() - 1;
                labels_raw[idx] = Some(new_k);
            }
        }
    }

    // Remove clusters that are too small (likely outliers).
    let mut old_to_new: Vec<Option<usize>> = vec![None; centers_raw.len()];
    let mut centers = Vec::new();
    for (k, (&center, &size)) in centers_raw.iter().zip(&sizes_raw).enumerate() {
        if size >= min_points_per_line {
            let new_idx = centers.len();
            centers.push(center);
            old_to_new[k] = Some(new_idx);
        }
    }

    let mut labels = Vec::with_capacity(n);
    for lab in labels_raw.iter() {
        let mapped = match lab {
            Some(old_k) => old_to_new[*old_k],
            None => None,
        };
        labels.push(mapped);
    }

    LineClustering { centers, labels }
}

/// Compute median of a vec in-place.
fn median(v: &mut Vec<f32>) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| match (a.is_nan(), b.is_nan()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => a
            .partial_cmp(b)
            .unwrap_or_else(|| a.to_bits().cmp(&b.to_bits())),
    });

    let mid = v.len() / 2;
    if v.len() % 2 == 1 {
        v[mid]
    } else {
        0.5 * (v[mid - 1] + v[mid])
    }
}
