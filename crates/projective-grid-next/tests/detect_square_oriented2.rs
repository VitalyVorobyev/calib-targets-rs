//! End-to-end integration tests for `detect_grid` with
//! `(LatticeKind::Square, Evidence::Oriented2)`.
//!
//! These tests are synthetic and target-agnostic: real-image regression for
//! consumer crates (chessboard, charuco, puzzleboard, marker) belongs in
//! those crates and lands in Phase E.

use std::collections::HashSet;

use nalgebra::Point2;
use projective_grid_next::{
    detect_grid, Coord, DetectionParams, DetectionRequest, Evidence, LatticeKind, LocalAxis,
    OrientedFeature, PointFeature, RejectionReason,
};

fn axis_aligned_features(rows: i32, cols: i32, s: f32) -> Vec<OrientedFeature<f32, 2>> {
    let origin = 50.0_f32;
    let mut out = Vec::with_capacity((rows * cols) as usize);
    let mut idx = 0_usize;
    for j in 0..rows {
        for i in 0..cols {
            let x = (i as f32) * s + origin;
            let y = (j as f32) * s + origin;
            let point = PointFeature::new(idx, Point2::new(x, y));
            let axes = [
                LocalAxis::new(0.0_f32, None),
                LocalAxis::new(std::f32::consts::FRAC_PI_2, None),
            ];
            out.push(OrientedFeature::new(point, axes));
            idx += 1;
        }
    }
    out
}

fn assert_all_labels_in_box(coords: &HashSet<Coord>, max_u: i32, max_v: i32) {
    for c in coords {
        assert!(
            c.u >= 0 && c.u <= max_u && c.v >= 0 && c.v <= max_v,
            "coord {:?} outside rebased bbox (0, 0)..({}, {})",
            c,
            max_u,
            max_v
        );
    }
    // Every (u, v) in the box must be present (perfect grid).
    for u in 0..=max_u {
        for v in 0..=max_v {
            assert!(coords.contains(&Coord::new(u, v)), "missing coord {u},{v}");
        }
    }
}

#[test]
fn perfect_5x5_grid_is_fully_labelled() {
    let features = axis_aligned_features(5, 5, 20.0);
    let request = DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Oriented2(&features),
        None,
        DetectionParams::default(),
    );
    let solution = detect_grid(request).expect("detect_grid on perfect 5x5 grid");

    assert_eq!(solution.grid.lattice, LatticeKind::Square);
    assert_eq!(solution.grid.entries.len(), 25);
    assert_eq!(solution.rejected.len(), 0);

    let coords: HashSet<Coord> = solution.grid.entries.iter().map(|e| e.coord).collect();
    assert_all_labels_in_box(&coords, 4, 4);

    let fit = solution.grid.bbox.expect("bbox present on non-empty grid");
    assert_eq!(fit, (Coord::new(0, 0), Coord::new(4, 4)));

    let fit = solution.fit.expect("fit present on success");
    assert!(
        fit.residuals.max_px < 0.01,
        "max residual {} too high on perfect grid",
        fit.residuals.max_px
    );
}

#[test]
fn perturbed_5x5_grid_recovers_at_least_24_of_25() {
    // Position jitter ≤ 0.5 px, axis jitter ≤ 5°. Deterministic perturbation
    // via a tiny xorshift seeded with the index — no randomness, so the test
    // doesn't flake.
    let s = 20.0_f32;
    let rows = 5_i32;
    let cols = 5_i32;
    let mut features = axis_aligned_features(rows, cols, s);
    let mut state: u32 = 0x1234_5678;
    fn next(s: &mut u32) -> f32 {
        *s ^= *s << 13;
        *s ^= *s >> 17;
        *s ^= *s << 5;
        // map to (-1, 1)
        (*s as f32) / (u32::MAX as f32) * 2.0 - 1.0
    }
    for feature in features.iter_mut() {
        let dx = next(&mut state) * 0.5;
        let dy = next(&mut state) * 0.5;
        feature.point.position.x += dx;
        feature.point.position.y += dy;
        // 5° = 0.0873 rad
        let da0 = next(&mut state) * 0.0873;
        let da1 = next(&mut state) * 0.0873;
        feature.axes[0] = LocalAxis::new(feature.axes[0].angle_rad + da0, None);
        feature.axes[1] = LocalAxis::new(feature.axes[1].angle_rad + da1, None);
    }

    let request = DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Oriented2(&features),
        None,
        DetectionParams::default(),
    );
    let solution = detect_grid(request).expect("detect_grid on perturbed 5x5 grid");

    let labelled = solution.grid.entries.len();
    assert!(
        labelled >= 24,
        "expected >= 24/25 labelled on perturbed grid, got {labelled}",
    );

    let fit = solution.fit.expect("fit present on success");
    assert!(
        fit.residuals.max_px < 1.0,
        "max residual {} too high on perturbed grid",
        fit.residuals.max_px
    );

    // Labels remain non-negative after rebase.
    for entry in &solution.grid.entries {
        assert!(
            entry.coord.u >= 0 && entry.coord.v >= 0,
            "{:?}",
            entry.coord
        );
    }
}

#[test]
fn extra_noise_features_are_rejected_not_labelled() {
    // 25 grid features at canonical positions + 5 noise points placed far
    // from any grid intersection. The detector should label all 25 grid
    // features and reject all 5 noise features.
    let s = 20.0_f32;
    let mut features = axis_aligned_features(5, 5, s);

    // Insert noise points well outside the grid support so they are not
    // mistaken for lattice corners.
    let noise_positions: [(f32, f32); 5] = [
        (-100.0, -100.0),
        (-100.0, 500.0),
        (500.0, -100.0),
        (500.0, 500.0),
        (300.0, -200.0),
    ];
    let next_idx = features.len();
    for (i, (x, y)) in noise_positions.iter().enumerate() {
        let point = PointFeature::new(next_idx + i, Point2::new(*x, *y));
        // Noise points get arbitrary axes; the detector should still ignore
        // them because they fall outside the lattice support.
        let axes = [
            LocalAxis::new(0.0_f32, None),
            LocalAxis::new(std::f32::consts::FRAC_PI_2, None),
        ];
        features.push(OrientedFeature::new(point, axes));
    }

    let request = DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Oriented2(&features),
        None,
        DetectionParams::default(),
    );
    let solution = detect_grid(request).expect("detect_grid on noise-augmented grid");

    assert_eq!(
        solution.grid.entries.len(),
        25,
        "expected exactly 25 grid labels"
    );
    assert_eq!(
        solution.rejected.len(),
        5,
        "expected 5 rejected noise features, got {}",
        solution.rejected.len()
    );
    for r in &solution.rejected {
        assert_eq!(r.reason, RejectionReason::Unlabelled);
    }

    let fit = solution.fit.expect("fit present");
    assert!(fit.residuals.max_px < 0.01, "{}", fit.residuals.max_px);
}

#[test]
fn noise_features_inside_grid_support_are_not_labelled() {
    // Tighter robustness test: noise points placed *inside* the lattice
    // bounding box, but at non-lattice positions. The BFS attach loop and
    // the validate gate together must reject these without ever giving
    // them a lattice coord.
    let s = 20.0_f32;
    let mut features = axis_aligned_features(5, 5, s);

    let bbox_origin = 50.0_f32;
    let bbox_max = bbox_origin + 4.0 * s;
    let noise_positions: [(f32, f32); 4] = [
        (bbox_origin + 0.5 * s, bbox_origin + 0.5 * s), // dead centre of cell (0,0)
        (bbox_origin + 2.5 * s, bbox_origin + 1.5 * s), // centre of cell (2,1)
        (bbox_origin + 0.5 * s, bbox_max - 0.5 * s),    // centre of cell (0,3)
        (bbox_origin + 3.5 * s, bbox_origin + 3.5 * s), // centre of cell (3,3)
    ];
    let next_idx = features.len();
    for (i, (x, y)) in noise_positions.iter().enumerate() {
        let point = PointFeature::new(next_idx + i, Point2::new(*x, *y));
        let axes = [
            LocalAxis::new(0.0_f32, None),
            LocalAxis::new(std::f32::consts::FRAC_PI_2, None),
        ];
        features.push(OrientedFeature::new(point, axes));
    }

    let request = DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Oriented2(&features),
        None,
        DetectionParams::default(),
    );
    let solution = detect_grid(request).expect("detect_grid on cell-centre noise");

    let labelled = solution.grid.entries.len();
    assert_eq!(
        labelled, 25,
        "expected exactly 25 grid labels even with interior noise, got {labelled}"
    );
    let labelled_source_indices: HashSet<usize> = solution
        .grid
        .entries
        .iter()
        .map(|e| e.source_index)
        .collect();
    for i in 0..4 {
        assert!(
            !labelled_source_indices.contains(&(next_idx + i)),
            "noise feature {} was incorrectly labelled",
            next_idx + i
        );
    }

    let fit = solution.fit.expect("fit present");
    assert!(fit.residuals.max_px < 0.01, "{}", fit.residuals.max_px);
}

#[test]
fn fewer_than_four_features_returns_insufficient_evidence() {
    let features = axis_aligned_features(1, 3, 20.0);
    let request = DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Oriented2(&features),
        None,
        DetectionParams::default(),
    );
    let err = detect_grid(request).unwrap_err();
    assert_eq!(err, projective_grid_next::GridError::InsufficientEvidence);
}
