//! Criterion bench for [`square::validate::validate`].
//!
//! Run with `cargo bench -p projective-grid --bench validate`.
//!
//! Drives the post-grow validation pass over a synthetic perspective-
//! warped grid. Two scenarios:
//!
//! - **clean**: every labelled corner sits exactly on the rectified
//!   grid. Validation should produce an empty blacklist.
//! - **with_outliers**: a fraction of labelled corners are offset by
//!   `0.4 × cell_size` to trigger line-fit and local-H residual flags.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use nalgebra::Point2;
use projective_grid::square::validate::{validate, LabelledEntry, ValidationParams};

fn perspective_warped_entries(rows: i32, cols: i32, scale: f32) -> Vec<LabelledEntry> {
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
    let mut entries = Vec::with_capacity((rows * cols) as usize);
    for j in 0..rows {
        for i in 0..cols {
            let p = h * nalgebra::Vector3::new(i as f32, j as f32, 1.0);
            let pixel = Point2::new(p.x / p.z, p.y / p.z);
            entries.push(LabelledEntry {
                idx: entries.len(),
                pixel,
                grid: (i, j),
            });
        }
    }
    entries
}

fn perturb(entries: &mut [LabelledEntry], cell_size: f32, fraction: f32, seed: u64) {
    let mut state = seed | 1;
    let mut next = || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((state >> 33) as u32) as f32 / u32::MAX as f32
    };
    let target_count = (entries.len() as f32 * fraction) as usize;
    for _ in 0..target_count {
        let i = (next() * entries.len() as f32) as usize % entries.len();
        let dx = (next() * 2.0 - 1.0) * 0.4 * cell_size;
        let dy = (next() * 2.0 - 1.0) * 0.4 * cell_size;
        entries[i].pixel.x += dx;
        entries[i].pixel.y += dy;
    }
}

fn bench_validate(c: &mut Criterion) {
    let mut group = c.benchmark_group("validate");
    let params = ValidationParams::default();

    for &(rows, cols) in &[(10, 10), (20, 20), (40, 40), (60, 60)] {
        let cell_size = 50.0_f32; // matches scale in perspective_warped_entries
        let entries = perspective_warped_entries(rows, cols, cell_size);

        group.bench_with_input(
            BenchmarkId::new("clean", format!("{rows}x{cols}")),
            &entries,
            |b, entries| {
                b.iter(|| {
                    let res =
                        validate(black_box(entries), black_box(cell_size), black_box(&params));
                    black_box(res.blacklist.len())
                });
            },
        );

        let mut perturbed = entries.clone();
        perturb(&mut perturbed, cell_size, 0.10, 0xCAFEBABE);
        group.bench_with_input(
            BenchmarkId::new("with_outliers", format!("{rows}x{cols}")),
            &perturbed,
            |b, entries| {
                b.iter(|| {
                    let res =
                        validate(black_box(entries), black_box(cell_size), black_box(&params));
                    black_box(res.blacklist.len())
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_validate);
criterion_main!(benches);
