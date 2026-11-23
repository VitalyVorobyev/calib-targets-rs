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

use calib_targets::blur::{
    BoardPhotometricOptions, PhotometricFitOptions, RadiometricModel, ThinLensDefocusModel,
};
use calib_targets::chessboard::ChessboardParams;
use calib_targets::core::GrayImageView;
use calib_targets::detect::{default_chess_config, detect_chessboard};
use calib_targets::printable::{
    board_primitives, ChessboardTargetSpec, PuzzleBoardTargetSpec, TargetSpec,
};
use calib_targets::puzzleboard::{PuzzleBoardDetector, PuzzleBoardParams, PuzzleBoardSpec};
use calib_targets_bench::accuracy::blur::{
    blur_conditions, psf_conditions, run_psf, summarize_psf,
};
use calib_targets_bench::accuracy::blur::{robust_sd, BlurScore};
use calib_targets_bench::accuracy::families::Family;
use calib_targets_bench::accuracy::render::{
    supersample, Camera, EinkSurfaceSpec, ExposureSpec, Pose, Psf, RenderSpec, Rendered, Scene,
};
use calib_targets_bench::accuracy::{
    metrics, quick_conditions, render_in_geometry, run, summarize, Condition, Refiner, Row,
};
use nalgebra::Point2;

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
const BLUR_BOUNDS: [(Family, Refiner, f64, f64, f64); 8] = [
    // 1.000, worst 1.53 %, honesty 1.08
    (Family::Chessboard, Refiner::CenterOfMass, 0.95, 0.024, 1.35),
    // 1.000, worst 1.54 %, honesty 1.12
    (Family::Chessboard, Refiner::SaddlePoint, 0.95, 0.024, 1.40),
    // 1.000, worst 1.30 %, honesty 1.06
    (Family::Charuco, Refiner::CenterOfMass, 0.95, 0.022, 1.35),
    // 1.000, worst 1.30 %, honesty 1.06
    (Family::Charuco, Refiner::SaddlePoint, 0.95, 0.022, 1.35),
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

    // Other PSFs: anisotropic Gaussians (2×1 at 30°, and rotated 3:1 at 35°),
    // radius-2 px defocus disk, Moffat (measured −9.5%..−9.6% Gaussian-core
    // bias), and Airy (measured −36.9%..−37.2% Gaussian-core bias). Every PSF
    // delivers sub-0.01 px photometric RMSE.
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
            Psf::Moffat { .. } => {
                if p.rel_bias.is_none_or(|b| !(-0.14..=-0.05).contains(&b)) {
                    failures.push(format!("{tag}: Moffat σ bias {:?}", p.rel_bias));
                }
            }
            Psf::Airy { .. } => {
                if p.rel_bias.is_none_or(|b| !(-0.44..=-0.30).contains(&b)) {
                    failures.push(format!("{tag}: Airy σ bias {:?}", p.rel_bias));
                }
            }
        }
        if p.baseline_rmse_px.is_none() || p.refined_rmse_px.is_none_or(|r| r >= 0.01) {
            failures.push(format!(
                "{tag}: rmse base={:?} refined={:?}",
                p.baseline_rmse_px, p.refined_rmse_px
            ));
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

#[test]
fn tilted_sensor_scheimpflug_scatter_bias_removed() {
    // 8x6 squares (7x5 = 35 inner corners) kept compact so 2-D physical
    // Scheimpflug scatter convolution completes quickly even in debug builds.
    let (sx, sy, sq_mm, cell_px) = (8u32, 6u32, 10.0f64, 24.0f64);
    let spec = TargetSpec::Chessboard(ChessboardTargetSpec::new(sy - 1, sx - 1, sq_mm));
    let (w_mm, h_mm) = spec.board_size_mm().expect("valid spec");
    let prims = board_primitives(&spec).expect("prims");
    let scene = Scene::new(&prims, w_mm, h_mm, sq_mm);
    let pose = Pose {
        cell_px,
        square_mm: sq_mm,
        rotation_deg: 4.0,
        tilt_deg: 35.0,
        k1: 0.0,
    };
    let defocus = ThinLensDefocusModel::new(50.0, 1.2, 438.0, 0.05, [0.0, 0.0])
        .with_sensor_tilt_rad([4.0_f64.to_radians(), -2.0_f64.to_radians()])
        .with_base_blur(calib_targets::blur::BlurKernel::new(0.4, 0.0, 0.4));
    let (camera, size, synced_defocus, plane_n, plane_d) =
        Camera::looking_at_scheimpflug(w_mm, h_mm, &pose, &defocus, 16.0);
    let sampled = supersample(&scene, &camera, size, 4, 6.0, 2026);
    let irr = sampled.integrate_scheimpflug(&synced_defocus, plane_n, plane_d);
    let render_spec = RenderSpec {
        supersample: 4,
        blur_sigma_px: 0.0,
        noise_sigma: 0.8,
        black: 35.0,
        white: 215.0,
        jitter_seed: 2026,
        noise_seed: 9001,
    };
    let rendered = irr.expose(&render_spec);
    let view = GrayImageView {
        width: rendered.width,
        height: rendered.height,
        data: &rendered.pixels,
    };
    let det_base = detect_chessboard(&view, &default_chess_config(), &ChessboardParams::default())
        .expect("chessboard detects under Scheimpflug defocus");
    assert_eq!(
        det_base.corners.len(),
        (sx as usize - 1) * (sy as usize - 1)
    );

    let mut det_no_scatter = det_base.clone();
    let opts_no_scatter =
        BoardPhotometricOptions::default().with_two_pass_scatter_correction(false);
    let ref_no_scatter = calib_targets::blur::refine_chessboard_corners(
        &view,
        &mut det_no_scatter,
        &opts_no_scatter,
    );
    let field = ref_no_scatter
        .blur_field
        .as_ref()
        .expect("blur field fits across Scheimpflug board");

    let mut det_scatter = det_base.clone();
    let opts_scatter = BoardPhotometricOptions::default().with_two_pass_scatter_correction(true);
    let ref_scatter =
        calib_targets::blur::refine_chessboard_corners(&view, &mut det_scatter, &opts_scatter);

    let mut gt_pts = Vec::with_capacity(det_base.corners.len());
    for j in 0..(sy as i32 - 1) {
        for i in 0..(sx as i32 - 1) {
            gt_pts.push((
                (i, j),
                Rendered::truth(
                    &camera,
                    Point2::new(f64::from(i + 1) * sq_mm, f64::from(j + 1) * sq_mm),
                ),
            ));
        }
    }
    let measure_proj_bias =
        |corners: &[calib_targets::chessboard::ChessboardCorner]| -> (f64, f64, usize) {
            let mut proj_sum = 0.0;
            let mut err_sq = 0.0;
            let mut n = 0usize;
            // Estimate gradient direction at board centre so projection onto the unit gradient
            // directly measures signed shift along `∇(tr Σ)`.
            let centre_grad = field.gradient_at(Point2::new(
                rendered.width as f32 * 0.5,
                rendered.height as f32 * 0.5,
            ));
            let gx = f64::from(centre_grad.d_sigma_dx[0] + centre_grad.d_sigma_dx[2]);
            let gy = f64::from(centre_grad.d_sigma_dy[0] + centre_grad.d_sigma_dy[2]);
            let gnorm = gx.hypot(gy).max(1e-12);
            let dir = [gx / gnorm, gy / gnorm];
            // Match corners via nearest GT (order-independent under D4 board grid symmetry).
            let mut wrong = 0usize;
            for c in corners {
                let p = Point2::new(f64::from(c.position.x), f64::from(c.position.y));
                let (_, q) = *gt_pts
                    .iter()
                    .min_by(|a, b| {
                        (a.1 - p)
                            .norm_squared()
                            .total_cmp(&(b.1 - p).norm_squared())
                    })
                    .unwrap();
                let err = p - q;
                if err.norm() > 0.5 * cell_px {
                    wrong += 1;
                }
                proj_sum += err.x * dir[0] + err.y * dir[1];
                err_sq += err.norm_squared();
                n += 1;
            }
            assert_eq!(wrong, 0, "wrong corner labels under Scheimpflug defocus");
            ((err_sq / n as f64).sqrt(), proj_sum / n as f64, n)
        };

    let (rmse_base, bias_base, _) = measure_proj_bias(&det_base.corners);
    let (rmse_no_sc, bias_no_sc, _) = measure_proj_bias(&det_no_scatter.corners);
    let (rmse_sc, bias_sc, n_ok) = measure_proj_bias(&det_scatter.corners);
    let corrections = ref_scatter
        .corners
        .iter()
        .filter_map(|r| r.as_ref().ok().and_then(|f| f.scatter_correction_px))
        .count();
    eprintln!(
        "Scheimpflug n={n_ok} corrections={corrections}: base rmse={rmse_base:.5} (grad_bias={bias_base:+.5}), \
         no_scatter rmse={rmse_no_sc:.5} (grad_bias={bias_no_sc:+.5}), \
         scatter rmse={rmse_sc:.5} (grad_bias={bias_sc:+.5})"
    );
    assert!(
        corrections >= n_ok / 2,
        "expected Pass 2 scatter corrections on most corners"
    );
    assert!(
        bias_sc.abs() < bias_no_sc.abs(),
        "scatter correction should reduce signed defocus-gradient bias: |{bias_sc:.5}| vs |{bias_no_sc:.5}|"
    );
    assert!(
        rmse_sc < 0.03,
        "expected tight Scheimpflug RMSE, got {rmse_sc:.5}"
    );
}

#[test]
fn pocketbook_eink_srgb_photometric_accuracy() {
    // Realistic 300 PPI PocketBook E Ink Carta target (1.5 mm square -> 17.7 dpx/cell)
    // photographed close-up at 32 px/cell under sRGB encoding, 16 gray levels,
    // cover-glass MTF 0.5 dpx, microcapsule bleed κ = 0.10, illumination gradient,
    // and optical Gaussian blur σ_cam = 1.2 px.
    let (sx, sy, sq_mm, cell_px) = (10u32, 8u32, 2.0f64, 32.0f64);
    let target = TargetSpec::PuzzleBoard(PuzzleBoardTargetSpec::new(sy, sx, sq_mm));
    let (w_mm, h_mm) = target.board_size_mm().expect("valid puzzleboard spec");
    let prims = board_primitives(&target).expect("prims");
    let scene = Scene::new(&prims, w_mm, h_mm, sq_mm);
    let pose = Pose {
        cell_px,
        square_mm: sq_mm,
        rotation_deg: 12.0,
        tilt_deg: 20.0,
        k1: 0.0,
    };
    let (camera, size) = Camera::looking_at(w_mm, h_mm, &pose, 24.0);
    let eink = EinkSurfaceSpec {
        display_ppi: 300.0,
        cell_mm: sq_mm,
        gray_levels: 16,
        surface_mtf_dpx: 0.5,
        microcapsule_bleed_kappa: 0.10,
    };
    let cam_sigma = 1.2f64;
    let sampled = scene.supersample_eink(&camera, size, 8, cam_sigma, &eink, 777);
    let irr = sampled.integrate(cam_sigma);
    let exp_spec = ExposureSpec {
        render: RenderSpec {
            supersample: 8,
            blur_sigma_px: cam_sigma,
            noise_sigma: 1.5,
            black: 35.0,
            white: 220.0,
            jitter_seed: 777,
            noise_seed: 888,
        },
        radiometric: RadiometricModel::Srgb,
        shading: [0.10, -0.07],
        gamma: 2.2,
    };
    let rendered = irr.expose_with(&exp_spec);
    let view = GrayImageView {
        width: rendered.width,
        height: rendered.height,
        data: &rendered.pixels,
    };

    let pb_spec = PuzzleBoardSpec::with_origin(sy, sx, sq_mm as f32, 0, 0).expect("valid pb");
    let mut pb_params = PuzzleBoardParams::for_board(pb_spec);
    pb_params.chess = default_chess_config();
    let detector = PuzzleBoardDetector::new(pb_params).expect("detector");
    let det_base = detector
        .detect(&view)
        .expect("PuzzleBoard detects on e-ink render");
    assert_eq!(
        det_base.corners.len(),
        (sx as usize - 1) * (sy as usize - 1)
    );

    let true_cam_sigma = (cam_sigma * cam_sigma + 1.0 / 12.0).sqrt();

    let mut det_naive = det_base.clone();
    let ref_naive = calib_targets::blur::refine_puzzleboard_corners(
        &view,
        &mut det_naive,
        &BoardPhotometricOptions::default(),
    );
    let naive_cam_sigmas: Vec<f64> = ref_naive
        .corners
        .iter()
        .filter_map(|r| r.as_ref().ok()?.camera_blur())
        .map(|k| (0.5 * f64::from(k.xx + k.yy)).sqrt())
        .collect();
    let naive_med_sigma = {
        let mut s = naive_cam_sigmas.clone();
        s.sort_by(f64::total_cmp);
        s[s.len() / 2]
    };

    let mut det_eink = det_base.clone();
    let fit_eink = PhotometricFitOptions::default()
        .with_radiometric(RadiometricModel::Srgb)
        .with_fit_ink_bleed(true)
        .with_display(eink.to_display_surface_model());
    let ref_eink = calib_targets::blur::refine_puzzleboard_corners(
        &view,
        &mut det_eink,
        &BoardPhotometricOptions::default().with_fit(fit_eink),
    );
    let eink_cam_sigmas: Vec<f64> = ref_eink
        .corners
        .iter()
        .filter_map(|r| r.as_ref().ok()?.camera_blur())
        .map(|k| (0.5 * f64::from(k.xx + k.yy)).sqrt())
        .collect();
    let eink_med_sigma = {
        let mut s = eink_cam_sigmas.clone();
        s.sort_by(f64::total_cmp);
        s[s.len() / 2]
    };

    let mut rmse_base_sq = 0.0;
    let mut rmse_eink_sq = 0.0;
    let mut n = 0usize;
    for (cb, ce) in det_base.corners.iter().zip(&det_eink.corners) {
        let p_b = Point2::new(f64::from(cb.position.x), f64::from(cb.position.y));
        let p_e = Point2::new(f64::from(ce.position.x), f64::from(ce.position.y));
        let truth = Rendered::truth(
            &camera,
            Point2::new(
                f64::from(ce.target_position.x),
                f64::from(ce.target_position.y),
            ),
        );
        assert!(
            (p_e - truth).norm() < 0.5 * cell_px,
            "wrong PuzzleBoard e-ink corner"
        );
        rmse_base_sq += (p_b - truth).norm_squared();
        rmse_eink_sq += (p_e - truth).norm_squared();
        n += 1;
    }
    let rmse_base = (rmse_base_sq / n as f64).sqrt();
    let rmse_eink = (rmse_eink_sq / n as f64).sqrt();
    eprintln!(
        "PocketBook e-ink n={n}: true_cam_σ={true_cam_sigma:.3}, naive_med={naive_med_sigma:.3}, \
         eink_med={eink_med_sigma:.3}, rmse base={rmse_base:.4} -> eink={rmse_eink:.4}"
    );
    assert!(
        (eink_med_sigma - true_cam_sigma).abs() < (naive_med_sigma - true_cam_sigma).abs(),
        "e-ink physical model must reduce optical sigma bias: eink {eink_med_sigma:.3} vs naive {naive_med_sigma:.3} (truth {true_cam_sigma:.3})"
    );
    assert!(
        rmse_eink < 0.05,
        "e-ink refined corner RMSE too high: {rmse_eink:.4}"
    );
}
