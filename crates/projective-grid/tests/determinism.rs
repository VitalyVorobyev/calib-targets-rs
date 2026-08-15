//! Detection must depend only on its input, never on hash iteration order.
//!
//! `std`'s `RandomState` draws a fresh key pair for *every* `HashMap`, so two
//! maps built from the same keys inside one process already iterate in
//! different orders — which is what makes this testable in-process rather than
//! by spawning binaries. Any reduction over a labelled set that is not
//! order-invariant (an `f32` sum, a greedy first-come claim) will therefore
//! produce different labellings across repeats, exactly as it does across
//! processes for a downstream consumer.
//!
//! Regression guard for
//! [#77](https://github.com/VitalyVorobyev/calib-targets-rs/issues/77), where a
//! 24 × 24 dot grid labelled 576/576 on most runs and ~455/576 on roughly one
//! in thirty, from identical input bytes.

use nalgebra::{Matrix3, Point2, Vector3};
use projective_grid::{
    detect_grid_all, DetectionRequest, Evidence, GridDimensions, LatticeKind, PointFeature,
};

/// Deterministic jitter, so the feature set is byte-identical every repeat.
struct Lcg(u64);

impl Lcg {
    fn jitter(&mut self, amp: f32) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let unit = ((self.0 >> 40) as f32) / ((1u32 << 24) as f32);
        (unit * 2.0 - 1.0) * amp
    }
}

/// A synthetic dot grid: `side × side` points at `pitch`, through a homography
/// with rotation and a perspective term, with sub-pixel centre noise. Every
/// `drop_every`-th point is omitted so the recovery schedule has real work —
/// that is where the order-dependent reductions live.
fn dot_grid(
    side: i32,
    pitch: f32,
    persp: f32,
    rot: f32,
    noise: f32,
    drop_every: usize,
) -> Vec<PointFeature> {
    let (c, s) = (rot.cos(), rot.sin());
    let h = Matrix3::new(c, -s, 0.0, s, c, 0.0, persp, persp * 0.6, 1.0);
    let mut rng = Lcg(0x5DEE_CE66_D1CE_F00D);
    let mut features = Vec::new();
    let mut n = 0usize;
    for j in 0..side {
        for i in 0..side {
            let (jx, jy) = (rng.jitter(noise), rng.jitter(noise));
            n += 1;
            if drop_every > 0 && n.is_multiple_of(drop_every) {
                continue;
            }
            let v = h * Vector3::new(i as f32 * pitch + 60.0, j as f32 * pitch + 60.0, 1.0);
            let p = Point2::new(v.x / v.z + jx, v.y / v.z + jy);
            features.push(PointFeature::new(features.len(), p));
        }
    }
    features
}

/// The full labelling, in a form that compares exactly: every component's
/// `(source index → (u, v))` pairs, sorted.
fn labelling(features: &[PointFeature], side: i32) -> Vec<Vec<(usize, (i32, i32))>> {
    let request = DetectionRequest::new(LatticeKind::Square, Evidence::Positions(features))
        .with_dimensions(GridDimensions::new(side as usize, side as usize));
    let mut components: Vec<Vec<(usize, (i32, i32))>> = detect_grid_all(request)
        .unwrap_or_default()
        .iter()
        .map(|detection| {
            let mut entries: Vec<(usize, (i32, i32))> = detection
                .grid()
                .entries()
                .iter()
                .map(|e| (e.source_index, (e.coord.u, e.coord.v)))
                .collect();
            entries.sort_unstable();
            entries
        })
        .collect();
    components.sort_unstable();
    components
}

fn assert_stable(name: &str, features: &[PointFeature], side: i32, repeats: usize) {
    let first = labelling(features, side);
    let first_total: usize = first.iter().map(Vec::len).sum();
    for repeat in 1..repeats {
        let again = labelling(features, side);
        if again != first {
            let total: usize = again.iter().map(Vec::len).sum();
            panic!(
                "{name}: repeat {repeat} of {repeats} produced a different labelling from the \
                 same input — {} corners in {} component(s) vs {first_total} in {} — some \
                 reduction is reading hash iteration order",
                total,
                again.len(),
                first.len(),
            );
        }
    }
}

#[test]
fn square_positions_labelling_is_order_stable() {
    // The geometry from the issue: 24 × 24 at ~36 px, mild perspective and
    // rotation, sub-0.1 px noise.
    //
    // The last case is the measured witness. A sweep of 144 regimes (varying
    // perspective, rotation, noise and dropout) found exactly one that was not
    // repeat-stable before the fix, and it flipped hard: over 40 repeats of the
    // *same* input it returned four different labellings, totalling 193, 200,
    // 277 and 386 corners. The first two cases are the issue's own geometry and
    // are stable either way — they are here so a future regression that widens
    // the unstable set is caught as well.
    const SIDE: i32 = 24;
    let cases = [
        ("clean", 0.000_18f32, 0.03f32, 0.05f32, 0usize, 30usize),
        ("dropouts", 0.000_18, 0.03, 0.05, 17, 30),
        // The witness gets more repeats because the flip is probabilistic: each
        // repeat draws a fresh hash order, and only some of them expose the
        // unstable reduction. 150 repeats failed on 6 of 6 trials against the
        // unfixed code, where 40 caught it only intermittently.
        (
            "witness: strong perspective + dropouts",
            0.000_9,
            0.03,
            0.05,
            7,
            150,
        ),
    ];
    for (name, persp, rot, noise, drop_every, repeats) in cases {
        let features = dot_grid(SIDE, 36.0, persp, rot, noise, drop_every);
        assert_stable(name, &features, SIDE, repeats);
    }
}
