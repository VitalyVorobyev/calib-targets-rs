//! Synthetic-grid integration tests for the topological pipeline.

use std::f32::consts::FRAC_PI_2;

use nalgebra::Point2;

use super::{build_grid_topological, AxisHint, TopologicalParams};

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
