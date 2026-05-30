//! End-to-end integration tests for `detect_grid` with
//! `(LatticeKind::Square, Evidence::Oriented2)` running the Phase D
//! axis-driven topological algorithm.
//!
//! All inputs are synthetic and target-agnostic; consumer-crate
//! migration lands in Phase E.

use std::collections::HashSet;

use nalgebra::{Matrix3, Point2, Projective2, Vector3};
use projective_grid::{
    detect_grid, Coord, DetectionParams, DetectionRequest, Evidence, GridError, LatticeKind,
    LocalAxis, OrientedFeature, PointFeature, RejectionReason, SquareAlgorithm,
};

fn topological_params() -> DetectionParams<f32> {
    DetectionParams::<f32>::default().with_algorithm(SquareAlgorithm::Topological)
}

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
fn clean_5x5_grid_is_fully_labelled() {
    let features = axis_aligned_features(5, 5, 20.0);
    let request = DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Oriented2(&features),
        None,
        topological_params(),
    );
    let solution = detect_grid(request).expect("detect_grid (topological) on clean 5x5");

    assert_eq!(solution.grid.lattice, LatticeKind::Square);
    assert_eq!(solution.grid.entries.len(), 25, "all 25 features labelled");

    let coords: HashSet<Coord> = solution.grid.entries.iter().map(|e| e.coord).collect();
    for u in 0..5 {
        for v in 0..5 {
            assert!(coords.contains(&Coord::new(u, v)), "missing coord {u},{v}");
        }
    }
    let bbox = solution.grid.bbox.expect("non-empty grid has bbox");
    assert_eq!(bbox, (Coord::new(0, 0), Coord::new(4, 4)));

    let fit = solution.fit.expect("fit present");
    assert!(
        fit.residuals.max_px < 0.01,
        "max residual {} too high on clean grid",
        fit.residuals.max_px,
    );
}

#[test]
fn perturbed_5x5_grid_recovers_at_least_24_of_25() {
    // Position jitter ≤ 0.5 px, axis jitter ≤ 5°. Deterministic via a
    // tiny xorshift seeded with the index — no randomness, no flakes.
    let s = 20.0_f32;
    let mut features = axis_aligned_features(5, 5, s);
    let mut state: u32 = 0x1234_5678;
    fn next(s: &mut u32) -> f32 {
        *s ^= *s << 13;
        *s ^= *s >> 17;
        *s ^= *s << 5;
        (*s as f32) / (u32::MAX as f32) * 2.0 - 1.0
    }
    for feature in features.iter_mut() {
        let dx = next(&mut state) * 0.5;
        let dy = next(&mut state) * 0.5;
        feature.point.position.x += dx;
        feature.point.position.y += dy;
        let da0 = next(&mut state) * 0.0873;
        let da1 = next(&mut state) * 0.0873;
        feature.axes[0] = LocalAxis::new(feature.axes[0].angle_rad + da0, Some(0.05));
        feature.axes[1] = LocalAxis::new(feature.axes[1].angle_rad + da1, Some(0.05));
    }

    let request = DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Oriented2(&features),
        None,
        topological_params(),
    );
    let solution = detect_grid(request).expect("detect_grid (topological) on perturbed 5x5");

    let labelled = solution.grid.entries.len();
    assert!(
        labelled >= 24,
        "expected >= 24/25 labelled on perturbed grid, got {labelled}",
    );

    let fit = solution.fit.expect("fit present");
    assert!(
        fit.residuals.max_px < 1.5,
        "max residual {} too high on perturbed grid",
        fit.residuals.max_px,
    );

    for entry in &solution.grid.entries {
        assert!(
            entry.coord.u >= 0 && entry.coord.v >= 0,
            "labels must be non-negative, got {:?}",
            entry.coord,
        );
    }
}

#[test]
fn extra_noise_features_with_no_info_axes_are_rejected() {
    // 25 grid features at canonical positions + 5 noisers carrying the
    // "no information" sentinel axes (`sigma_rad = Some(π)`). Under the
    // topological eligibility rule the noisers are filtered before
    // Delaunay, so they must appear in `rejected` as `Unlabelled`.
    let s = 20.0_f32;
    let mut features = axis_aligned_features(5, 5, s);

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
        let axes = [
            LocalAxis::new(0.0_f32, Some(std::f32::consts::PI)),
            LocalAxis::new(std::f32::consts::FRAC_PI_2, Some(std::f32::consts::PI)),
        ];
        features.push(OrientedFeature::new(point, axes));
    }

    let request = DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Oriented2(&features),
        None,
        topological_params(),
    );
    let solution = detect_grid(request).expect("detect_grid (topological) on noise-augmented");

    assert_eq!(
        solution.grid.entries.len(),
        25,
        "expected exactly 25 grid labels",
    );
    assert_eq!(
        solution.rejected.len(),
        5,
        "expected 5 rejected noise features",
    );
    for r in &solution.rejected {
        assert_eq!(r.reason, RejectionReason::Unlabelled);
    }
}

#[test]
fn perspective_warped_5x5_grid_recovers_at_least_22_of_25() {
    // Phase D's success signal: the axis-driven classifier should handle
    // moderate projective warp without losing more than ~3 of 25
    // corners. A length-based or angle-bisector classifier would do
    // markedly worse here because the projected cell diagonal is not at
    // 45° from the projected grid axes.
    let s = 20.0_f32;
    let origin = (50.0_f32, 50.0_f32);

    // Build a perspective transform that foreshortens along x.
    let h_matrix = Matrix3::new(
        1.0, 0.1, 0.0, // first row
        0.05, 1.0, 0.0, // second row
        0.0015, 0.0008, 1.0, // third row — perspective
    );
    let h = Projective2::from_matrix_unchecked(h_matrix);

    let mut features = Vec::with_capacity(25);
    for j in 0..5i32 {
        for i in 0..5i32 {
            let model = Point2::new((i as f32) * s + origin.0, (j as f32) * s + origin.1);
            let v = h.matrix() * Vector3::new(model.x, model.y, 1.0);
            let image = Point2::new(v.x / v.z, v.y / v.z);

            // Local axes = direction of the warped step in (+u, 0) and (0, +v).
            let next_u = Point2::new(model.x + 1.0, model.y);
            let next_v = Point2::new(model.x, model.y + 1.0);
            let v_u = h.matrix() * Vector3::new(next_u.x, next_u.y, 1.0);
            let v_v = h.matrix() * Vector3::new(next_v.x, next_v.y, 1.0);
            let p_u = Point2::new(v_u.x / v_u.z, v_u.y / v_u.z);
            let p_v = Point2::new(v_v.x / v_v.z, v_v.y / v_v.z);

            // Axis angles (in [-π, π]); the classifier normalises modulo π.
            let theta_u = (p_u.y - image.y).atan2(p_u.x - image.x);
            let theta_v = (p_v.y - image.y).atan2(p_v.x - image.x);

            let point = PointFeature::new(features.len(), image);
            let axes = [
                LocalAxis::new(theta_u, Some(0.05)),
                LocalAxis::new(theta_v, Some(0.05)),
            ];
            features.push(OrientedFeature::new(point, axes));
        }
    }

    let topo = projective_grid::TopologicalParams::<f32>::default()
        // Loosen the per-cell length-ratio gate so the foreshortened
        // far end of the perspective doesn't trip the upper bound.
        .with_edge_length_max_rel(3.5);
    let params = DetectionParams::<f32>::default()
        .with_algorithm(SquareAlgorithm::Topological)
        .with_topological(topo)
        // Bump the residual threshold: the projective fit is global
        // and the warped 5×5 corners spread residuals out.
        .with_max_residual_px(5.0);
    let request = DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Oriented2(&features),
        None,
        params,
    );
    let solution = detect_grid(request).expect("detect_grid (topological) on perspective grid");

    let labelled = solution.grid.entries.len();
    // Phase D success signal: the axis-driven classifier should
    // handle the perspective warp essentially perfectly. The brief's
    // target was ≥ 22/25 (a length-based classifier would do markedly
    // worse); the per-corner rotated axes let the topological
    // pipeline recover all 25 corners in practice.
    assert!(
        labelled >= 22,
        "expected >= 22/25 labelled on perspective-warped grid, got {labelled}",
    );

    let fit = solution.fit.expect("fit present");
    assert!(
        fit.residuals.max_px < 1.0,
        "max residual {} too high on perspective grid",
        fit.residuals.max_px,
    );

    for entry in &solution.grid.entries {
        assert!(
            entry.coord.u >= 0 && entry.coord.v >= 0,
            "labels must be non-negative, got {:?}",
            entry.coord,
        );
    }
}

#[test]
fn fewer_than_three_features_errors() {
    let features = axis_aligned_features(1, 2, 20.0);
    let request = DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Oriented2(&features),
        None,
        topological_params(),
    );
    let err = detect_grid(request).unwrap_err();
    assert_eq!(err, GridError::InsufficientEvidence);
}

#[test]
fn topological_default_off_seed_grow_used_by_default() {
    // Phase C's default selector (SeedAndGrow) must remain unchanged so
    // the Phase C integration test suite keeps passing without code
    // changes. This test is a regression gate on that default.
    let features = axis_aligned_features(5, 5, 20.0);
    let request = DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Oriented2(&features),
        None,
        DetectionParams::<f32>::default(),
    );
    let solution = detect_grid(request).expect("default (seed-and-grow) on clean 5x5");
    assert_eq!(solution.grid.entries.len(), 25);
}
