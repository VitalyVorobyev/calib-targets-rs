//! Stage 7 — post-growth validation.
//!
//! Pattern-agnostic logic lives in the
//! [`projective_grid::square::validate`](mod@projective_grid::square::validate)
//! module; this adapter maps the chessboard detector's internal
//! `CornerAug` + labelled-map representation to
//! [`projective_grid::square::validate::LabelledEntry`] and
//! forwards the call.
//!
//! See the hoisted module for the algorithm description (line
//! collinearity + local-H residual + attribution rules).

use crate::corner::CornerAug;
use crate::params::DetectorParams;
use projective_grid::square::validate as pg_validate;
use std::collections::HashMap;

pub use pg_validate::ValidationResult;

/// Run both validation passes and produce a blacklist.
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "debug",
        skip_all,
        fields(labelled = labelled.len(), cell_size = cell_size)
    )
)]
pub fn validate(
    corners: &[CornerAug],
    labelled: &HashMap<(i32, i32), usize>,
    cell_size: f32,
    params: &DetectorParams,
) -> ValidationResult {
    let entries: Vec<pg_validate::LabelledEntry> = labelled
        .iter()
        .map(|(&grid, &idx)| pg_validate::LabelledEntry {
            idx,
            pixel: corners[idx].position,
            grid,
        })
        .collect();
    let mut pg_params = pg_validate::ValidationParams::new(
        params.line_tol_rel,
        params.line_min_members,
        params.local_h_tol_rel,
    );
    if params.validate_step_aware {
        pg_params = pg_params.with_step_aware(params.validate_step_deviation_thresh_rel);
    }
    pg_validate::validate(&entries, cell_size, &pg_params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::cluster_axes;
    use crate::grow::grow_from_seed;
    use crate::seed::find_seed;
    use calib_targets_core::{AxisEstimate, Corner};
    use nalgebra::Point2;
    use std::collections::HashSet;

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

        let mid_idx = 3 * 7 + 3;
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
