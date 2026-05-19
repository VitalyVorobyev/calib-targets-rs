//! Advanced-policy test: the validator-driven `detect_square_grid`
//! path exercised with explicit caller-written validators.
//!
//! This is **not** the onboarding path. A caller with only a point
//! cloud should use [`projective_grid::detect_regular_grid`] (covered
//! by `tests/regular_grid.rs`), which supplies a built-in regular-grid
//! policy. This file deliberately hand-writes the `SeedQuadValidator`
//! and `GrowValidator` pair to verify that the advanced API — the hook
//! point for pattern-specific detectors (chessboard parity, marker
//! slots, and so on) — still works end-to-end.
//!
//! The validators here encode no real pattern rules — they accept any
//! parallelogram-shaped 2×2 seed and any candidate at any cell — so
//! the test also serves as a reference for what a minimal custom
//! policy looks like. The pipeline's geometric primitives alone
//! (seed-finder, BFS-grow, global-H extension, hole fill, line/local-H
//! validation) carry the grid through.

use std::f32::consts::FRAC_PI_2;

use nalgebra::{Matrix3, Point2};

use projective_grid::component_merge::{merge_components_local, ComponentInput, LocalMergeParams};
use projective_grid::square::grow::{Admit, GrowValidator, LabelledNeighbour};
use projective_grid::square::seed::finder::SeedQuadValidator;
use projective_grid::{
    detect_square_grid, detect_square_grid_all, AxisEstimate, ExtensionStrategy,
    MultiComponentParams, SquareGridParams,
};

/// A pattern-agnostic seed validator that partitions corners into two
/// "color" groups by `(i + j) % 2` (matches the chessboard convention
/// without depending on any chess-corners types). Axes are passed
/// through verbatim — every corner gets the same `(0, π/2)` pair.
struct ToySeedValidator {
    positions: Vec<Point2<f32>>,
    is_a: Vec<bool>,
}

impl SeedQuadValidator for ToySeedValidator {
    fn position(&self, idx: usize) -> Point2<f32> {
        self.positions[idx]
    }

    fn axes(&self, idx: usize) -> [AxisEstimate; 2] {
        let _ = idx;
        [
            AxisEstimate::from_angle(0.0),
            AxisEstimate::from_angle(FRAC_PI_2),
        ]
    }

    fn a_candidates(&self) -> Vec<usize> {
        (0..self.is_a.len()).filter(|&i| self.is_a[i]).collect()
    }

    fn bc_candidates(&self) -> Vec<usize> {
        (0..self.is_a.len()).filter(|&i| !self.is_a[i]).collect()
    }
}

/// Permissive grow validator: every corner is eligible, every candidate
/// is accepted, every edge passes. The geometric checks inside
/// `bfs_grow` (KD-tree neighbour search, ambiguity gate) carry the
/// recovery.
struct OpenValidator;

impl GrowValidator for OpenValidator {
    fn is_eligible(&self, _idx: usize) -> bool {
        true
    }

    fn required_label_at(&self, _i: i32, _j: i32) -> Option<u8> {
        None
    }

    fn label_of(&self, _idx: usize) -> Option<u8> {
        None
    }

    fn accept_candidate(
        &self,
        _idx: usize,
        _at: (i32, i32),
        _prediction: Point2<f32>,
        _neighbours: &[LabelledNeighbour],
    ) -> Admit {
        Admit::Accept
    }
}

/// Build a perspective-warped `rows × cols` grid centred near pixel
/// (256, 256), with an explicit homography to introduce realistic
/// foreshortening. Same warp as `benches/grow.rs` / `benches/topological.rs`.
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

#[test]
fn detect_square_grid_recovers_a_clean_perspective_warped_grid() {
    let rows = 7;
    let cols = 7;
    let positions = perspective_warped_grid(rows, cols, 30.0);

    // Partition into the two "colour" groups expected by the seed
    // finder: (i + j) % 2 == 0 → A candidates, else BC candidates.
    let is_a: Vec<bool> = (0..(rows * cols) as usize)
        .map(|n| {
            let i = (n as i32) % cols;
            let j = (n as i32) / cols;
            (i + j).rem_euclid(2) == 0
        })
        .collect();
    let seed_validator = ToySeedValidator {
        positions: positions.clone(),
        is_a,
    };
    let grow_validator = OpenValidator;

    let params = SquareGridParams::default();
    let detection = detect_square_grid(&positions, &seed_validator, &grow_validator, &params)
        .expect("seed finder should pick a 2x2 quad on a clean perspective-warped grid");

    // Every corner must be labelled (no holes; perfect 49-cell grid).
    assert_eq!(
        detection.labelled.len(),
        (rows * cols) as usize,
        "expected every corner to be labelled (got {}, want {})",
        detection.labelled.len(),
        rows * cols
    );

    // Bounding box rebased to (0, 0) — invariant enforced by bfs_grow.
    let min_i = detection.labelled.keys().map(|(i, _)| *i).min().unwrap();
    let min_j = detection.labelled.keys().map(|(_, j)| *j).min().unwrap();
    let max_i = detection.labelled.keys().map(|(i, _)| *i).max().unwrap();
    let max_j = detection.labelled.keys().map(|(_, j)| *j).max().unwrap();
    assert_eq!((min_i, min_j), (0, 0));
    assert_eq!((max_i, max_j), (cols - 1, rows - 1));

    // Stats sanity: a seed and a positive number of BFS-grown corners
    // (the seed contributes 4, the rest are grown).
    assert!(detection.stats.seed.is_some(), "seed must be recorded");
    assert!(
        detection.stats.grown >= (rows * cols) as usize - 4 - 8,
        "expected BFS to attach the bulk of the grid; got {}",
        detection.stats.grown
    );

    // Validation must run when params.validate is Some and produce a
    // result (empty blacklist on a clean grid is acceptable).
    let validation = detection
        .stats
        .validation
        .as_ref()
        .expect("validation stage must run with default params");
    assert!(
        validation.blacklist.is_empty(),
        "clean grid should produce an empty blacklist; got {:?}",
        validation.blacklist
    );
}

#[test]
fn detect_square_grid_returns_none_when_no_seed_exists() {
    // Three colinear points: no parallelogram possible.
    let positions = vec![
        Point2::new(0.0, 0.0),
        Point2::new(10.0, 0.0),
        Point2::new(20.0, 0.0),
    ];
    let is_a = vec![true, false, true];
    let seed_validator = ToySeedValidator {
        positions: positions.clone(),
        is_a,
    };
    let grow_validator = OpenValidator;

    let detection = detect_square_grid(
        &positions,
        &seed_validator,
        &grow_validator,
        &SquareGridParams::default(),
    );
    assert!(
        detection.is_none(),
        "expected None when no seed can be found"
    );
}

#[test]
fn detect_square_grid_all_recovers_two_disjoint_components() {
    // Two clean axis-aligned grids placed far apart so the seed
    // finder never bridges them. Each side picks up one component;
    // the multi-component driver should return exactly two.
    let mut positions: Vec<Point2<f32>> = Vec::new();
    let mut is_a: Vec<bool> = Vec::new();
    let s = 20.0;

    // Component A: 5×5 grid centred at (100, 100).
    for j in 0..5i32 {
        for i in 0..5i32 {
            positions.push(Point2::new(100.0 + i as f32 * s, 100.0 + j as f32 * s));
            is_a.push((i + j).rem_euclid(2) == 0);
        }
    }
    // Component B: 4×4 grid centred at (600, 600).
    for j in 0..4i32 {
        for i in 0..4i32 {
            positions.push(Point2::new(600.0 + i as f32 * s, 600.0 + j as f32 * s));
            is_a.push((i + j).rem_euclid(2) == 0);
        }
    }

    let seed_validator = ToySeedValidator {
        positions: positions.clone(),
        is_a,
    };
    let grow_validator = OpenValidator;

    let detections = detect_square_grid_all(
        &positions,
        &seed_validator,
        &grow_validator,
        &SquareGridParams::default(),
        &MultiComponentParams::default(),
    );

    assert_eq!(
        detections.len(),
        2,
        "expected exactly two components (got {})",
        detections.len()
    );

    // The two components together cover every corner.
    let total_labelled: usize = detections.iter().map(|d| d.labelled.len()).sum();
    assert_eq!(total_labelled, positions.len());

    // Indices recovered in each component must be disjoint.
    let mut seen: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for det in &detections {
        for (_, &idx) in det.labelled.iter() {
            assert!(seen.insert(idx), "corner {idx} appeared in two components");
        }
    }

    // Bonus: feeding the two components through merge_components_local
    // does NOT merge them — they're disjoint in label space. This is
    // the documented "Out-of-scope (v1)" case in component_merge.rs.
    let inputs: Vec<ComponentInput> = detections
        .iter()
        .map(|d| ComponentInput {
            labelled: &d.labelled,
            positions: &positions,
        })
        .collect();
    let merged = merge_components_local(&inputs, &LocalMergeParams::default());
    assert_eq!(
        merged.components.len(),
        2,
        "disjoint components shouldn't merge"
    );
}

#[test]
fn detect_square_grid_skips_disabled_stages() {
    // Run with extension/fill/validate disabled: pipeline collapses
    // to seed → grow. Still recovers the 7×7 grid because the
    // perspective warp is mild.
    let rows = 7;
    let cols = 7;
    let positions = perspective_warped_grid(rows, cols, 30.0);
    let is_a: Vec<bool> = (0..(rows * cols) as usize)
        .map(|n| {
            let i = (n as i32) % cols;
            let j = (n as i32) / cols;
            (i + j).rem_euclid(2) == 0
        })
        .collect();
    let seed_validator = ToySeedValidator {
        positions: positions.clone(),
        is_a,
    };
    let grow_validator = OpenValidator;

    let mut params = SquareGridParams::default();
    params.extension = ExtensionStrategy::Disabled;
    params.fill = None;
    params.validate = None;
    let detection = detect_square_grid(&positions, &seed_validator, &grow_validator, &params)
        .expect("seed must still be found");

    assert!(detection.stats.extension.is_none());
    assert!(detection.stats.fill.is_none());
    assert!(detection.stats.validation.is_none());
    // BFS-grow alone covers every corner on a clean grid.
    assert_eq!(detection.labelled.len(), (rows * cols) as usize);
}
