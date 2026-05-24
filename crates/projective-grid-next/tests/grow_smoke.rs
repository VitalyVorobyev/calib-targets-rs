//! Integration tests for the Phase 2 BFS engine.
//!
//! Each test runs once for `f32` and once for `f64`. The tests do not call a
//! `detect_*` entry point — those land in Phase 5. They drive the BFS
//! directly via the lowest-level public surface: build observations, seed
//! from a known 2×2 quad, call `bfs_grow` with `OpenContext`, and assert on
//! the labelled output.

mod common;

use std::collections::HashSet;

use nalgebra::Point2;

use projective_grid_next::diagnostics::NoOpSink;
use projective_grid_next::feature::Observation;
use projective_grid_next::float::Float;
use projective_grid_next::grow::{bfs_grow, GrowParams, OpenContext};
use projective_grid_next::seed::{Seed, SeedOutput};

use common::{axis_aligned_grid, lit, perspective_warped_grid, rotated_grid};

/// Build a seed from the corners at row-major indices
/// `(0, 1, cols, cols + 1)` — the top-left 2×2 of an axis-aligned grid.
fn seed_top_left_2x2<F: Float>(obs: &[Observation<F>], cols: i32) -> SeedOutput<F> {
    let c = cols as usize;
    let cell = (obs[1].position - obs[0].position).norm();
    SeedOutput::new(Seed::new(0, 1, c, c + 1), cell)
}

fn assert_clean_axis_aligned_7x7_is_fully_labelled<F: Float + kiddo::float::kdtree::Axis>() {
    let rows = 7_i32;
    let cols = 7_i32;
    let s = lit::<F>(25.0_f32);
    let obs = axis_aligned_grid::<F>(rows, cols, s, lit::<F>(60.0_f32), lit::<F>(60.0_f32));
    let seed = seed_top_left_2x2(&obs, cols);
    let ctx = OpenContext::<F>::new(obs.len());
    let mut sink = NoOpSink;
    let result = bfs_grow(&obs, &seed, &GrowParams::default(), &ctx, &mut sink);
    assert_eq!(
        result.labelled.len(),
        49,
        "every corner of the 7x7 must be labelled"
    );
    assert_eq!(result.bbox, ((0, 0), (cols - 1, rows - 1)));
    let (mi, mj) = result
        .labelled
        .keys()
        .fold((i32::MAX, i32::MAX), |(a, b), &(i, j)| (a.min(i), b.min(j)));
    assert_eq!((mi, mj), (0, 0));
}

fn assert_rotated_grid_is_recovered<F: Float + kiddo::float::kdtree::Axis>() {
    // 30° rotated 5x5.
    let theta = lit::<F>(30.0_f32) * F::pi() / lit::<F>(180.0_f32);
    let s = lit::<F>(22.0_f32);
    let obs = rotated_grid::<F>(5, 5, s, theta, lit::<F>(150.0_f32), lit::<F>(150.0_f32));
    let seed = seed_top_left_2x2(&obs, 5);
    // OpenContext supplies (0, π/2) as global axes for the seed finder;
    // when seeding from a known 2×2 we don't go through the finder. The grow
    // engine derives its own axes from the seed, so a 30° rotation is fine.
    let ctx = OpenContext::<F>::new(obs.len());
    let mut sink = NoOpSink;
    let result = bfs_grow(&obs, &seed, &GrowParams::default(), &ctx, &mut sink);
    // Allow recall ≤ 25, but expect at least 24 / 25 corners on a clean
    // rotated grid — the engine should not lose anything to rotation per se.
    assert!(
        result.labelled.len() >= 24,
        "expected ≥24 of 25 rotated corners, got {}",
        result.labelled.len()
    );
}

fn assert_perspective_warped_grid_is_recovered<F: Float + kiddo::float::kdtree::Axis>() {
    let obs = perspective_warped_grid::<F>(6, 6, lit::<F>(30.0_f32));
    let seed = seed_top_left_2x2(&obs, 6);
    let ctx = OpenContext::<F>::new(obs.len());
    let mut sink = NoOpSink;
    let result = bfs_grow(&obs, &seed, &GrowParams::default(), &ctx, &mut sink);
    assert!(
        result.labelled.len() >= 30,
        "expected ≥30 of 36 warped corners, got {}",
        result.labelled.len()
    );
}

fn assert_spurious_off_grid_points_are_not_labelled<F: Float + kiddo::float::kdtree::Axis>() {
    let rows = 5_i32;
    let cols = 5_i32;
    let s = lit::<F>(20.0_f32);
    let mut obs = axis_aligned_grid::<F>(rows, cols, s, lit::<F>(50.0_f32), lit::<F>(50.0_f32));
    // Inject 3 random off-grid points far enough from any cell centre to be
    // ineligible candidates. They sit between cells so they are within search
    // radius of some predictions but the unambiguous gate should reject them
    // (or they should never become the nearest).
    obs.push(Observation::new(Point2::new(
        lit::<F>(60.0_f32),
        lit::<F>(60.0_f32),
    )));
    obs.push(Observation::new(Point2::new(
        lit::<F>(95.0_f32),
        lit::<F>(105.0_f32),
    )));
    obs.push(Observation::new(Point2::new(
        lit::<F>(120.0_f32),
        lit::<F>(125.0_f32),
    )));
    let seed = seed_top_left_2x2(&obs, cols);
    let ctx = OpenContext::<F>::new(obs.len());
    let mut sink = NoOpSink;
    let result = bfs_grow(&obs, &seed, &GrowParams::default(), &ctx, &mut sink);
    // At most 25 labelled corners; off-grid points should never be labelled.
    assert!(result.labelled.len() <= 25);
    // The first 25 observations are the true grid; the 3 noise points are at
    // indices 25, 26, 27.
    let labelled_idx: HashSet<usize> = result.labelled.values().copied().collect();
    for noise_idx in 25..=27 {
        assert!(
            !labelled_idx.contains(&noise_idx),
            "noise observation {noise_idx} must not be labelled"
        );
    }
}

#[test]
fn clean_axis_aligned_7x7_is_fully_labelled_f32() {
    assert_clean_axis_aligned_7x7_is_fully_labelled::<f32>();
}
#[test]
fn clean_axis_aligned_7x7_is_fully_labelled_f64() {
    assert_clean_axis_aligned_7x7_is_fully_labelled::<f64>();
}
#[test]
fn rotated_grid_is_recovered_f32() {
    assert_rotated_grid_is_recovered::<f32>();
}
#[test]
fn rotated_grid_is_recovered_f64() {
    assert_rotated_grid_is_recovered::<f64>();
}
#[test]
fn perspective_warped_grid_is_recovered_f32() {
    assert_perspective_warped_grid_is_recovered::<f32>();
}
#[test]
fn perspective_warped_grid_is_recovered_f64() {
    assert_perspective_warped_grid_is_recovered::<f64>();
}
#[test]
fn spurious_off_grid_points_are_not_labelled_f32() {
    assert_spurious_off_grid_points_are_not_labelled::<f32>();
}
#[test]
fn spurious_off_grid_points_are_not_labelled_f64() {
    assert_spurious_off_grid_points_are_not_labelled::<f64>();
}
