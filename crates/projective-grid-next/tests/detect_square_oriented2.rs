//! End-to-end integration tests for `detect_grid` with
//! `(LatticeKind::Square, Evidence::Oriented2)`.
//!
//! These tests are synthetic and target-agnostic: real-image regression for
//! consumer crates (chessboard, charuco, puzzleboard, marker) belongs in
//! those crates and lands in Phase E.

use std::collections::HashSet;

use nalgebra::Point2;
use projective_grid_next::{
    detect_grid, Coord, DetectionParams, DetectionRequest, Evidence, GridError, GrowParams,
    LatticeKind, LocalAxis, OrientedFeature, PointFeature, RejectionReason, SeedParams,
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

// ---------------------------------------------------------------------------
// New knobs (Phase E.1b prereq): cardinal_edge_quorum, boundary_search_factor,
// global_axis_u_v, candidate_pool_split. Each test verifies the default
// behaviour AND the legacy-equivalent override.
// ---------------------------------------------------------------------------

#[test]
fn cardinal_edge_quorum_one_admits_partial_band_pass() {
    // 5×5 grid; shift feature at lattice (2, 0) by 4 px in +y so the
    // (2, 0)→(2, 1) edge length (16 px) falls below the tight `[0.9, 1.1] *
    // cell_size` band (so out of band) while the (1, 1)→(2, 1) edge (20 px)
    // stays in band. With `cardinal_edge_quorum = u8::MAX` (default) the
    // (2, 1) attach is rejected (in_band = 1 < required = 2). With `quorum =
    // 1` the attach passes.
    let s = 20.0_f32;
    let mut features = axis_aligned_features(5, 5, s);
    // Feature index of lattice (2, 0) is 2 (row-major, cols=5).
    features[2].point.position.y += 4.0;

    let mut grow_strict = GrowParams::<f32>::default();
    grow_strict.edge_length_tol = 0.1;
    let request = DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Oriented2(&features),
        None,
        DetectionParams::default().with_grow(grow_strict),
    );
    let strict = detect_grid(request).expect("default cardinal_edge_quorum");

    let grow_quorum1 = grow_strict.with_cardinal_edge_quorum(1);
    let request = DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Oriented2(&features),
        None,
        DetectionParams::default().with_grow(grow_quorum1),
    );
    let lenient = detect_grid(request).expect("cardinal_edge_quorum = 1");

    assert!(
        lenient.grid.entries.len() > strict.grid.entries.len(),
        "quorum=1 should label more than the strict default (got {} vs {})",
        lenient.grid.entries.len(),
        strict.grid.entries.len()
    );
}

#[test]
fn boundary_search_factor_extends_extrapolated_reach() {
    // 4×4 inner grid plus a 5th perimeter column whose features sit at
    // a position the BFS prediction can't quite reach with the default
    // search radius. The perimeter cells are placed at the true lattice
    // x = 130 but their `(3, j)` cardinal neighbour is offset 4 px in
    // the +x direction (to 114), so the local-step prediction for the
    // perimeter cells lands at x = 138 — 8 px from the true x = 130.
    // With `attach_search_rel = 0.20` the unscaled search radius
    // (4 px at cell_size = 20) cannot bridge the gap, so the perimeter
    // column stays unlabelled. The `boundary_search_factor` knob
    // multiplies the radius for cells outside the labelled bbox;
    // setting it to `3.0` widens the radius to 12 px and the perimeter
    // cells attach.
    //
    // The post-validate / post-fit gates are loosened just enough
    // (`max_residual_px = 10`) to allow the small residual the (3, j)
    // shift induces; the test isolates the BFS search-radius behaviour.
    let s = 20.0_f32;
    let origin = 50.0_f32;
    let mut features = axis_aligned_features(4, 4, s);
    // Shift the entire column 3 by +4 px in x so the local-step
    // prediction for column 4 lands 8 px beyond the true perimeter
    // feature. Shifting the whole column keeps the inner straight-line
    // validate gate happy.
    let cols = 4_usize;
    for j in 0..4 {
        features[3 + j * cols].point.position.x += 4.0;
    }
    let next_idx = features.len();
    for j in 0..4 {
        let x = origin + 4.0 * s;
        let y = origin + (j as f32) * s;
        let point = PointFeature::new(next_idx + j, Point2::new(x, y));
        let axes = [
            LocalAxis::new(0.0_f32, None),
            LocalAxis::new(std::f32::consts::FRAC_PI_2, None),
        ];
        features.push(OrientedFeature::new(point, axes));
    }

    let mut grow_default = GrowParams::<f32>::default();
    grow_default.attach_search_rel = 0.20;
    let params_default = DetectionParams::default()
        .with_grow(grow_default)
        .with_max_residual_px(20.0);
    let request = DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Oriented2(&features),
        None,
        params_default,
    );
    let baseline = detect_grid(request).expect("default boundary_search_factor");

    let grow_wide = grow_default.with_boundary_search_factor(3.0);
    let params_wide = DetectionParams::default()
        .with_grow(grow_wide)
        .with_max_residual_px(20.0);
    let request = DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Oriented2(&features),
        None,
        params_wide,
    );
    let widened = detect_grid(request).expect("boundary_search_factor = 3.0");

    assert!(
        widened.grid.entries.len() > baseline.grid.entries.len(),
        "boundary_search_factor should reach more perimeter cells \
         (got {} vs baseline {})",
        widened.grid.entries.len(),
        baseline.grid.entries.len()
    );
}

#[test]
fn global_axis_u_v_overrides_seed_derived_axes() {
    // 5×5 axis-aligned grid where the first 2×2 (the canonical seed
    // quad) is rotated 14° around its anchor `A = (0, 0)`. Rotation
    // preserves the parallelogram, the four seed edges, and the per-
    // corner axes (the seed-finder consults the *features' own* axes
    // for B/C classification, not the chord directions, so the seed
    // still parses). The downstream features stay axis-aligned, so the
    // seed-derived `B-A` chord sits 14° off the true `[0, π/2]` global
    // axes.
    //
    // To isolate the global-axis behaviour the test disables
    // `local_step_fallback` — otherwise the BFS uses neighbour-pair
    // local steps that inherit the same seed chord rotation, and the
    // global override has nothing to bite on at the first ring of
    // attachments. With local steps off, the BFS prediction depends
    // entirely on the global axes: seed-derived 14° → labelling stalls;
    // the supplied `[0, π/2]` override → near-full labelling.
    let s = 20.0_f32;
    let mut features = axis_aligned_features(5, 5, s);
    let origin = 50.0_f32;
    let theta = 14.0_f32.to_radians();
    let (cos_t, sin_t) = (theta.cos(), theta.sin());
    let cols = 5_usize;
    // Rotate B = (1, 0), C = (0, 1), D = (1, 1) around A = (0, 0). A is
    // the rotation anchor, so its own position is unchanged.
    let ax = origin;
    let ay = origin;
    for &(i, j) in &[(1usize, 0usize), (0, 1), (1, 1)] {
        let idx = i + j * cols;
        let p = &mut features[idx].point.position;
        let dx = p.x - ax;
        let dy = p.y - ay;
        p.x = ax + cos_t * dx - sin_t * dy;
        p.y = ay + sin_t * dx + cos_t * dy;
    }

    let mut grow_default = GrowParams::<f32>::default();
    grow_default.local_step_fallback = false;
    let params_seed = DetectionParams::<f32>::default()
        .with_grow(grow_default)
        .with_max_residual_px(50.0);
    let request = DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Oriented2(&features),
        None,
        params_seed,
    );
    let seed_axes = detect_grid(request).expect("seed-derived axes");

    let grow_override = grow_default.with_global_axis_u_v([0.0, std::f32::consts::FRAC_PI_2]);
    let params_override = DetectionParams::<f32>::default()
        .with_grow(grow_override)
        .with_max_residual_px(50.0);
    let request = DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Oriented2(&features),
        None,
        params_override,
    );
    let overridden = detect_grid(request).expect("global_axis_u_v override");

    assert!(
        overridden.grid.entries.len() > seed_axes.grid.entries.len(),
        "global axis override should beat the biased seed-derived axes \
         (got {} vs {})",
        overridden.grid.entries.len(),
        seed_axes.grid.entries.len()
    );
}

#[test]
fn candidate_pool_split_wrong_parity_blocks_seed() {
    // 5×5 grid where every feature is tagged with its `(i + j) % 2` parity.
    // The canonical chess seed (A in pool 0, B/C in pool 1, D in pool 0)
    // is satisfied so the detector finds a seed. With every tag forced to
    // 0, B and C candidates (pool 1) become invalid and the seed finder
    // reports `DegenerateGeometry` (the post-`find_quad` `ok_or` in
    // `detect_square_oriented2_seed_grow`).
    let s = 20.0_f32;
    let cols = 5_i32;
    let features = axis_aligned_features(cols, cols, s);

    let parity_tags: Vec<u8> = (0..(cols * cols) as usize)
        .map(|idx| {
            let i = idx as i32 % cols;
            let j = idx as i32 / cols;
            ((i + j).rem_euclid(2)) as u8
        })
        .collect();
    let seed_parity = SeedParams::<f32>::default().with_candidate_pool_split(parity_tags);
    let request = DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Oriented2(&features),
        None,
        DetectionParams::default().with_seed(seed_parity),
    );
    let solution = detect_grid(request).expect("parity-valid tags should still solve");
    assert!(
        solution.grid.entries.len() >= 24,
        "parity-tagged 5×5 should recover near-full grid (got {})",
        solution.grid.entries.len()
    );

    let bad_tags: Vec<u8> = vec![0; (cols * cols) as usize];
    let seed_bad = SeedParams::<f32>::default().with_candidate_pool_split(bad_tags);
    let request = DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Oriented2(&features),
        None,
        DetectionParams::default().with_seed(seed_bad),
    );
    let err = detect_grid(request).unwrap_err();
    assert_eq!(err, GridError::DegenerateGeometry);
}

#[test]
fn candidate_pool_split_wrong_length_is_inconsistent_input() {
    // Length-mismatch contract: `detect_grid_all` validates the tag slice
    // length against `features.len()` before reaching `find_quad`.
    let features = axis_aligned_features(3, 3, 20.0);
    let seed_short = SeedParams::<f32>::default().with_candidate_pool_split(vec![0_u8, 1, 0, 1]);
    let request = DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Oriented2(&features),
        None,
        DetectionParams::default().with_seed(seed_short),
    );
    let err = detect_grid(request).unwrap_err();
    assert!(
        matches!(err, GridError::InconsistentInput(_)),
        "expected InconsistentInput, got {err:?}"
    );
}
