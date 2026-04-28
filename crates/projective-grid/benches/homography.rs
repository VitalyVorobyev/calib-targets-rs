//! Criterion benches for homography estimation.
//!
//! Run with `cargo bench -p projective-grid --bench homography`.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use nalgebra::{Matrix3, Point2};
use projective_grid::homography::{
    estimate_homography, estimate_homography_with_quality, homography_from_4pt,
    homography_from_4pt_with_quality, Homography,
};

fn truth_homography() -> Homography<f32> {
    Homography::new(Matrix3::new(
        1.0, 0.2, 12.0, //
        -0.1, 0.9, 6.0, //
        0.0006, 0.0004, 1.0,
    ))
}

fn rect_grid(cols: usize, rows: usize, spacing: f32) -> (Vec<Point2<f32>>, Vec<Point2<f32>>) {
    let h = truth_homography();
    let mut src = Vec::with_capacity(cols * rows);
    let mut dst = Vec::with_capacity(cols * rows);
    for j in 0..rows {
        for i in 0..cols {
            let p = Point2::new(i as f32 * spacing, j as f32 * spacing);
            src.push(p);
            dst.push(h.apply(p));
        }
    }
    (src, dst)
}

fn bench_4pt(c: &mut Criterion) {
    let (src, dst) = rect_grid(2, 2, 100.0);
    let src_arr: [Point2<f32>; 4] = [src[0], src[1], src[2], src[3]];
    let dst_arr: [Point2<f32>; 4] = [dst[0], dst[1], dst[2], dst[3]];
    c.bench_function("homography_from_4pt", |b| {
        b.iter(|| {
            black_box(homography_from_4pt(
                black_box(&src_arr),
                black_box(&dst_arr),
            ))
        });
    });
    c.bench_function("homography_from_4pt_with_quality", |b| {
        b.iter(|| {
            black_box(homography_from_4pt_with_quality(
                black_box(&src_arr),
                black_box(&dst_arr),
            ))
        });
    });
}

fn bench_dlt(c: &mut Criterion) {
    let mut group = c.benchmark_group("estimate_homography");
    for &(cols, rows) in &[(3, 3), (5, 5), (10, 10), (15, 15)] {
        let (src, dst) = rect_grid(cols, rows, 50.0);
        let n = cols * rows;
        group.bench_function(format!("N={n}"), |b| {
            b.iter(|| black_box(estimate_homography(black_box(&src), black_box(&dst))));
        });
        group.bench_function(format!("N={n}+quality"), |b| {
            b.iter(|| {
                black_box(estimate_homography_with_quality(
                    black_box(&src),
                    black_box(&dst),
                ))
            });
        });
    }
    group.finish();
}

/// K values that match the per-cell hot path in
/// `extend_via_local_homography`: `min_k=4..8` and `k_nearest=8..12`
/// from `LocalExtensionParams` defaults and sweep variants. The bench
/// covers K=8 / 12 / 20 to bracket the realistic range.
fn bench_dlt_local_extension_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("estimate_homography_local_extension");
    let h = truth_homography();
    for &k in &[8usize, 12, 20] {
        // K points scattered on a grid scale comparable to the labelled
        // corners that feed the per-candidate fit. Use a fractional-row
        // grid so non-square K values still place points sanely.
        let mut src: Vec<Point2<f32>> = Vec::with_capacity(k);
        let cols = (k as f32).sqrt().ceil() as usize;
        for idx in 0..k {
            let i = idx % cols;
            let j = idx / cols;
            src.push(Point2::new(i as f32 * 30.0, j as f32 * 30.0));
        }
        let dst: Vec<Point2<f32>> = src.iter().map(|&p| h.apply(p)).collect();

        group.bench_function(format!("K={k}"), |b| {
            b.iter(|| black_box(estimate_homography(black_box(&src), black_box(&dst))));
        });
        group.bench_function(format!("K={k}+quality"), |b| {
            b.iter(|| {
                black_box(estimate_homography_with_quality(
                    black_box(&src),
                    black_box(&dst),
                ))
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_4pt,
    bench_dlt,
    bench_dlt_local_extension_sizes
);
criterion_main!(benches);
