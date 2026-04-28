//! Criterion bench for [`merge_components_local`].
//!
//! Run with `cargo bench -p projective-grid --bench merge`.
//!
//! Splits a synthetic perspective-warped grid into two or three
//! components by deleting strips of corners and re-labelling each
//! surviving component to start at `(0, 0)` (mirrors what the topological
//! pipeline outputs in real failures: occluded rows / columns).

use std::collections::HashMap;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use nalgebra::Point2;
use projective_grid::component_merge::{merge_components_local, ComponentInput, LocalMergeParams};

type Labels = HashMap<(i32, i32), usize>;
type Fixture = (Vec<Point2<f32>>, Vec<Labels>);

fn perspective_warped_grid(rows: i32, cols: i32, scale: f32) -> Vec<Point2<f32>> {
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
    let mut points = Vec::with_capacity((rows * cols) as usize);
    for j in 0..rows {
        for i in 0..cols {
            let p = h * nalgebra::Vector3::new(i as f32, j as f32, 1.0);
            points.push(Point2::new(p.x / p.z, p.y / p.z));
        }
    }
    points
}

/// Build a labelled map for a `(rows × cols)` block whose positions
/// occupy slot indices `start..start+rows*cols` in the shared positions
/// slice. Labels are rebased so the bounding-box minimum is `(0, 0)`.
fn labelled_block(start: usize, rows: i32, cols: i32) -> Labels {
    let mut out = HashMap::new();
    for j in 0..rows {
        for i in 0..cols {
            let idx = start + (j * cols + i) as usize;
            out.insert((i, j), idx);
        }
    }
    out
}

/// Build two components that overlap in label space (the ground truth
/// alignment is identity — `merge_components_local` should accept it).
fn two_overlapping_components() -> Fixture {
    // Single 30×30 grid; component A keeps rows 0..20, component B
    // keeps rows 10..30 — 10 overlapping rows.
    let positions = perspective_warped_grid(30, 30, 50.0);
    let mut comp_a = HashMap::new();
    let mut comp_b = HashMap::new();
    for j in 0..30_i32 {
        for i in 0..30_i32 {
            let idx = (j * 30 + i) as usize;
            if j < 20 {
                comp_a.insert((i, j), idx);
            }
            if j >= 10 {
                // Component B uses the same indices but is rebased to
                // start at (0, 0) — mirrors what `topological::walk`
                // emits per component.
                comp_b.insert((i, j - 10), idx);
            }
        }
    }
    (positions, vec![comp_a, comp_b])
}

/// Build three components: A (top), B (middle), C (bottom). All three
/// share boundary rows with their neighbour for non-trivial merging.
fn three_overlapping_components() -> Fixture {
    let positions = perspective_warped_grid(30, 30, 50.0);
    let mut comp_a = HashMap::new();
    let mut comp_b = HashMap::new();
    let mut comp_c = HashMap::new();
    for j in 0..30_i32 {
        for i in 0..30_i32 {
            let idx = (j * 30 + i) as usize;
            if j < 12 {
                comp_a.insert((i, j), idx);
            }
            if (8..22).contains(&j) {
                comp_b.insert((i, j - 8), idx);
            }
            if j >= 18 {
                comp_c.insert((i, j - 18), idx);
            }
        }
    }
    (positions, vec![comp_a, comp_b, comp_c])
}

fn bench_merge(c: &mut Criterion) {
    let mut group = c.benchmark_group("merge_components_local");
    let params = LocalMergeParams::default();

    {
        let (positions, comps) = two_overlapping_components();
        group.bench_function(BenchmarkId::new("overlap", "2_components"), |b| {
            b.iter(|| {
                let inputs: Vec<ComponentInput<'_>> = comps
                    .iter()
                    .map(|labelled| ComponentInput {
                        labelled,
                        positions: &positions,
                    })
                    .collect();
                let res = merge_components_local(black_box(&inputs), black_box(&params));
                black_box(res.diagnostics.components_out)
            });
        });
    }

    {
        let (positions, comps) = three_overlapping_components();
        group.bench_function(BenchmarkId::new("overlap", "3_components"), |b| {
            b.iter(|| {
                let inputs: Vec<ComponentInput<'_>> = comps
                    .iter()
                    .map(|labelled| ComponentInput {
                        labelled,
                        positions: &positions,
                    })
                    .collect();
                let res = merge_components_local(black_box(&inputs), black_box(&params));
                black_box(res.diagnostics.components_out)
            });
        });
    }

    // Larger workload — 60×60 grid split in half.
    {
        let positions = perspective_warped_grid(60, 60, 50.0);
        let comp_a = labelled_block(0, 32, 60);
        let mut comp_b = HashMap::new();
        for j in 28..60_i32 {
            for i in 0..60_i32 {
                let idx = (j * 60 + i) as usize;
                comp_b.insert((i, j - 28), idx);
            }
        }
        let comps = [comp_a, comp_b];
        group.bench_function(BenchmarkId::new("overlap", "2_components_large"), |b| {
            b.iter(|| {
                let inputs: Vec<ComponentInput<'_>> = comps
                    .iter()
                    .map(|labelled| ComponentInput {
                        labelled,
                        positions: &positions,
                    })
                    .collect();
                let res = merge_components_local(black_box(&inputs), black_box(&params));
                black_box(res.diagnostics.components_out)
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_merge);
criterion_main!(benches);
