//! Repro harness for issue #77 — `detect_grid` determinism on `(Square, Positions)`.
//!
//! Prints the labelled-corner count for one fixed synthetic input. The input is
//! deterministic (a fixed LCG, no clock, no RNG seeding from the environment),
//! so **every process must print the same number**. Hash-based iteration order
//! is the one thing that legitimately differs between processes, since
//! `std`'s `RandomState` reseeds per process — which is exactly why this has to
//! be run as many separate processes rather than a loop:
//!
//! ```text
//! for i in $(seq 200); do
//!   cargo run --release -q -p projective-grid --example determinism_probe
//! done | sort | uniq -c
//! ```
//!
//! A single line of output means the pipeline is order-stable on this input.
//!
//! Matches the geometry reported in the issue: a 24 × 24 grid (576
//! position-only features) at ~36 px pitch, mild perspective plus rotation, and
//! sub-0.1 px centre noise.

use nalgebra::{Matrix3, Point2, Vector3};
use projective_grid::{
    detect_grid_all, DetectionRequest, Evidence, GridDimensions, LatticeKind, PointFeature,
};

/// Deterministic sub-pixel jitter — a fixed LCG, so the feature set is
/// byte-identical in every process.
struct Lcg(u64);

impl Lcg {
    fn next_f32(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        // Top 24 bits → [0, 1).
        ((self.0 >> 40) as f32) / ((1u32 << 24) as f32)
    }

    /// Uniform in `[-amp, amp]`.
    fn jitter(&mut self, amp: f32) -> f32 {
        (self.next_f32() * 2.0 - 1.0) * amp
    }
}

/// One synthetic regime: perspective strength, rotation, centre noise, and how
/// many features are dropped (a partial view forces the recovery schedule to do
/// real work, which is where the order-dependent sites live).
struct Regime {
    persp: f32,
    rot: f32,
    noise_px: f32,
    drop_every: usize,
}

fn build(regime: &Regime, side: i32) -> Vec<PointFeature> {
    const PITCH: f32 = 36.0;
    const ORIGIN: f32 = 60.0;
    let (c, s) = (regime.rot.cos(), regime.rot.sin());
    let h = Matrix3::new(
        c,
        -s,
        0.0, //
        s,
        c,
        0.0, //
        regime.persp,
        regime.persp * 0.6,
        1.0,
    );
    let project = |x: f32, y: f32| -> Point2<f32> {
        let v = h * Vector3::new(x, y, 1.0);
        Point2::new(v.x / v.z, v.y / v.z)
    };
    let mut rng = Lcg(0x5DEE_CE66_D1CE_F00D);
    let mut features = Vec::new();
    let mut n = 0usize;
    for j in 0..side {
        for i in 0..side {
            let jx = rng.jitter(regime.noise_px);
            let jy = rng.jitter(regime.noise_px);
            n += 1;
            if regime.drop_every > 0 && n.is_multiple_of(regime.drop_every) {
                continue;
            }
            let p = project(i as f32 * PITCH + ORIGIN, j as f32 * PITCH + ORIGIN);
            features.push(PointFeature::new(
                features.len(),
                Point2::new(p.x + jx, p.y + jy),
            ));
        }
    }
    features
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    const SIDE: i32 = 24;

    let mut regimes = Vec::new();
    for &persp in &[0.0f32, 0.000_18, 0.000_45, 0.000_9] {
        for &rot in &[0.0f32, 0.03, 0.21] {
            for &noise in &[0.0f32, 0.05, 0.15, 0.35] {
                for &drop_every in &[0usize, 17, 7] {
                    regimes.push(Regime {
                        persp,
                        rot,
                        noise_px: noise,
                        drop_every,
                    });
                }
            }
        }
    }

    // In-process instability scan: `RandomState` draws fresh keys per `HashMap`,
    // so repeating a detection inside one process already varies hash order.
    // Report every regime whose labelling is not repeat-stable.
    let repeats: usize = std::env::var("REPEATS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    if repeats > 0 {
        let mut unstable = Vec::new();
        for (idx, regime) in regimes.iter().enumerate() {
            let features = build(regime, SIDE);
            let run = || -> Vec<usize> {
                let request =
                    DetectionRequest::new(LatticeKind::Square, Evidence::Positions(&features))
                        .with_dimensions(GridDimensions::new(SIDE as usize, SIDE as usize));
                let mut sizes: Vec<usize> = detect_grid_all(request)
                    .unwrap_or_default()
                    .iter()
                    .map(|d| d.grid().entries().len())
                    .collect();
                sizes.sort_unstable();
                sizes
            };
            let first = run();
            let mut seen = std::collections::BTreeSet::new();
            seen.insert(first.clone());
            for _ in 1..repeats {
                seen.insert(run());
            }
            if seen.len() > 1 {
                unstable.push((
                    idx,
                    regime.persp,
                    regime.rot,
                    regime.noise_px,
                    regime.drop_every,
                    seen,
                ));
            }
        }
        println!("unstable regimes: {}/{}", unstable.len(), regimes.len());
        for (idx, persp, rot, noise, drop, seen) in unstable.iter().take(8) {
            let totals: Vec<usize> = seen.iter().map(|s| s.iter().sum()).collect();
            println!(
                "  regime{idx}: persp={persp} rot={rot} noise={noise} drop_every={drop} totals={totals:?}"
            );
        }
        return Ok(());
    }

    let mut digest: u64 = 0xcbf2_9ce4_8422_2325;
    let mut worst: Option<(usize, usize)> = None;
    for (idx, regime) in regimes.iter().enumerate() {
        let features = build(regime, SIDE);
        let request = DetectionRequest::new(LatticeKind::Square, Evidence::Positions(&features))
            .with_dimensions(GridDimensions::new(SIDE as usize, SIDE as usize));
        let mut sizes: Vec<usize> = match detect_grid_all(request) {
            Ok(detections) => detections
                .iter()
                .map(|d| d.grid().entries().len())
                .collect(),
            Err(_) => Vec::new(),
        };
        sizes.sort_unstable();
        let total: usize = sizes.iter().sum();
        if worst.is_none_or(|(_, w)| total < w) {
            worst = Some((idx, total));
        }
        for value in std::iter::once(features.len()).chain(sizes) {
            digest ^= value as u64;
            digest = digest.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    let (worst_idx, worst_total) = worst.unwrap_or((0, 0));
    println!(
        "regimes={} digest={digest:016x} worst=regime{worst_idx}:{worst_total}",
        regimes.len()
    );
    Ok(())
}
