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
//! A ChArUco guard renders the board under a blur that leaves its markers
//! unreadable and checks that nothing wrong is delivered (a miss is fine).
//!
//! A further test runs the corner-blur study's quick subset and bounds the
//! per-corner blur ([`calib_targets::blur`]) the same way (see
//! [`BLUR_BOUNDS`]): coverage, worst error and standard-error honesty against
//! the rendered truth up to σ/cell 0.25, and the anisotropic and defocus-disk
//! PSFs.
//!
//! The full grid is `cargo run -p calib-targets-bench --release --bin bench
//! -- accuracy`.

use calib_targets_bench::accuracy::blur::{
    blur_conditions, psf_conditions, run_psf, summarize_psf,
};
use calib_targets_bench::accuracy::blur::{robust_sd, BlurScore};
use calib_targets_bench::accuracy::families::Family;
use calib_targets_bench::accuracy::render::Psf;
use calib_targets_bench::accuracy::{
    metrics, quick_conditions, render_in_geometry, run, summarize, Condition, Refiner, Row,
};

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
    let rows = run(&conditions, &Refiner::ALL, threads, false);
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
    let again = run(&conditions, &Refiner::ALL, threads, false);
    assert_eq!(
        serde_json::to_string(&rows).expect("rows serialize"),
        serde_json::to_string(&again).expect("rows serialize"),
        "the suite is not deterministic"
    );
}

/// The ChArUco board at 32 px squares under a 4 px blur and noise 4, the
/// frame that once delivered wrong labels. The blur leaves the markers
/// unreadable and the true X-junctions faint while marker-interior saddles
/// respond strongly; a detector may miss here, never mislabel. Rendered as
/// the condition grid renders it: returns the 4 px condition and the 0.5 px
/// condition that leads its geometry group (and so seeds the render).
fn unresolved_charuco_markers() -> (Condition, Condition) {
    let condition = |blur_px: f64, noise: f64| Condition {
        family: Family::Charuco,
        cell_px: 32.0,
        rotation_deg: 0.0,
        tilt_deg: 0.0,
        blur_px,
        noise,
        k1: 0.0,
    };
    (condition(4.0, 4.0), condition(0.5, 1.0))
}

/// On that frame the chessboard stage once assembled a 10-corner "grid" from
/// marker-interior saddles (cells of 8 and 30 px side by side on one grid
/// line), one of its cells read as a marker with three bits wrong, and that
/// chance decode placed the board. ChArUco must deliver no wrong label.
#[test]
fn charuco_under_unresolving_blur_delivers_no_wrong_labels() {
    let (frame, head) = unresolved_charuco_markers();
    let rows = run(&[head, frame], &Refiner::ALL, 2, false);
    assert_eq!(rows.len(), 4);
    for row in &rows {
        let wrong = row.score.as_ref().map_or(0, |s| s.wrong_labels);
        assert_eq!(
            wrong,
            0,
            "{} {}: {wrong} wrong label(s)",
            row.condition.label(),
            row.refiner.label()
        );
    }
}

/// The same false grid reached plain chessboard users: the chessboard
/// detector run on the frame delivered it as a 10-corner detection. Its
/// edges disagree with their neighbours by up to 3.6× along one grid line,
/// which the final wrong-label check sees — once it runs on small
/// components too.
#[test]
fn chessboard_on_unresolved_charuco_markers_delivers_no_false_grid() {
    let (frame, head) = unresolved_charuco_markers();
    let (board, camera, image) = render_in_geometry(&head, frame.blur_px, &frame);
    for refiner in Refiner::ALL {
        if let Ok(found) = board.detect_as_chessboard(&image, &|cfg| refiner.apply(cfg)) {
            let score = metrics::score(&found, &board.gt, &camera);
            assert_eq!(
                score.wrong_labels,
                0,
                "{} {}: the chessboard detector delivered {} corners, {} wrong",
                frame.label(),
                refiner.label(),
                score.delivered,
                score.wrong_labels
            );
        }
    }
}

/// Per-(family, refiner) metrics of the blur quick subset:
/// `(coverage, worst |σ̂/σ_true − 1|, robust SD of (σ̂ − σ_true)/SE)`.
fn blur_metrics(rows: &[Row], family: Family, refiner: Refiner) -> (f64, f64, f64) {
    let mine: Vec<&BlurScore> = rows
        .iter()
        .filter(|r| r.condition.family == family && r.refiner == refiner)
        .filter_map(|r| r.blur.as_ref())
        .collect();
    let corners: usize = mine.iter().map(|b| b.corners).sum();
    let estimated: usize = mine.iter().map(|b| b.estimates.len()).sum();
    let worst = mine
        .iter()
        .flat_map(|b| {
            b.estimates
                .iter()
                .map(|e| (e.sigma / b.truth_px - 1.0).abs())
        })
        .fold(0.0, f64::max);
    let z: Vec<f64> = mine
        .iter()
        .flat_map(|b| b.estimates.iter().map(|e| (e.sigma - b.truth_px) / e.se))
        .collect();
    (
        estimated as f64 / corners.max(1) as f64,
        worst,
        robust_sd(&z).unwrap_or(f64::NAN),
    )
}

/// `(family, refiner, min coverage, max worst |σ̂/σ_true − 1|, max SE
/// honesty against the truth)` on the blur study's quick subset, measured
/// values in the comments. Margins: coverage −0.05 absolute, the worst error
/// ×1.25 + 0.005, honesty ×1.25 (a robust SD over a few hundred corners
/// scatters by ~0.1).
const BLUR_BOUNDS: [(Family, Refiner, f64, f64, f64); 6] = [
    // 1.000, worst 1.53 %, honesty 1.08
    (Family::Chessboard, Refiner::CenterOfMass, 0.95, 0.024, 1.35),
    // 1.000, worst 1.54 %, honesty 1.12
    (Family::Chessboard, Refiner::SaddlePoint, 0.95, 0.024, 1.40),
    // 1.000, worst 2.26 %, honesty 0.96
    (
        Family::PuzzleBoard,
        Refiner::CenterOfMass,
        0.95,
        0.033,
        1.19,
    ),
    // 1.000, worst 2.19 %, honesty 1.03
    (Family::PuzzleBoard, Refiner::SaddlePoint, 0.95, 0.033, 1.28),
    // 1.000, worst 3.25 %, honesty 1.00
    (
        Family::MarkerBoard,
        Refiner::CenterOfMass,
        0.95,
        0.046,
        1.24,
    ),
    // 1.000, worst 3.25 %, honesty 1.01
    (Family::MarkerBoard, Refiner::SaddlePoint, 0.95, 0.046, 1.26),
];

#[test]
fn quick_blur_study_holds_its_accuracy() {
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    let conditions = blur_conditions(&Family::ALL, true);
    let rows = run(&conditions, &Refiner::ALL, threads, true);

    let mut failures = Vec::new();
    for row in &rows {
        if row.blur.is_none() {
            failures.push(format!(
                "{} {}: no detection",
                row.condition.label(),
                row.refiner.label()
            ));
        }
    }
    for (family, refiner, min_coverage, max_error, max_honesty) in BLUR_BOUNDS {
        let (coverage, worst, honesty) = blur_metrics(&rows, family, refiner);
        let tag = format!("{} {}", family.label(), refiner.label());
        eprintln!("{tag}: coverage {coverage:.3}, worst |error| {worst:.4}, honesty {honesty:.2}");
        if coverage < min_coverage {
            failures.push(format!("{tag}: coverage {coverage:.3} < {min_coverage}"));
        }
        if worst > max_error {
            failures.push(format!("{tag}: worst σ error {worst:.4} > {max_error}"));
        }
        if honesty.is_nan() || honesty > max_honesty {
            failures.push(format!("{tag}: SE honesty {honesty:.2} > {max_honesty}"));
        }
    }

    // Other PSFs. An anisotropic Gaussian (σ 2 × 1 px at 30°): every
    // component of Σ within 1 % (+0.01 px²) of the truth and the major axis
    // within 0.5° (measured: components within 0.2 %, angle −0.02°). A
    // radius-2 px disk: σ above the disk's second moment by the measured
    // +7.9 % (chessboard) / +8.1 % (PuzzleBoard), band ±3 % — the
    // documented Gaussian-equivalent behaviour on a flat-topped PSF.
    let psf = summarize_psf(&run_psf(&psf_conditions(true), threads));
    for p in &psf {
        let tag = p.condition.label();
        if p.coverage.is_none_or(|c| c < 0.95) {
            failures.push(format!("{tag}: coverage {:?}", p.coverage));
        }
        match p.condition.psf {
            Psf::Gaussian { .. } => {
                for (k, (got, want)) in p.median_kernel.iter().zip(p.truth_kernel).enumerate() {
                    let ok = got.is_some_and(|g| (g - want).abs() <= 0.01 * want.abs() + 0.01);
                    if !ok {
                        failures.push(format!("{tag}: Σ[{k}] {got:?} vs truth {want:.3}"));
                    }
                }
                if p.angle_error_deg.is_none_or(|a| a.abs() > 0.5) {
                    failures.push(format!("{tag}: angle error {:?}°", p.angle_error_deg));
                }
            }
            Psf::Disk { .. } => {
                if p.rel_bias.is_none_or(|b| !(0.05..=0.11).contains(&b)) {
                    failures.push(format!("{tag}: σ vs second moment {:?}", p.rel_bias));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));

    let again = run(&conditions, &Refiner::ALL, threads, true);
    assert_eq!(
        serde_json::to_string(&rows).expect("rows serialize"),
        serde_json::to_string(&again).expect("rows serialize"),
        "the blur study is not deterministic"
    );
}
