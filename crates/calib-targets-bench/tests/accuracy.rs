//! Regression gate on the ground-truth accuracy suite's quick subset.
//!
//! Renders every family's board under the four quick conditions with fixed
//! seeds — plus, for the marker board, a fifth guard condition whose disks are
//! blurred below resolution — runs both refiners, and checks the detection
//! contract and the measured accuracy:
//!
//! - **zero wrong labels**, every family, every refiner — the contract;
//! - detections, recall, worst RMS error and worst variance factor against
//!   the truth stay within a margin of their first measurement. These are
//!   regression guards, not acceptance criteria: each bound is the measured
//!   value (in the comment) plus a stated margin — recall −0.05 absolute,
//!   RMSE ×1.25 + 0.005 px, σ̂₀² ×1.5 + 0.05. A guard that trips is a change
//!   to explain (and re-measure), not a bound to loosen;
//! - the run is deterministic: two runs serialize identically, covariances
//!   included.
//!
//! The full grid is `cargo run -p calib-targets-bench --release --bin bench
//! -- accuracy`.

use calib_targets_bench::accuracy::families::Family;
use calib_targets_bench::accuracy::{quick_conditions, run, summarize, Refiner};

/// `(family, refiner, min detected of the family's quick conditions — 4, or 5
/// for the marker board — min mean recall, max RMSE px, max σ̂₀² truth)` —
/// measured values in the comments.
const BOUNDS: [(Family, Refiner, usize, f64, f64, f64); 8] = [
    // 4/4, recall 1.000, RMSE 0.202, σ̂₀² 1.78
    (
        Family::Chessboard,
        Refiner::CenterOfMass,
        4,
        0.95,
        0.258,
        2.72,
    ),
    // 4/4, recall 1.000, RMSE 0.047, σ̂₀² 0.28
    (
        Family::Chessboard,
        Refiner::SaddlePoint,
        4,
        0.95,
        0.064,
        0.47,
    ),
    // 2/4, recall 0.496, RMSE 0.194, σ̂₀² 1.47
    (
        Family::Charuco,
        Refiner::CenterOfMass,
        2,
        0.446,
        0.248,
        2.26,
    ),
    // 2/4, recall 0.500, RMSE 0.096, σ̂₀² 0.44 (re-measured when the
    // saddle-point refiner became the default with the center of mass for the
    // corners it rejects: the 24 px / 45° board that the rejecting refiner
    // alone detected now misses, as under the center of mass)
    (Family::Charuco, Refiner::SaddlePoint, 2, 0.450, 0.125, 0.71),
    // 4/4, recall 0.996, RMSE 0.379, σ̂₀² 5.57
    (
        Family::PuzzleBoard,
        Refiner::CenterOfMass,
        4,
        0.946,
        0.479,
        8.41,
    ),
    // 4/4, recall 0.996, RMSE 0.266, σ̂₀² 7.64 (re-measured with the
    // center-of-mass fallback: the corners the saddle-point fit rejects next
    // to an edge dot now count, at the center of mass's accuracy)
    (
        Family::PuzzleBoard,
        Refiner::SaddlePoint,
        4,
        0.946,
        0.338,
        11.51,
    ),
    // 3/5, recall 0.600, RMSE 0.203, σ̂₀² 1.80 (the unresolved-disk guard
    // condition declines; it delivered wrong labels before the empty-square
    // evidence test)
    (
        Family::MarkerBoard,
        Refiner::CenterOfMass,
        3,
        0.55,
        0.259,
        2.75,
    ),
    // 3/5, recall 0.600, RMSE 0.049, σ̂₀² 0.30
    (
        Family::MarkerBoard,
        Refiner::SaddlePoint,
        3,
        0.55,
        0.066,
        0.50,
    ),
];

#[test]
fn quick_suite_holds_the_contract_and_its_accuracy() {
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    let conditions = quick_conditions(&Family::ALL);
    let rows = run(&conditions, &Refiner::ALL, threads);
    let summary = summarize(&rows);

    let mut failures = Vec::new();
    for row in &rows {
        if let Some(score) = &row.score {
            if score.wrong_labels > 0 {
                failures.push(format!(
                    "{} {}: {} wrong label(s)",
                    row.condition.label(),
                    row.refiner.label(),
                    score.wrong_labels
                ));
            }
        }
    }
    for (family, refiner, detected, recall, rmse, sigma0) in BOUNDS {
        let s = summary
            .iter()
            .find(|s| s.family == family && s.refiner == refiner)
            .expect("every family × refiner runs");
        let tag = format!("{} {}", family.label(), refiner.label());
        eprintln!(
            "{tag}: detected {}/{} recall {:.3} max RMSE {:?} max σ̂₀² {:?}",
            s.detected, s.conditions, s.mean_recall, s.max_rmse_px, s.max_sigma0_sq_truth
        );
        if s.detected < detected {
            failures.push(format!("{tag}: detected {} < {detected}", s.detected));
        }
        if s.mean_recall < recall {
            failures.push(format!("{tag}: recall {:.3} < {recall}", s.mean_recall));
        }
        if s.max_rmse_px.is_none_or(|v| v > rmse) {
            failures.push(format!("{tag}: max RMSE {:?} > {rmse}", s.max_rmse_px));
        }
        if s.max_sigma0_sq_truth.is_none_or(|v| v > sigma0) {
            failures.push(format!(
                "{tag}: max σ̂₀² {:?} > {sigma0}",
                s.max_sigma0_sq_truth
            ));
        }
        // Lossless synthetic frames: the noise estimate holds and every
        // delivered corner carries a covariance.
        if s.min_coverage.is_some_and(|c| c < 1.0) {
            failures.push(format!("{tag}: coverage {:?} < 1", s.min_coverage));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));

    // Deterministic: a second run serializes identically (positions,
    // covariances, quality and all).
    let again = run(&conditions, &Refiner::ALL, threads);
    assert_eq!(
        serde_json::to_string(&rows).expect("rows serialize"),
        serde_json::to_string(&again).expect("rows serialize"),
        "the suite is not deterministic"
    );
}
