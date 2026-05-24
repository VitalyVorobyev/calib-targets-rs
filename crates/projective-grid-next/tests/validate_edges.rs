//! Integration tests for the new per-edge precision gate
//! (`projective_grid_next::validate::edges`).
//!
//! Three scenarios, each run for both `f32` and `f64`:
//!
//! 1. A clean axis-aligned grid drops nothing.
//! 2. A 40 %-of-cell-size displaced corner is blacklisted with an
//!    `EdgeLengthOutOfBand` event.
//! 3. A flipped-axes interior corner is blacklisted with an
//!    `AxisSlotParityMismatch` event under chessboard parity.

use nalgebra::Point2;

use projective_grid_next::diagnostics::{Event, NoOpSink, RecordingSink, ValidationReason};
use projective_grid_next::feature::{AxisEstimate, Observation};
use projective_grid_next::float::Float;
use projective_grid_next::policy::{FeatureTag, LabelPolicy, ParityRule};
use projective_grid_next::validate::{validate, LabelledEntry, ValidationParams};

/// Convert an `f32` literal to `F`. Local copy of the helper in
/// `tests/common/mod.rs` — re-using that module would force every helper there
/// to be exercised by this test (otherwise dead-code lints fire under
/// `clippy --all-targets -D warnings`).
#[inline]
fn lit<F: Float>(v: f32) -> F {
    <F as From<f32>>::from(v)
}

/// Build a 5×5 chessboard with proper per-corner axes and parity tags.
///
/// Returns `(entries, observations)`. Row-major indexing: feature at `(i, j)`
/// lives at `idx = j * 5 + i`. Cell pitch is `s` pixels, origin `(50, 50)`.
fn chessboard_5x5<F: Float>(
    rows: i32,
    cols: i32,
    s: F,
) -> (Vec<LabelledEntry<F>>, Vec<Observation<F>>) {
    let origin = lit::<F>(50.0_f32);
    let pi = F::pi();
    let half_pi = pi / lit::<F>(2.0_f32);
    let mut entries = Vec::with_capacity((rows * cols) as usize);
    let mut obs = Vec::with_capacity((rows * cols) as usize);
    for j in 0..rows {
        for i in 0..cols {
            let idx = (j * cols + i) as usize;
            let x = lit::<F>(i as f32) * s + origin;
            let y = lit::<F>(j as f32) * s + origin;
            entries.push(LabelledEntry::new(idx, Point2::new(x, y), (i, j)));

            // Parity-0 corner (i + j even): axes[0] ≈ 0 (horizontal),
            // axes[1] ≈ π/2 (vertical).
            // Parity-1 corner (i + j odd):  axes[0] ≈ π/2 (vertical),
            // axes[1] ≈ π  (horizontal, undirected-equivalent to 0).
            let parity = ((i + j).rem_euclid(2)) as u32;
            let axes = if parity == 0 {
                [
                    AxisEstimate::<F>::from_angle(F::zero()),
                    AxisEstimate::<F>::from_angle(half_pi),
                ]
            } else {
                [
                    AxisEstimate::<F>::from_angle(half_pi),
                    AxisEstimate::<F>::from_angle(pi),
                ]
            };
            let tag = FeatureTag::new(parity);
            obs.push(
                Observation::<F>::new(Point2::new(x, y))
                    .with_axes(axes)
                    .with_tag(tag),
            );
        }
    }
    (entries, obs)
}

fn chessboard_policy<F: Float>(n: usize, entries: &[LabelledEntry<F>]) -> LabelPolicy<F> {
    // Mirror the observation tags into the policy.
    let mut builder =
        LabelPolicy::<F>::builder(n).with_parity_rule(ParityRule::Chessboard { shift: 0 });
    for e in entries {
        let parity = ((e.coord.0 + e.coord.1).rem_euclid(2)) as u32;
        builder = builder.with_tag(e.idx, FeatureTag::new(parity));
    }
    builder.build()
}

fn assert_clean_grid_no_drops<F: Float>() {
    let s = lit::<F>(20.0_f32);
    let (entries, obs) = chessboard_5x5::<F>(7, 7, s);
    let policy = chessboard_policy::<F>(obs.len(), &entries);
    let mut sink = NoOpSink;
    let result = validate(
        &entries,
        &obs,
        s,
        &policy,
        &ValidationParams::default(),
        &mut sink,
    );
    assert!(
        result.blacklist.is_empty(),
        "clean grid produced unexpected drops: {:?}",
        result.blacklist
    );
    assert!(
        result.edge_failures.is_empty(),
        "{:?}",
        result.edge_failures
    );
}

fn assert_edge_length_outlier_is_blacklisted<F: Float>() {
    let s = lit::<F>(20.0_f32);
    let (mut entries, obs) = chessboard_5x5::<F>(5, 5, s);

    // Displace (2, 2) by +40 % of cell size along x. That makes the edges
    // (1,2)↔(2,2) (length 1.4 s) and (2,2)↔(3,2) (length 0.6 s) fall outside
    // the default band [1/1.35, 1.35]; vertical edges to (2,1) and (2,3)
    // grow to sqrt(s² + 0.16 s²) ≈ 1.077 s, which stays in band.
    let target_idx = entries
        .iter()
        .find(|e| e.coord == (2, 2))
        .map(|e| e.idx)
        .expect("(2,2) present");
    for e in entries.iter_mut() {
        if e.idx == target_idx {
            e.position.x += lit::<F>(0.40_f32) * s;
        }
    }

    let policy = chessboard_policy::<F>(obs.len(), &entries);
    let mut sink: RecordingSink<F> = RecordingSink::new();
    let result = validate(
        &entries,
        &obs,
        s,
        &policy,
        &ValidationParams::default(),
        &mut sink,
    );

    assert!(
        result.blacklist.contains(&target_idx),
        "expected {target_idx} blacklisted, got {:?}",
        result.blacklist
    );

    let has_edge_event = sink.events().iter().any(|ev| {
        matches!(
            ev,
            Event::ValidationDropped {
                reason: ValidationReason::EdgeLengthOutOfBand { .. },
                ..
            }
        )
    });
    assert!(
        has_edge_event,
        "expected at least one EdgeLengthOutOfBand event in {} events",
        sink.events().len()
    );

    // The recorded failure must reference the worst edge from (2, 2)'s POV.
    let failure = result
        .edge_failures
        .get(&target_idx)
        .expect("edge failure recorded for the displaced corner");
    // Ratio must sit outside the active band.
    let one = F::one();
    let band = lit::<F>(0.35_f32);
    let low = one / (one + band);
    let high = one + band;
    assert!(
        failure.ratio < low || failure.ratio > high,
        "ratio {:?} must be outside the active band [{:?}, {:?}]",
        failure.ratio,
        low,
        high
    );
}

fn assert_axis_slot_parity_violation_is_blacklisted<F: Float>() {
    let s = lit::<F>(20.0_f32);
    let (entries, mut obs) = chessboard_5x5::<F>(5, 5, s);

    // Flip the axes at the interior parity-0 corner (2, 2): give it the
    // parity-1 axis pattern so all four neighbours pick the same axis slot
    // for their shared edge -> parity mismatch on every cardinal edge.
    let target_idx = entries
        .iter()
        .find(|e| e.coord == (2, 2))
        .map(|e| e.idx)
        .expect("(2,2) present");
    let pi = F::pi();
    let half_pi = pi / lit::<F>(2.0_f32);
    obs[target_idx] = Observation::<F>::new(obs[target_idx].position)
        .with_axes([
            AxisEstimate::<F>::from_angle(half_pi),
            AxisEstimate::<F>::from_angle(pi),
        ])
        .with_tag(FeatureTag::new(0));

    let policy = chessboard_policy::<F>(obs.len(), &entries);
    let mut sink: RecordingSink<F> = RecordingSink::new();
    let result = validate(
        &entries,
        &obs,
        s,
        &policy,
        &ValidationParams::default(),
        &mut sink,
    );

    assert!(
        result.blacklist.contains(&target_idx),
        "expected (2, 2) idx {target_idx} blacklisted, got {:?}",
        result.blacklist
    );

    let has_parity_event = sink.events().iter().any(|ev| {
        matches!(
            ev,
            Event::ValidationDropped {
                reason: ValidationReason::AxisSlotParityMismatch,
                ..
            }
        )
    });
    assert!(
        has_parity_event,
        "expected at least one AxisSlotParityMismatch event in {} events",
        sink.events().len()
    );
}

#[test]
fn clean_grid_no_drops_f32() {
    assert_clean_grid_no_drops::<f32>();
}
#[test]
fn clean_grid_no_drops_f64() {
    assert_clean_grid_no_drops::<f64>();
}
#[test]
fn edge_length_outlier_is_blacklisted_f32() {
    assert_edge_length_outlier_is_blacklisted::<f32>();
}
#[test]
fn edge_length_outlier_is_blacklisted_f64() {
    assert_edge_length_outlier_is_blacklisted::<f64>();
}
#[test]
fn axis_slot_parity_violation_is_blacklisted_f32() {
    assert_axis_slot_parity_violation_is_blacklisted::<f32>();
}
#[test]
fn axis_slot_parity_violation_is_blacklisted_f64() {
    assert_axis_slot_parity_violation_is_blacklisted::<f64>();
}
