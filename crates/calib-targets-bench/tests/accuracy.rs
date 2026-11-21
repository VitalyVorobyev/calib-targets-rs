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
//! A second test runs the edge-blur study's quick subset and bounds the
//! per-corner blur estimate the same way (see [`BLUR_BOUNDS`]).
//!
//! The full grid is `cargo run -p calib-targets-bench --release --bin bench
//! -- accuracy`.

use calib_targets_bench::accuracy::blur::blur_conditions;
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
    // 2/4, recall 0.500, RMSE 0.096, σ̂₀² 0.44 (re-measured when a corner the
    // saddle-point fit rejects began keeping its centre-of-mass position: the
    // 24 px / 45° board that the rejecting fit alone detected now misses, as
    // under the centre of mass)
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
    // centre-of-mass fallback: the corners the saddle-point fit rejects next to
    // an edge dot now count, at the centre of mass's accuracy)
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

/// `(family, refiner, min coverage, max |σ̂/σ_true − 1| over every estimated
/// corner)` on the blur study's quick subset, measured values in the comments.
/// Margins as above: coverage −0.05 absolute, the worst error ×1.25 + 0.005.
/// Coverage includes the guard condition of the first three families, which
/// must decline (see [`BLUR_GUARDS`]).
const BLUR_BOUNDS: [(Family, Refiner, f64, f64); 8] = [
    // 270/378 = 0.714, worst 2.30 %
    (Family::Chessboard, Refiner::CenterOfMass, 0.664, 0.034),
    // 270/378 = 0.714, worst 2.38 %
    (Family::Chessboard, Refiner::SaddlePoint, 0.664, 0.035),
    // 63/126 = 0.500, worst 1.78 %
    (Family::Charuco, Refiner::CenterOfMass, 0.450, 0.027),
    // 63/126 = 0.500, worst 1.55 %
    (Family::Charuco, Refiner::SaddlePoint, 0.450, 0.025),
    // 82/189 = 0.434, worst 4.02 %
    (Family::PuzzleBoard, Refiner::CenterOfMass, 0.384, 0.055),
    // 82/189 = 0.434, worst 3.97 %
    (Family::PuzzleBoard, Refiner::SaddlePoint, 0.384, 0.055),
    // 63/63 = 1.000, worst 3.38 %
    (Family::MarkerBoard, Refiner::CenterOfMass, 0.95, 0.048),
    // 63/63 = 1.000, worst 3.48 %
    (Family::MarkerBoard, Refiner::SaddlePoint, 0.95, 0.049),
];

/// `(family, cell px, blur px)` of the guard conditions: a blur just beyond
/// what the family's window holds at that cell. Every corner must decline —
/// a σ there would come from a truncated or contaminated profile.
const BLUR_GUARDS: [(Family, f64, f64); 3] = [
    (Family::Chessboard, 32.0, 4.0),
    (Family::Charuco, 24.0, 0.5),
    (Family::PuzzleBoard, 32.0, 1.5),
];

#[test]
fn quick_blur_study_holds_its_accuracy() {
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    let conditions = blur_conditions(&Family::ALL, true);
    let rows = run(&conditions, &Refiner::ALL, threads);

    let mut failures = Vec::new();
    for row in &rows {
        let c = row.condition;
        let Some(blur) = &row.edge_blur else {
            failures.push(format!(
                "{} {}: no detection",
                c.label(),
                row.refiner.label()
            ));
            continue;
        };
        let guard = BLUR_GUARDS
            .iter()
            .any(|&(f, cell, b)| f == c.family && cell == c.cell_px && b == c.blur_px);
        if guard && !blur.estimates.is_empty() {
            failures.push(format!(
                "{} {}: {} σ beyond the window's range",
                c.label(),
                row.refiner.label(),
                blur.estimates.len()
            ));
        }
    }
    for (family, refiner, min_coverage, max_error) in BLUR_BOUNDS {
        let mine: Vec<_> = rows
            .iter()
            .filter(|r| r.condition.family == family && r.refiner == refiner)
            .filter_map(|r| r.edge_blur.as_ref())
            .collect();
        let corners: usize = mine.iter().map(|b| b.corners).sum();
        let estimated: usize = mine.iter().map(|b| b.estimates.len()).sum();
        let coverage = estimated as f64 / corners.max(1) as f64;
        let worst = mine
            .iter()
            .flat_map(|b| b.estimates.iter().map(|e| (e[0] / b.truth_px - 1.0).abs()))
            .fold(0.0, f64::max);
        let tag = format!("{} {}", family.label(), refiner.label());
        eprintln!("{tag}: σ on {estimated}/{corners} = {coverage:.3}, worst |error| {worst:.4}");
        if coverage < min_coverage {
            failures.push(format!("{tag}: coverage {coverage:.3} < {min_coverage}"));
        }
        if worst > max_error {
            failures.push(format!("{tag}: worst σ error {worst:.4} > {max_error}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));

    let again = run(&conditions, &Refiner::ALL, threads);
    assert_eq!(
        serde_json::to_string(&rows).expect("rows serialize"),
        serde_json::to_string(&again).expect("rows serialize"),
        "the blur study is not deterministic"
    );
}
