//! End-to-end Criterion bench for the square topological detector path.
//!
//! Run with `cargo bench -p projective-grid --bench topological`.

use std::f32::consts::FRAC_PI_2;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use nalgebra::Point2;
use projective_grid::{
    detect_grid_all, DetectionParams, DetectionRequest, Evidence, LatticeKind, LocalAxis,
    OrientedFeature, PointFeature, SquareAlgorithm, TopologicalParams,
};

fn perspective_warped_grid(rows: i32, cols: i32, scale: f32) -> Vec<OrientedFeature<f32, 2>> {
    use nalgebra::Matrix3;

    let h = Matrix3::new(
        1.0_f32 * scale,
        0.10 * scale,
        100.0,
        -0.05 * scale,
        0.95 * scale,
        80.0,
        0.0006,
        0.0004,
        1.0,
    );

    let mut features = Vec::with_capacity((rows * cols) as usize);
    for j in 0..rows {
        for i in 0..cols {
            let p = h * nalgebra::Vector3::new(i as f32, j as f32, 1.0);
            let pu = h * nalgebra::Vector3::new(i as f32 + 1.0, j as f32, 1.0);
            let pv = h * nalgebra::Vector3::new(i as f32, j as f32 + 1.0, 1.0);
            let here = Point2::new(p.x / p.z, p.y / p.z);
            let right = Point2::new(pu.x / pu.z, pu.y / pu.z);
            let down = Point2::new(pv.x / pv.z, pv.y / pv.z);
            let theta_u = (right.y - here.y).atan2(right.x - here.x);
            let theta_v = (down.y - here.y).atan2(down.x - here.x);
            let source_index = features.len();
            features.push(OrientedFeature::new(
                PointFeature::new(source_index, here),
                [
                    LocalAxis::new(theta_u, Some(0.05)),
                    LocalAxis::new(theta_v, Some(0.05)),
                ],
            ));
        }
    }
    features
}

fn inject_noise(features: &mut Vec<OrientedFeature<f32, 2>>, extra: usize, seed: u64) {
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    );
    for feature in features.iter() {
        min_x = min_x.min(feature.point.position.x);
        min_y = min_y.min(feature.point.position.y);
        max_x = max_x.max(feature.point.position.x);
        max_y = max_y.max(feature.point.position.y);
    }
    let mut state = seed | 1;
    let mut next = || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((state >> 33) as u32) as f32 / u32::MAX as f32
    };
    for _ in 0..extra {
        let x = min_x + next() * (max_x - min_x);
        let y = min_y + next() * (max_y - min_y);
        let theta = next() * std::f32::consts::PI;
        let source_index = features.len();
        features.push(OrientedFeature::new(
            PointFeature::new(source_index, Point2::new(x, y)),
            [
                LocalAxis::new(theta, Some(0.05)),
                LocalAxis::new(theta + FRAC_PI_2, Some(0.05)),
            ],
        ));
    }
}

fn topological_params() -> DetectionParams<f32> {
    DetectionParams::default()
        .with_algorithm(SquareAlgorithm::Topological)
        .with_topological(TopologicalParams::default())
        .with_max_residual_px(f32::INFINITY)
}

fn bench_detect_grid_all_topological(c: &mut Criterion) {
    let mut group = c.benchmark_group("detect_grid_all/topological");
    let params = topological_params();

    for &(rows, cols) in &[(10, 10), (20, 20), (40, 40), (60, 60)] {
        let features = perspective_warped_grid(rows, cols, 50.0);
        group.bench_with_input(
            BenchmarkId::new("clean", format!("{rows}x{cols}")),
            &features,
            |b, features| {
                b.iter(|| {
                    let request = DetectionRequest::new(
                        LatticeKind::Square,
                        Evidence::Oriented2(black_box(features)),
                        None,
                        black_box(params.clone()),
                    );
                    let report = detect_grid_all(request).expect("synthetic grid");
                    black_box(
                        report
                            .solutions
                            .iter()
                            .map(|solution| solution.grid.entries.len())
                            .sum::<usize>(),
                    )
                });
            },
        );
    }

    let mut noisy = perspective_warped_grid(20, 20, 50.0);
    inject_noise(&mut noisy, 200, 0xC0FFEE);
    group.bench_function(BenchmarkId::new("noisy", "20x20+50pct"), |b| {
        b.iter(|| {
            let request = DetectionRequest::new(
                LatticeKind::Square,
                Evidence::Oriented2(black_box(&noisy)),
                None,
                black_box(params.clone()),
            );
            let report = detect_grid_all(request).expect("synthetic grid");
            black_box(
                report
                    .solutions
                    .iter()
                    .map(|solution| solution.grid.entries.len())
                    .sum::<usize>(),
            )
        });
    });

    group.finish();
}

criterion_group!(benches, bench_detect_grid_all_topological);
criterion_main!(benches);
