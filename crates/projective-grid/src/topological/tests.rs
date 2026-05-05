//! Synthetic-grid integration tests for the topological pipeline.

use std::f32::consts::FRAC_PI_2;

use nalgebra::Point2;

use super::{
    build_grid_topological, build_grid_topological_trace, AxisClusterCenters, AxisHint,
    TopologicalParams,
};

fn axes_axis_aligned() -> [AxisHint; 2] {
    [
        AxisHint {
            angle: 0.0,
            sigma: 0.05,
        },
        AxisHint {
            angle: FRAC_PI_2,
            sigma: 0.05,
        },
    ]
}

fn axes_no_info() -> [AxisHint; 2] {
    [AxisHint::default(), AxisHint::default()]
}

fn build_axis_aligned_grid(
    rows: usize,
    cols: usize,
    step: f32,
) -> (Vec<Point2<f32>>, Vec<[AxisHint; 2]>) {
    let mut pts = Vec::new();
    let mut axs = Vec::new();
    for j in 0..rows {
        for i in 0..cols {
            pts.push(Point2::new(i as f32 * step, j as f32 * step));
            axs.push(axes_axis_aligned());
        }
    }
    (pts, axs)
}

#[test]
fn default_tolerances_are_regression_values() {
    let params = TopologicalParams::default();
    // 15°/15° — paired with the pre-Delaunay cluster gate. Together the
    // two are below π/4 = 45° so an edge can never satisfy both Grid and
    // Diagonal simultaneously: classification is unambiguous by
    // construction, no margin-gate disambiguation is needed.
    assert!((params.axis_align_tol_rad - 15.0_f32.to_radians()).abs() < 1e-6);
    assert!((params.diagonal_angle_tol_rad - 15.0_f32.to_radians()).abs() < 1e-6);

    let json = serde_json::to_string(&params).unwrap();
    let restored: TopologicalParams = serde_json::from_str(&json).unwrap();
    assert!((restored.axis_align_tol_rad - params.axis_align_tol_rad).abs() < 1e-6);
    assert!((restored.diagonal_angle_tol_rad - params.diagonal_angle_tol_rad).abs() < 1e-6);
}

#[test]
fn clean_5x5_grid_produces_single_component() {
    let (pts, axs) = build_axis_aligned_grid(5, 5, 10.0);
    let g = build_grid_topological(&pts, &axs, &TopologicalParams::default()).unwrap();
    assert_eq!(g.components.len(), 1, "expected one connected component");
    let c = &g.components[0];
    assert_eq!(c.labelled.len(), 25, "all 25 corners labelled");
    // Expect 5x5 bounding box starting at (0, 0).
    let max_i = c.labelled.keys().map(|(i, _)| *i).max().unwrap();
    let max_j = c.labelled.keys().map(|(_, j)| *j).max().unwrap();
    let min_i = c.labelled.keys().map(|(i, _)| *i).min().unwrap();
    let min_j = c.labelled.keys().map(|(_, j)| *j).min().unwrap();
    assert_eq!((min_i, min_j), (0, 0), "bbox rebased to (0, 0)");
    assert_eq!((max_i, max_j), (4, 4), "5x5 grid spans (0..4, 0..4)");
}

#[test]
fn three_corners_of_one_cell_cannot_seed_a_topological_component() {
    // The topological core walks a quad mesh. A single incomplete
    // chessboard cell has only one Delaunay triangle and therefore no
    // buddy triangle across a diagonal to merge into a quad.
    let pts = vec![
        Point2::new(0.0, 0.0),
        Point2::new(10.0, 0.0),
        Point2::new(0.0, 10.0),
    ];
    let axs = vec![axes_axis_aligned(); pts.len()];
    let trace = build_grid_topological_trace(&pts, &axs, &TopologicalParams::default()).unwrap();
    assert_eq!(trace.diagnostics.triangles, 1);
    assert_eq!(trace.diagnostics.quads_merged, 0);
    assert_eq!(trace.components.len(), 0);
}

#[test]
fn grid_with_extra_spurious_corner_is_rejected() {
    // 4x4 grid + one spurious corner well off to the side with random axes.
    let (mut pts, mut axs) = build_axis_aligned_grid(4, 4, 10.0);
    pts.push(Point2::new(100.0, 100.0));
    axs.push([
        AxisHint {
            angle: 1.1, // ≈ 63°, not aligned with the grid axes
            sigma: 0.05,
        },
        AxisHint {
            angle: 1.1 + FRAC_PI_2,
            sigma: 0.05,
        },
    ]);
    let g = build_grid_topological(&pts, &axs, &TopologicalParams::default()).unwrap();
    assert_eq!(g.components.len(), 1);
    let c = &g.components[0];
    // The 16 grid corners must be labelled; the spurious corner must not.
    assert_eq!(c.labelled.len(), 16);
    let labelled_idxs: std::collections::HashSet<usize> = c.labelled.values().copied().collect();
    assert!(
        !labelled_idxs.contains(&16),
        "spurious corner must be excluded"
    );
}

#[test]
fn corners_with_no_axis_info_are_skipped() {
    let (mut pts, mut axs) = build_axis_aligned_grid(4, 4, 10.0);
    // Inject one well-placed but uninformative corner inside the grid bbox.
    pts.push(Point2::new(15.0, 15.0));
    axs.push(axes_no_info());
    let g = build_grid_topological(&pts, &axs, &TopologicalParams::default()).unwrap();
    // Should still recover the 4×4 grid; the no-info corner cannot
    // contribute to any classified edge.
    assert_eq!(g.components.len(), 1);
    assert_eq!(g.components[0].labelled.len(), 16);
}

#[test]
fn length_mismatch_is_an_error() {
    let pts = vec![Point2::new(0.0, 0.0); 4];
    let axs = vec![axes_axis_aligned(); 3];
    assert!(matches!(
        build_grid_topological(&pts, &axs, &TopologicalParams::default()),
        Err(super::TopologicalError::LengthMismatch { .. })
    ));
}

#[test]
fn fewer_than_three_usable_corners_is_an_error() {
    let pts = vec![Point2::new(0.0, 0.0), Point2::new(1.0, 0.0)];
    let axs = vec![axes_axis_aligned(); 2];
    assert!(matches!(
        build_grid_topological(&pts, &axs, &TopologicalParams::default()),
        Err(super::TopologicalError::NotEnoughCorners { .. })
    ));
}

#[test]
fn rotated_grid_still_recovered() {
    // 5x5 grid rotated by 30°; each corner's axes rotate by the same amount.
    let theta: f32 = 30.0_f32.to_radians();
    let (cos_t, sin_t) = (theta.cos(), theta.sin());
    let mut pts = Vec::new();
    let mut axs = Vec::new();
    for j in 0..5 {
        for i in 0..5 {
            let x = i as f32 * 10.0;
            let y = j as f32 * 10.0;
            pts.push(Point2::new(cos_t * x - sin_t * y, sin_t * x + cos_t * y));
            axs.push([
                AxisHint {
                    angle: theta,
                    sigma: 0.05,
                },
                AxisHint {
                    angle: theta + FRAC_PI_2,
                    sigma: 0.05,
                },
            ]);
        }
    }
    let g = build_grid_topological(&pts, &axs, &TopologicalParams::default()).unwrap();
    assert_eq!(g.components.len(), 1);
    assert_eq!(g.components[0].labelled.len(), 25);
}

#[test]
fn trace_matches_production_grid_and_serializes() {
    let (pts, axs) = build_axis_aligned_grid(5, 5, 10.0);
    let params = TopologicalParams::default();
    let grid = build_grid_topological(&pts, &axs, &params).unwrap();
    let trace = build_grid_topological_trace(&pts, &axs, &params).unwrap();

    assert_eq!(trace.diagnostics.corners_in, grid.diagnostics.corners_in);
    assert_eq!(
        trace.diagnostics.corners_used,
        grid.diagnostics.corners_used
    );
    assert_eq!(trace.diagnostics.triangles, grid.diagnostics.triangles);
    assert_eq!(
        trace.diagnostics.quads_merged,
        grid.diagnostics.quads_merged
    );
    assert_eq!(trace.diagnostics.quads_kept, grid.diagnostics.quads_kept);
    assert_eq!(trace.diagnostics.components, grid.diagnostics.components);
    assert_eq!(trace.components.len(), grid.components.len());

    let traced_labels: std::collections::HashSet<_> = trace.components[0]
        .labels
        .iter()
        .map(|l| ((l.i, l.j), l.corner_idx))
        .collect();
    let grid_labels: std::collections::HashSet<_> = grid.components[0]
        .labelled
        .iter()
        .map(|(&ij, &idx)| (ij, idx))
        .collect();
    assert_eq!(traced_labels, grid_labels);

    let json = serde_json::to_value(&trace).unwrap();
    assert_eq!(json["diagnostics"]["corners_in"], 25);
    assert!(!json["triangles"].as_array().unwrap().is_empty());
    assert!(json["triangles"][0]["edge_metrics"].is_array());
    assert!(!json["quads"].as_array().unwrap().is_empty());
}

#[test]
fn cluster_centers_default_to_none_and_legacy_behavior_holds() {
    let p = TopologicalParams::default();
    assert!(p.axis_cluster_centers.is_none());
    assert!((p.cluster_axis_tol_rad - 16.0_f32.to_radians()).abs() < 1e-6);
}

#[test]
fn cluster_gate_drops_off_axis_noiser_when_centers_supplied() {
    // 5×5 grid (axes ≈ 0°/90°) plus 4 spurious corners with axes at 30°.
    let (mut pts, mut axs) = build_axis_aligned_grid(5, 5, 10.0);
    let off_axis = [
        AxisHint {
            angle: 30.0_f32.to_radians(),
            sigma: 0.05,
        },
        AxisHint {
            angle: 30.0_f32.to_radians() + FRAC_PI_2,
            sigma: 0.05,
        },
    ];
    pts.push(Point2::new(60.0, 5.0));
    axs.push(off_axis);
    pts.push(Point2::new(-10.0, 25.0));
    axs.push(off_axis);
    pts.push(Point2::new(45.0, 60.0));
    axs.push(off_axis);
    pts.push(Point2::new(15.0, -8.0));
    axs.push(off_axis);

    // Without centers: legacy gate (sigma only). Off-axis noisers enter
    // Delaunay; whether they label depends on the angle test, but they
    // pollute the diagnostic count.
    let no_gate = build_grid_topological(&pts, &axs, &TopologicalParams::default()).unwrap();
    assert_eq!(no_gate.diagnostics.corners_used, 29);

    // With centers at 0°/90°: noisers fail the cluster gate at the
    // default tol — see `cluster_centers_default_to_none_and_legacy_behavior_holds`.
    let params = TopologicalParams {
        axis_cluster_centers: Some(AxisClusterCenters::new(0.0, FRAC_PI_2)),
        ..TopologicalParams::default()
    };
    let gated = build_grid_topological(&pts, &axs, &params).unwrap();
    assert_eq!(
        gated.diagnostics.corners_used, 25,
        "cluster gate must reject the four 30° noisers"
    );
    // The 5×5 grid should still recover.
    assert_eq!(gated.components.len(), 1);
    assert_eq!(gated.components[0].labelled.len(), 25);
}

#[test]
fn cluster_gate_widens_with_tolerance() {
    // The 30° noiser that was filtered out at 12° tol should pass at
    // 35° tol — sanity check that `cluster_axis_tol_rad` is honoured.
    let (mut pts, mut axs) = build_axis_aligned_grid(5, 5, 10.0);
    pts.push(Point2::new(60.0, 5.0));
    axs.push([
        AxisHint {
            angle: 30.0_f32.to_radians(),
            sigma: 0.05,
        },
        AxisHint {
            angle: 30.0_f32.to_radians() + FRAC_PI_2,
            sigma: 0.05,
        },
    ]);

    let strict_params = TopologicalParams {
        axis_cluster_centers: Some(AxisClusterCenters::new(0.0, FRAC_PI_2)),
        cluster_axis_tol_rad: 12.0_f32.to_radians(),
        ..TopologicalParams::default()
    };
    let strict = build_grid_topological(&pts, &axs, &strict_params).unwrap();
    assert_eq!(strict.diagnostics.corners_used, 25);

    let lax_params = TopologicalParams {
        cluster_axis_tol_rad: 35.0_f32.to_radians(),
        ..strict_params
    };
    let lax = build_grid_topological(&pts, &axs, &lax_params).unwrap();
    assert_eq!(lax.diagnostics.corners_used, 26);
}

#[test]
fn axis_cluster_centers_are_ordered_and_wrapped() {
    let c = AxisClusterCenters::new(2.5, 0.5);
    assert!(c.theta0 < c.theta1);
    assert!((c.theta0 - 0.5).abs() < 1e-6);
    assert!((c.theta1 - 2.5).abs() < 1e-6);

    let wrapped = AxisClusterCenters::new(std::f32::consts::PI + 0.1, 0.5);
    // π + 0.1 → 0.1 after wrap_pi to [0, π); ordered with 0.5 → (0.1, 0.5).
    assert!(wrapped.theta0 < wrapped.theta1);
    assert!(wrapped.theta0 < std::f32::consts::PI);
    assert!(wrapped.theta1 < std::f32::consts::PI);
}

#[test]
fn trace_edge_metrics_have_consistent_margins() {
    let (pts, axs) = build_axis_aligned_grid(4, 4, 10.0);
    let params = TopologicalParams::default();
    let trace = build_grid_topological_trace(&pts, &axs, &params).unwrap();
    let metric = trace
        .triangles
        .iter()
        .flat_map(|t| t.edge_metrics.iter())
        .find(|m| m.grid_distance_rad.is_some() && m.diagonal_distance_rad.is_some())
        .expect("at least one finite edge metric");
    let grid_distance = metric.grid_distance_rad.unwrap();
    let diagonal_distance = metric.diagonal_distance_rad.unwrap();
    assert!(
        (metric.grid_margin_rad.unwrap() - (params.axis_align_tol_rad - grid_distance)).abs()
            < 1e-6
    );
    assert!(
        (metric.diagonal_margin_rad.unwrap() - (params.diagonal_angle_tol_rad - diagonal_distance))
            .abs()
            < 1e-6
    );
}
