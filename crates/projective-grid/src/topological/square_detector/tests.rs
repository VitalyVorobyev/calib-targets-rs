use super::solution::axes_pass_cluster_gate;
use super::*;
use crate::cluster::angular_dist_pi;
use crate::feature::{LocalAxis, PointFeature};
use crate::result::RejectionReason;
use crate::topological::axis::AxisCache;

fn axis_aligned_features(rows: i32, cols: i32, s: f32) -> Vec<OrientedFeature<2>> {
    let origin = 50.0_f32;
    let mut out = Vec::with_capacity((rows * cols) as usize);
    let mut idx = 0_usize;
    for j in 0..rows {
        for i in 0..cols {
            let x = (i as f32) * s + origin;
            let y = (j as f32) * s + origin;
            let point = PointFeature::new(idx, Point2::new(x, y));
            let axes = [
                LocalAxis::new(0.0_f32, Some(0.05)),
                LocalAxis::new(std::f32::consts::FRAC_PI_2, Some(0.05)),
            ];
            out.push(OrientedFeature::new(point, axes));
            idx += 1;
        }
    }
    out
}

#[test]
fn default_params_match_regression_values() {
    let p = TopologicalParams::default();
    assert!((p.axis_align_tol_rad - 15.0_f32.to_radians()).abs() < 1e-5);
    assert!((p.max_axis_sigma_rad - 0.6).abs() < 1e-5);
    assert!((p.opposing_edge_ratio_max - 1.5).abs() < 1e-5);
    assert!((p.edge_length_min_rel - 0.4).abs() < 1e-5);
    assert!((p.edge_length_max_rel - 2.5).abs() < 1e-5);
    assert_eq!(p.min_corners_for_component, 4);
    assert_eq!(p.min_quads_per_component, 1);
    assert!(p.axis_cluster_centers.is_none());
    assert!((p.cluster_axis_tol_rad - 16.0_f32.to_radians()).abs() < 1e-5);
}

#[test]
fn clean_5x5_grid_is_fully_labelled() {
    let features = axis_aligned_features(5, 5, 20.0);
    let params = DetectionParams::default();
    let mut solutions = detect_square_oriented2_all(
        &features,
        None,
        &params,
        SquareAxisProvenance::FullyMeasured,
    )
    .unwrap();
    assert_eq!(solutions.len(), 1);
    let solution = solutions.remove(0);
    assert_eq!(solution.detection.grid().entries().len(), 25);
    let fit = solution.detection.fit();
    assert!(fit.residuals.max_px < 0.01, "{}", fit.residuals.max_px);
}

/// Clutter inside every cell — a feature with confident axes that no
/// lattice edge passes through — splits each cell's triangle pair into a
/// fan, so no cell would pair into a quad. Pruning features without a
/// Grid edge before the cell test recovers the whole grid, and the
/// clutter itself is never labelled.
#[test]
fn clutter_inside_every_cell_costs_no_cell() {
    let (rows, cols, s) = (6, 6, 20.0_f32);
    let mut features = axis_aligned_features(rows, cols, s);
    let lattice = features.len();
    let origin = 50.0_f32;
    for j in 0..rows - 1 {
        for i in 0..cols - 1 {
            // Off-centre, with axes 30° off the lattice: every Delaunay
            // edge to a cell corner or to the neighbouring cell's clutter
            // is more than the 15° alignment tolerance off an axis at
            // one end at least.
            let x = origin + (i as f32 + 0.4) * s;
            let y = origin + (j as f32 + 0.35) * s;
            let point = PointFeature::new(features.len(), Point2::new(x, y));
            let axes = [
                LocalAxis::new(30.0_f32.to_radians(), Some(0.05)),
                LocalAxis::new(120.0_f32.to_radians(), Some(0.05)),
            ];
            features.push(OrientedFeature::new(point, axes));
        }
    }

    let solutions = detect_square_oriented2_all(
        &features,
        None,
        &DetectionParams::default(),
        SquareAxisProvenance::FullyMeasured,
    )
    .unwrap();
    assert_eq!(solutions.len(), 1);
    let entries = solutions[0].detection.grid().entries();
    assert_eq!(entries.len(), (rows * cols) as usize);
    assert!(
        entries.iter().all(|entry| entry.source_index < lattice),
        "a clutter feature was labelled"
    );
}

#[test]
fn fewer_than_three_features_errors() {
    let features = axis_aligned_features(1, 2, 20.0);
    let params = DetectionParams::default();
    let err = detect_square_oriented2_all(
        &features,
        None,
        &params,
        SquareAxisProvenance::FullyMeasured,
    )
    .unwrap_err();
    assert_eq!(err, GridError::InsufficientEvidence);
}

#[test]
fn cluster_gate_drops_off_axis_features() {
    // 5×5 axis-aligned grid (axes at 0°, 90°) + 4 noise features
    // whose axes both sit near 45°. With the cluster gate centered
    // at [0, π/2] and a 16° tolerance, the noise features must be
    // dropped pre-Delaunay; with the gate disabled they survive.
    let mut features = axis_aligned_features(5, 5, 20.0);
    let extra: [(f32, f32); 4] = [(40.0, 40.0), (180.0, 40.0), (40.0, 180.0), (180.0, 180.0)];
    let next = features.len();
    for (i, &(x, y)) in extra.iter().enumerate() {
        let point = PointFeature::new(next + i, Point2::new(x, y));
        let off_axis = std::f32::consts::FRAC_PI_4;
        let axes = [
            LocalAxis::new(off_axis, Some(0.05)),
            LocalAxis::new(off_axis + std::f32::consts::FRAC_PI_2, Some(0.05)),
        ];
        features.push(OrientedFeature::new(point, axes));
    }

    let tuning = crate::detect::DetectionTuning::default().with_topological(
        TopologicalParams::default().with_axis_cluster_centers([0.0, std::f32::consts::FRAC_PI_2]),
    );
    let params_on = DetectionParams::default().with_advanced(tuning);
    let mut sol_on = detect_square_oriented2_all(
        &features,
        None,
        &params_on,
        SquareAxisProvenance::FullyMeasured,
    )
    .unwrap();
    assert_eq!(sol_on.len(), 1);
    let primary = sol_on.remove(0);
    assert_eq!(
        primary.detection.grid().entries().len(),
        25,
        "gate must keep the 5×5"
    );

    let params_off = DetectionParams::default();
    let mut sol_off = detect_square_oriented2_all(
        &features,
        None,
        &params_off,
        SquareAxisProvenance::FullyMeasured,
    )
    .unwrap();
    assert_eq!(sol_off.len(), 1);
    let primary_off = sol_off.remove(0);
    assert_eq!(primary_off.detection.grid().entries().len(), 25);
    let noise_ids: std::collections::HashSet<usize> = (next..next + 4).collect();
    for r in &primary.rejected {
        if noise_ids.contains(&r.source_index) {
            assert_eq!(r.reason, RejectionReason::Unlabelled);
        }
    }
}

#[test]
fn axes_pass_cluster_gate_with_no_centers_is_identity() {
    let cache = AxisCache {
        angle_rad: [std::f32::consts::FRAC_PI_4, std::f32::consts::FRAC_PI_4],
        informative: [true, true],
    };
    let axes = [
        LocalAxis::new(std::f32::consts::FRAC_PI_4, Some(0.05_f32)),
        LocalAxis::new(std::f32::consts::FRAC_PI_4, Some(0.05_f32)),
    ];
    let params_off = TopologicalParams::default();
    assert!(axes_pass_cluster_gate(&axes, &cache, &params_off));
    let params_on = TopologicalParams::default()
        .with_axis_cluster_centers([0.0_f32, std::f32::consts::FRAC_PI_2]);
    assert!(!axes_pass_cluster_gate(&axes, &cache, &params_on));
}

#[test]
fn angular_dist_pi_is_undirected() {
    let pi = std::f32::consts::PI;
    let d_zero = angular_dist_pi(0.0, pi);
    assert!(d_zero < 1e-5, "{d_zero}");
    let d_perp = angular_dist_pi(0.0, std::f32::consts::FRAC_PI_2);
    assert!((d_perp - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
    let d_signed = angular_dist_pi(-0.1, std::f32::consts::PI + 0.1);
    assert!((d_signed - 0.2).abs() < 1e-4, "{d_signed}");
    let d_seam = angular_dist_pi(std::f32::consts::PI - 0.05, 0.05);
    assert!((d_seam - 0.1).abs() < 1e-5, "{d_seam}");
}
