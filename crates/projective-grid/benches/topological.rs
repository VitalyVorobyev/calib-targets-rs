//! End-to-end Criterion bench for the topological grid pipeline.
//!
//! Run with `cargo bench -p projective-grid --bench topological`.
//!
//! Drives [`build_grid_topological`] (Delaunay → classify_all_edges →
//! merge_triangle_pairs → filter_quads → label_components) on synthetic
//! perspective-warped grids of increasing size, plus a noisy variant that
//! injects unaligned background corners — the realistic stressor for the
//! per-edge axis test in `topological::classify`.

use std::f32::consts::FRAC_PI_2;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use nalgebra::Point2;
use projective_grid::topological::{build_grid_topological, AxisHint, TopologicalParams};

/// Build a perspective-warped chessboard-cell corner cloud of size
/// `rows × cols`. Returns positions plus per-corner axes aligned with
/// the local grid directions (which is what the topological pipeline
/// expects).
fn perspective_warped_grid(
    rows: i32,
    cols: i32,
    scale: f32,
) -> (Vec<Point2<f32>>, Vec<[AxisHint; 2]>) {
    use nalgebra::Matrix3;
    // Same homography as `benches/grow.rs` so results are comparable.
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

    let mut positions = Vec::with_capacity((rows * cols) as usize);
    let mut axes = Vec::with_capacity((rows * cols) as usize);

    // Two unit basis vectors in board space — push them through H to get
    // the per-corner local grid directions in pixel space. We use a
    // forward-difference numerical estimate for simplicity (the real
    // detector reads ChESS axes, but here we just want the topological
    // pipeline to see plausible angles).
    for j in 0..rows {
        for i in 0..cols {
            let p = h * nalgebra::Vector3::new(i as f32, j as f32, 1.0);
            let pu = h * nalgebra::Vector3::new(i as f32 + 1.0, j as f32, 1.0);
            let pv = h * nalgebra::Vector3::new(i as f32, j as f32 + 1.0, 1.0);
            let here = Point2::new(p.x / p.z, p.y / p.z);
            let right = Point2::new(pu.x / pu.z, pu.y / pu.z);
            let down = Point2::new(pv.x / pv.z, pv.y / pv.z);
            positions.push(here);

            let theta_u = (right.y - here.y).atan2(right.x - here.x);
            let theta_v = (down.y - here.y).atan2(down.x - here.x);
            axes.push([
                AxisHint {
                    angle: theta_u,
                    sigma: 0.05,
                },
                AxisHint {
                    angle: theta_v,
                    sigma: 0.05,
                },
            ]);
        }
    }

    (positions, axes)
}

/// Inject `extra` noise corners with random-looking positions and axis
/// angles inside the bounding box of `positions`. Uses a deterministic
/// LCG so results are reproducible across runs.
fn inject_noise(
    positions: &mut Vec<Point2<f32>>,
    axes: &mut Vec<[AxisHint; 2]>,
    extra: usize,
    seed: u64,
) {
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    );
    for p in positions.iter() {
        min_x = min_x.min(p.x);
        min_y = min_y.min(p.y);
        max_x = max_x.max(p.x);
        max_y = max_y.max(p.y);
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
        // Random axis offset in [0, π) — explicitly NOT aligned with the
        // grid so the topological classifier sees a Spurious edge.
        let theta = next() * std::f32::consts::PI;
        positions.push(Point2::new(x, y));
        axes.push([
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

fn bench_build_grid_topological(c: &mut Criterion) {
    let mut group = c.benchmark_group("build_grid_topological");
    let params = TopologicalParams::default();

    for &(rows, cols) in &[(10, 10), (20, 20), (40, 40), (60, 60)] {
        let (positions, axes) = perspective_warped_grid(rows, cols, 50.0);
        group.bench_with_input(
            BenchmarkId::new("clean", format!("{rows}x{cols}")),
            &(positions, axes),
            |b, (positions, axes)| {
                b.iter(|| {
                    let res = build_grid_topological(
                        black_box(positions),
                        black_box(axes),
                        black_box(&params),
                    )
                    .expect("synthetic grids should always succeed");
                    black_box(res.diagnostics.quads_kept)
                });
            },
        );
    }

    // Noisy stressor: 20×20 + 50% noise corners. Mirrors what real
    // images look like to the topological pipeline (lots of background
    // ChESS corners that produce Spurious Delaunay edges).
    let (mut positions, mut axes) = perspective_warped_grid(20, 20, 50.0);
    inject_noise(&mut positions, &mut axes, 200, 0xC0FFEE);
    group.bench_function(BenchmarkId::new("noisy", "20x20+50pct"), |b| {
        b.iter(|| {
            let res =
                build_grid_topological(black_box(&positions), black_box(&axes), black_box(&params))
                    .expect("synthetic grids should always succeed");
            black_box(res.diagnostics.quads_kept)
        });
    });

    group.finish();
}

criterion_group!(benches, bench_build_grid_topological);
criterion_main!(benches);
