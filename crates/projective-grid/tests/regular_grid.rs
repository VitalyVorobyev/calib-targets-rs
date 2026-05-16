//! End-to-end coverage for the zero-config `detect_regular_grid`
//! onboarding entry point.
//!
//! Unlike `square_policy_advanced.rs`, none of these tests write a
//! validator: a bare `&[Point2<f32>]` goes in, a labelled grid comes
//! out. They exercise clean, rotated, and perspective-warped grids,
//! missing interior points, spurious off-grid points, two disjoint
//! components, and the visual top-left canonicalisation.

use nalgebra::{Matrix3, Point2};

use projective_grid::{
    detect_regular_grid, ExtensionStrategy, RegularGridDetector, RegularGridError,
    RegularGridParams,
};

/// Build a perspective-warped `rows × cols` grid. Same warp family as
/// the advanced-policy smoke test and `benches/grow.rs`.
fn perspective_warped_grid(rows: i32, cols: i32, scale: f32) -> Vec<Point2<f32>> {
    let h = Matrix3::new(
        1.0_f32 * scale,
        0.10 * scale,
        50.0,
        0.05 * scale,
        1.0 * scale,
        50.0,
        2e-4,
        1e-4,
        1.0,
    );
    let mut out = Vec::with_capacity((rows * cols) as usize);
    for j in 0..rows {
        for i in 0..cols {
            let x = i as f32;
            let y = j as f32;
            let w = h[(2, 0)] * x + h[(2, 1)] * y + h[(2, 2)];
            let xp = (h[(0, 0)] * x + h[(0, 1)] * y + h[(0, 2)]) / w;
            let yp = (h[(1, 0)] * x + h[(1, 1)] * y + h[(1, 2)]) / w;
            out.push(Point2::new(xp, yp));
        }
    }
    out
}

fn axis_aligned_grid(rows: i32, cols: i32, s: f32, ox: f32, oy: f32) -> Vec<Point2<f32>> {
    let mut out = Vec::new();
    for j in 0..rows {
        for i in 0..cols {
            out.push(Point2::new(i as f32 * s + ox, j as f32 * s + oy));
        }
    }
    out
}

#[test]
fn clean_axis_aligned_grid_is_fully_labelled() {
    let pts = axis_aligned_grid(7, 7, 25.0, 60.0, 60.0);
    let grid = detect_regular_grid(&pts).expect("clean grid detects");
    assert_eq!(grid.points.len(), 49, "every corner must be labelled");

    // Bounding box rebased to (0, 0).
    let min_i = grid.points.iter().map(|p| p.grid.0).min().unwrap();
    let min_j = grid.points.iter().map(|p| p.grid.1).min().unwrap();
    assert_eq!((min_i, min_j), (0, 0));

    // source_index round-trips to the right pixel.
    for p in &grid.points {
        assert_eq!(p.position, pts[p.source_index]);
    }
}

#[test]
fn rotated_grid_is_recovered() {
    let theta = 37.0_f32.to_radians();
    let (c, s) = (theta.cos(), theta.sin());
    let mut pts = Vec::new();
    for j in 0..6 {
        for i in 0..6 {
            let (x, y) = (i as f32 * 22.0, j as f32 * 22.0);
            pts.push(Point2::new(x * c - y * s + 150.0, x * s + y * c + 150.0));
        }
    }
    let grid = detect_regular_grid(&pts).expect("rotated grid detects");
    assert!(
        grid.points.len() >= 30,
        "expected most of the rotated grid; got {}",
        grid.points.len()
    );
}

#[test]
fn perspective_warped_grid_is_recovered() {
    let pts = perspective_warped_grid(7, 7, 30.0);
    let grid = detect_regular_grid(&pts).expect("warped grid detects");
    assert!(
        grid.points.len() >= 40,
        "expected the bulk of the warped grid; got {}",
        grid.points.len()
    );
}

#[test]
fn missing_interior_points_do_not_break_detection() {
    // Drop four interior corners; the grid must still be recovered
    // around the holes.
    let full = axis_aligned_grid(7, 7, 25.0, 60.0, 60.0);
    let cols = 7;
    let drop: [(i32, i32); 4] = [(2, 2), (3, 3), (4, 2), (3, 4)];
    let pts: Vec<Point2<f32>> = full
        .iter()
        .enumerate()
        .filter(|(idx, _)| {
            let i = (*idx as i32) % cols;
            let j = (*idx as i32) / cols;
            !drop.contains(&(i, j))
        })
        .map(|(_, &p)| p)
        .collect();
    assert_eq!(pts.len(), 45);
    let grid = detect_regular_grid(&pts).expect("holey grid detects");
    // The hole-fill pass may recover some interior cells from
    // neighbours, but no point should be invented for a corner that
    // does not exist in the input.
    assert!(grid.points.len() <= 45);
    assert!(
        grid.points.len() >= 40,
        "expected most of the holey grid; got {}",
        grid.points.len()
    );
    for p in &grid.points {
        assert_eq!(p.position, pts[p.source_index]);
    }
}

#[test]
fn spurious_off_grid_points_are_pruned() {
    let mut pts = axis_aligned_grid(6, 6, 25.0, 80.0, 80.0);
    let on_grid = pts.len();
    // Five spurious points well off the lattice.
    pts.push(Point2::new(7.0, 9.0));
    pts.push(Point2::new(400.0, 11.0));
    pts.push(Point2::new(13.0, 380.0));
    pts.push(Point2::new(500.0, 500.0));
    pts.push(Point2::new(250.0, 12.0));

    let grid = detect_regular_grid(&pts).expect("grid with noise detects");
    // The labelled component must not include the spurious points.
    assert!(
        grid.points.len() <= on_grid,
        "spurious points must not be labelled: got {} (on-grid {})",
        grid.points.len(),
        on_grid
    );
    assert!(grid.points.len() >= 30, "got {}", grid.points.len());
}

#[test]
fn two_disjoint_components_are_returned_by_detect_all() {
    // Two clean grids far apart so the seed finder never bridges them.
    let mut pts = axis_aligned_grid(5, 5, 20.0, 100.0, 100.0);
    pts.extend(axis_aligned_grid(4, 4, 20.0, 700.0, 700.0));

    let detector = RegularGridDetector::default();
    let detections = detector.detect_all(&pts);
    assert_eq!(
        detections.len(),
        2,
        "expected two components, got {}",
        detections.len()
    );
    let total: usize = detections.iter().map(|d| d.points.len()).sum();
    assert_eq!(total, pts.len(), "every corner accounted for");

    // Each component's source indices are disjoint.
    let mut seen = std::collections::HashSet::new();
    for d in &detections {
        for p in &d.points {
            assert!(
                seen.insert(p.source_index),
                "index reused across components"
            );
        }
    }
}

#[test]
fn top_left_canonicalization_orients_axes() {
    // A grid laid out so the raw +i runs DOWN and +j runs LEFT in
    // pixel space. With canonicalisation on (the default), the
    // returned grid must have +i → right and +j → down.
    let mut pts = Vec::new();
    for j in 0..5 {
        for i in 0..5 {
            // grid +i → +y (down), grid +j → -x (left)
            let x = 300.0 - j as f32 * 20.0;
            let y = 100.0 + i as f32 * 20.0;
            pts.push(Point2::new(x, y));
        }
    }
    let grid = detect_regular_grid(&pts).expect("detects");
    assert!(grid.stats.canonicalized);

    let at = |gi: i32, gj: i32| {
        grid.points
            .iter()
            .find(|p| p.grid == (gi, gj))
            .map(|p| p.position)
    };
    let p00 = at(0, 0).expect("(0,0)");
    let p10 = at(1, 0).expect("(1,0)");
    let p01 = at(0, 1).expect("(0,1)");
    assert!(p10.x > p00.x, "+i must point right after canonicalisation");
    assert!(p01.y > p00.y, "+j must point down after canonicalisation");

    // axis_i / axis_j must agree with the canonicalised labels.
    assert!(grid.axis_i.x > 0.0, "axis_i should point +x");
    assert!(grid.axis_j.y > 0.0, "axis_j should point +y");
}

#[test]
fn canonicalization_can_be_disabled() {
    let pts = axis_aligned_grid(5, 5, 25.0, 50.0, 50.0);
    let params = RegularGridParams::default().with_canonicalize_top_left(false);
    let grid = RegularGridDetector::new(params)
        .detect(&pts)
        .expect("detects");
    assert!(!grid.stats.canonicalized);
    assert_eq!(grid.points.len(), 25);
}

#[test]
fn extension_disabled_still_recovers_clean_grid() {
    let pts = axis_aligned_grid(6, 6, 25.0, 50.0, 50.0);
    let params = RegularGridParams::default().with_extension(ExtensionStrategy::Disabled);
    let grid = RegularGridDetector::new(params)
        .detect(&pts)
        .expect("detects");
    // BFS-grow alone covers a clean axis-aligned grid.
    assert_eq!(grid.points.len(), 36);
}

#[test]
fn too_few_points_returns_err() {
    let pts = vec![Point2::new(0.0, 0.0), Point2::new(10.0, 0.0)];
    assert_eq!(
        detect_regular_grid(&pts).unwrap_err(),
        RegularGridError::TooFewPoints { found: 2 }
    );
}

#[test]
fn collinear_cloud_yields_no_grid_found() {
    // A perfectly collinear cloud still has a recoverable global cell
    // size (uniform spacing), so it clears axis estimation but offers
    // no roughly-square parallelogram seed quad.
    let pts: Vec<Point2<f32>> = (0..8).map(|i| Point2::new(i as f32 * 12.0, 0.0)).collect();
    assert_eq!(
        detect_regular_grid(&pts).unwrap_err(),
        RegularGridError::NoGridFound
    );
}

#[test]
fn coincident_cloud_is_degenerate() {
    // Four or more coincident points: there is no nearest-neighbour
    // spacing to measure, so `estimate_grid_axes` cannot infer an axis
    // pair and the cloud is reported degenerate.
    let pts = vec![Point2::new(5.0, 5.0); 6];
    assert_eq!(
        detect_regular_grid(&pts).unwrap_err(),
        RegularGridError::DegeneratePointCloud
    );
}
