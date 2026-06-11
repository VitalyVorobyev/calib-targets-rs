//! Criterion benches for the public [`detect_grid_all`] entry point across the
//! three canonical lattice/evidence cells, for perf-regression tracking:
//!
//! - `(Square, Oriented2)` — the native two-axis square input both algorithms
//!   consume directly.
//! - `(Square, Positions)` — orientation-free square input; the per-corner
//!   axes are synthesized from neighbour geometry before the algorithm runs.
//! - `(Hex, Positions)` — orientation-free hexagonal input; three axis
//!   families synthesized, then the hex topological path runs.
//!
//! Fixtures are deterministic: a seeded xorshift LCG drives the position
//! jitter so there is no `rand` dependency and run-to-run numbers are
//! comparable. Each fixture is a single perspective-warped grid sized so the
//! whole suite stays well under 30 s.
//!
//! Run with:
//! ```text
//! cargo bench -p projective-grid --bench detect_grid
//! ```

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use nalgebra::{Matrix3, Point2, Vector3};
use projective_grid::{
    detect_grid_all, DetectionParams, DetectionRequest, Evidence, LatticeKind, LocalAxis,
    OrientedFeature, PointFeature, SquareAlgorithm,
};

/// Deterministic xorshift64* LCG — used only to jitter fixture positions so
/// runs are reproducible without pulling in `rand`.
struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    /// Next `f32` in `[-1.0, 1.0)`.
    fn next_signed(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        let bits = x.wrapping_mul(0x2545_F491_4F6C_DD1D);
        // Top 24 bits → [0, 1), then map to [-1, 1).
        let unit = ((bits >> 40) as f32) / ((1u32 << 24) as f32);
        unit * 2.0 - 1.0
    }
}

/// A mild perspective homography (image-plane warp) shared by all fixtures so
/// the synthesized-axis paths exercise non-orthogonal, spatially-varying local
/// grid directions.
fn perspective() -> Matrix3<f32> {
    Matrix3::new(
        1.05, 0.08, 12.0, //
        -0.06, 0.97, 9.0, //
        0.000_22, 0.000_15, 1.0,
    )
}

/// Project a model point through `h` into image pixels.
fn project(h: &Matrix3<f32>, x: f32, y: f32) -> Point2<f32> {
    let g = h * Vector3::new(x, y, 1.0);
    Point2::new(g.x / g.z, g.y / g.z)
}

/// Square grid as position-only features, perspective-warped with seeded
/// sub-pixel jitter.
fn square_positions(rows: i32, cols: i32, spacing: f32, jitter: f32) -> Vec<PointFeature> {
    let h = perspective();
    let mut lcg = Lcg::new(0x0514_71de_5b00_b1e5);
    let mut out = Vec::with_capacity((rows * cols) as usize);
    let mut idx = 0usize;
    for j in 0..rows {
        for i in 0..cols {
            let mut p = project(&h, i as f32 * spacing + 40.0, j as f32 * spacing + 40.0);
            p.x += jitter * lcg.next_signed();
            p.y += jitter * lcg.next_signed();
            out.push(PointFeature::new(idx, p));
            idx += 1;
        }
    }
    out
}

/// Square grid as native two-axis oriented features. The axes are the exact
/// image-space grid directions at each corner (forward differences of the
/// warped lattice), matching what a chess-corner detector would supply.
fn square_oriented2(rows: i32, cols: i32, spacing: f32) -> Vec<OrientedFeature<2>> {
    let h = perspective();
    let mut out = Vec::with_capacity((rows * cols) as usize);
    let mut idx = 0usize;
    for j in 0..rows {
        for i in 0..cols {
            let here = project(&h, i as f32 * spacing + 40.0, j as f32 * spacing + 40.0);
            let along_i = project(
                &h,
                (i + 1) as f32 * spacing + 40.0,
                j as f32 * spacing + 40.0,
            );
            let along_j = project(
                &h,
                i as f32 * spacing + 40.0,
                (j + 1) as f32 * spacing + 40.0,
            );
            let a0 = (along_i.y - here.y).atan2(along_i.x - here.x);
            let a1 = (along_j.y - here.y).atan2(along_j.x - here.x);
            out.push(OrientedFeature::new(
                PointFeature::new(idx, here),
                [LocalAxis::new(a0, None), LocalAxis::new(a1, None)],
            ));
            idx += 1;
        }
    }
    out
}

/// Hexagonal lattice (axial `(q, r)` within `radius`) as position-only
/// features, perspective-warped with seeded sub-pixel jitter.
fn hex_positions(radius: i32, spacing: f32, jitter: f32) -> Vec<PointFeature> {
    let h = perspective();
    let sqrt3_2 = 3.0_f32.sqrt() * 0.5;
    let mut lcg = Lcg::new(0xb1c7_0a55_d00d_feed);
    let mut out = Vec::new();
    let mut idx = 0usize;
    for q in -radius..=radius {
        for r in (-radius).max(-q - radius)..=radius.min(-q + radius) {
            let mx = (q as f32 + 0.5 * r as f32) * spacing + 200.0;
            let my = (sqrt3_2 * r as f32) * spacing + 200.0;
            let mut p = project(&h, mx, my);
            p.x += jitter * lcg.next_signed();
            p.y += jitter * lcg.next_signed();
            out.push(PointFeature::new(idx, p));
            idx += 1;
        }
    }
    out
}

fn bench_detect_grid(c: &mut Criterion) {
    let mut group = c.benchmark_group("detect_grid_all");

    // (Square, Oriented2) — both algorithm choices.
    let sq2 = square_oriented2(16, 16, 22.0);
    for algo in [SquareAlgorithm::SeedAndGrow, SquareAlgorithm::Topological] {
        let label = match algo {
            SquareAlgorithm::SeedAndGrow => "seed_and_grow",
            SquareAlgorithm::Topological => "topological",
            _ => "unknown",
        };
        group.bench_with_input(
            BenchmarkId::new("square_oriented2", label),
            &sq2,
            |b, feats| {
                b.iter(|| {
                    let req = DetectionRequest::new(
                        LatticeKind::Square,
                        Evidence::Oriented2(feats),
                        None,
                        DetectionParams::default().with_algorithm(algo),
                    );
                    detect_grid_all(req).unwrap()
                });
            },
        );
    }

    // (Square, Positions) — orientation-free, default (seed-and-grow) algorithm.
    let sqp = square_positions(16, 16, 22.0, 0.15);
    group.bench_with_input(
        BenchmarkId::new("square_positions", "seed_and_grow"),
        &sqp,
        |b, feats| {
            b.iter(|| {
                let req = DetectionRequest::new(
                    LatticeKind::Square,
                    Evidence::Positions(feats),
                    None,
                    DetectionParams::default(),
                );
                detect_grid_all(req).unwrap()
            });
        },
    );

    // (Hex, Positions) — orientation-free hex, topological path.
    let hexp = hex_positions(6, 22.0, 0.15);
    group.bench_with_input(
        BenchmarkId::new("hex_positions", "topological"),
        &hexp,
        |b, feats| {
            b.iter(|| {
                let req = DetectionRequest::new(
                    LatticeKind::Hex,
                    Evidence::Positions(feats),
                    None,
                    DetectionParams::default().with_algorithm(SquareAlgorithm::Topological),
                );
                detect_grid_all(req).unwrap()
            });
        },
    );

    group.finish();
}

criterion_group!(benches, bench_detect_grid);
criterion_main!(benches);
