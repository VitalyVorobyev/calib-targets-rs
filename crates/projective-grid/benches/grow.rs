//! Criterion benches for the BFS-grow hot path.
//!
//! Run with `cargo bench -p projective-grid --bench grow`.
//!
//! The fixtures are synthetic perspective-warped grids that exercise the
//! same code paths as the production chessboard detector — without
//! depending on any specific image data.

use std::collections::HashMap;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use nalgebra::{Point2, Vector2};
use projective_grid::square::grow::{
    bfs_grow, predict_from_neighbours, Admit, GrowParams, GrowValidator, LabelledNeighbour, Seed,
};

/// Generate a synthetic perspective-warped grid:
/// - Rectified grid of `rows × cols` corners with unit spacing.
/// - Apply a homography that does mild rotation + perspective foreshortening
///   (left side of the image is closer-to-camera, right side is farther).
fn perspective_warped_grid(rows: i32, cols: i32, scale: f32) -> (Vec<Point2<f32>>, [usize; 4]) {
    use nalgebra::Matrix3;
    // Mild perspective: top-down view at ~25° tilt.
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
    let mut seed_idx = [0usize; 4];
    for j in 0..rows {
        for i in 0..cols {
            let p = h * nalgebra::Vector3::new(i as f32, j as f32, 1.0);
            let pt = Point2::new(p.x / p.z, p.y / p.z);
            let k = points.len();
            points.push(pt);
            if (i, j) == (1, 1) {
                seed_idx[0] = k;
            }
            if (i, j) == (2, 1) {
                seed_idx[1] = k;
            }
            if (i, j) == (1, 2) {
                seed_idx[2] = k;
            }
            if (i, j) == (2, 2) {
                seed_idx[3] = k;
            }
        }
    }
    (points, seed_idx)
}

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

fn bench_bfs_grow(c: &mut Criterion) {
    let mut group = c.benchmark_group("bfs_grow");
    for &(rows, cols) in &[(10, 10), (20, 20), (40, 40)] {
        let (points, seed_idx) = perspective_warped_grid(rows, cols, 50.0);
        let cell_size = (points[seed_idx[1]] - points[seed_idx[0]]).norm();
        let seed = Seed {
            a: seed_idx[0],
            b: seed_idx[1],
            c: seed_idx[2],
            d: seed_idx[3],
        };
        let params = GrowParams::default();
        group.bench_function(format!("{rows}x{cols}"), |b| {
            b.iter(|| {
                let res = bfs_grow(
                    black_box(&points),
                    black_box(seed),
                    black_box(cell_size),
                    black_box(&params),
                    black_box(&OpenValidator),
                );
                black_box(res.labelled.len())
            });
        });
    }
    group.finish();
}

fn bench_predict_from_neighbours(c: &mut Criterion) {
    // 8-neighbour patch (max in BFS): all 8 cells around target labelled.
    let target = (5, 5);
    let mut neighbours: Vec<LabelledNeighbour> = Vec::with_capacity(8);
    let mut labelled = HashMap::new();
    let mut positions: Vec<Point2<f32>> = Vec::new();
    for dj in -1..=1_i32 {
        for di in -1..=1_i32 {
            if di == 0 && dj == 0 {
                continue;
            }
            let at = (target.0 + di, target.1 + dj);
            let pos = Point2::new(at.0 as f32 * 10.0, at.1 as f32 * 10.0);
            let idx = positions.len();
            positions.push(pos);
            labelled.insert(at, idx);
            neighbours.push(LabelledNeighbour {
                idx,
                at,
                position: pos,
            });
        }
    }
    let u = Vector2::new(1.0, 0.0);
    let v = Vector2::new(0.0, 1.0);
    let cell_size = 10.0;
    c.bench_function("predict_from_neighbours/8nb", |b| {
        b.iter(|| {
            black_box(predict_from_neighbours(
                black_box(target),
                black_box(&neighbours),
                black_box(u),
                black_box(v),
                black_box(cell_size),
                black_box(&labelled),
                black_box(&positions),
            ))
        });
    });
}

criterion_group!(benches, bench_bfs_grow, bench_predict_from_neighbours);
criterion_main!(benches);
