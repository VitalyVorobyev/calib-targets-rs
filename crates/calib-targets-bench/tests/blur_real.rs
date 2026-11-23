//! The per-corner blur ([`calib_targets::blur`]) on public real photographs.
//!
//! Synthetic truth carries the accuracy claim; real images have no truth, so
//! these tests check what a correct estimate must show without one:
//!
//! - **A smooth σ field with honest standard errors.** Optical blur varies
//!   slowly across a board, so along a grid line the second difference
//!   `σ(u−1) − 2σ(u) + σ(u+1)` is mostly noise; divided by its standard error
//!   (from the three corners' SEs), its robust spread is 1 when the standard
//!   errors are honest and below 1 when they are conservative (neighbouring
//!   windows share no pixels at the half-cell window, but do beyond it). It
//!   must not be well above 1: that would be a σ scattering more than its
//!   standard error admits.
//! - **A sharpened photograph is declined, not measured.** In-camera
//!   sharpening makes edges steeper than the pixel aperture allows; those
//!   corners must decline as sharper than the pixel rather than report a σ.
//!
//! Each bound is the measured value (in the comment) with a stated margin: a
//! regression guard on measured behaviour.

use std::collections::HashMap;
use std::path::PathBuf;

use calib_targets::blur::{chessboard_blur, puzzleboard_blur, BlurDecline, CornerBlur};
use calib_targets::chessboard::ChessboardParams;
use calib_targets::detect::{
    default_chess_config, detect_chessboard, detect_puzzleboard_best, gray_view,
};
use calib_targets::puzzleboard::{PuzzleBoardParams, PuzzleBoardSpec};
use calib_targets_bench::accuracy::blur::robust_sd;
use image::GrayImage;

fn load_gray(name: &str) -> GrayImage {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata")
        .join(name);
    image::ImageReader::open(&path)
        .unwrap_or_else(|e| panic!("open {}: {e}", path.display()))
        .decode()
        .unwrap_or_else(|e| panic!("decode {}: {e}", path.display()))
        .to_luma8()
}

/// Second differences of σ along both grid axes, each over its standard
/// error, and the same in px.
fn second_differences(estimates: &HashMap<(i32, i32), CornerBlur>) -> (Vec<f64>, Vec<f64>) {
    let mut keys: Vec<_> = estimates.keys().copied().collect();
    keys.sort_unstable();
    let mut z = Vec::new();
    let mut px = Vec::new();
    for (u, v) in keys {
        let b = &estimates[&(u, v)];
        for (du, dv) in [(1, 0), (0, 1)] {
            let (Some(a), Some(c)) = (
                estimates.get(&(u - du, v - dv)),
                estimates.get(&(u + du, v + dv)),
            ) else {
                continue;
            };
            let d2 = f64::from(a.sigma_px) - 2.0 * f64::from(b.sigma_px) + f64::from(c.sigma_px);
            let se = (f64::from(a.sigma_std_error_px).powi(2)
                + 4.0 * f64::from(b.sigma_std_error_px).powi(2)
                + f64::from(c.sigma_std_error_px).powi(2))
            .sqrt();
            z.push(d2 / se);
            px.push(d2);
        }
    }
    (z, px)
}

/// `example4.png`, a phone photograph of a PuzzleBoard from the PuzzleBoard
/// authors' set (decoded on the full image; the blur on a 15 × 15-corner
/// block). Measured: σ on 200/221 corners (0.90), median σ 0.72 px; second
/// differences: robust SD of z 0.83 over 281 pairs, of σ itself 0.084 px.
/// Bounds: coverage ≥ 0.80; z spread in [0.5, 1.25]; σ spread ≤ 0.12 px.
#[test]
fn puzzleboard_photograph_has_a_smooth_honest_blur_field() {
    // `testdata/puzzleboard_reference/` is a local-only directory.
    let local = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/puzzleboard_reference/example4.png");
    if !local.exists() {
        println!("testdata/puzzleboard_reference/ not found — skipping.");
        return;
    }
    let img = load_gray("puzzleboard_reference/example4.png");
    let view = gray_view(&img);
    let board = PuzzleBoardSpec::new(20, 20, 5.0).expect("board spec");
    let mut detection = detect_puzzleboard_best(&view, &PuzzleBoardParams::sweep_for_board(&board))
        .expect("the photograph decodes");
    // A 15 × 15-corner block on the photograph's defocused side (its centre
    // in master coordinates); the in-focus centre is sharpened and declines.
    let (u0, v0) = (302, 391);
    detection
        .corners
        .retain(|c| (c.grid.u - u0).abs() <= 7 && (c.grid.v - v0).abs() <= 7);
    let blur = puzzleboard_blur(&view, &detection);
    let estimates: HashMap<(i32, i32), CornerBlur> = detection
        .corners
        .iter()
        .zip(&blur)
        .filter_map(|(c, b)| b.as_ref().ok().map(|b| ((c.grid.u, c.grid.v), *b)))
        .collect();
    let coverage = estimates.len() as f64 / detection.corners.len() as f64;
    let mut sigma: Vec<f64> = estimates.values().map(|b| f64::from(b.sigma_px)).collect();
    sigma.sort_by(f64::total_cmp);
    let (z, px) = second_differences(&estimates);
    let z_spread = robust_sd(&z).expect("second differences");
    let px_spread = robust_sd(&px).expect("second differences");
    eprintln!(
        "example4: σ on {}/{} ({coverage:.2}), median σ {:.3} px; second differences n {}: z spread {z_spread:.2}, σ spread {px_spread:.4} px",
        estimates.len(),
        detection.corners.len(),
        sigma[sigma.len() / 2],
        z.len()
    );
    assert!(coverage >= 0.80, "coverage {coverage:.2}");
    assert!(
        (0.5..=1.25).contains(&z_spread),
        "second-difference z spread {z_spread:.2}"
    );
    assert!(
        px_spread <= 0.12,
        "σ second-difference spread {px_spread:.4} px"
    );
}

/// `mid.png`, an in-camera sharpened chessboard photograph. Measured: 51 of
/// 77 corners decline as sharper than the pixel, 23 get a σ. Bound: at least
/// half decline that way.
#[test]
fn sharpened_photograph_is_declined_as_sharper_than_the_pixel() {
    let img = load_gray("mid.png");
    let view = gray_view(&img);
    let mut params = ChessboardParams::default();
    params.min_corner_strength = 0.5;
    let detection =
        detect_chessboard(&view, &default_chess_config(), &params).expect("chessboard detects");
    let blur = chessboard_blur(&view, &detection);
    let sharper = blur
        .iter()
        .filter(|b| matches!(b, Err(BlurDecline::SharperThanPixel)))
        .count();
    eprintln!("mid.png: {sharper}/{} sharper than the pixel", blur.len());
    assert!(
        2 * sharper >= blur.len(),
        "{sharper}/{} declined as sharper than the pixel",
        blur.len()
    );
}

/// `puzzleboard/foreshortened.png`, tracked unsharpened PuzzleBoard image with
/// perspective foreshortening and ~2 px optical/lens blur.
///
/// Measured: 354 corners decoded; `puzzleboard_blur` covers 354/354 (1.00)
/// with smooth second-difference spread (`z` spread `0.90`, σ spread
/// `0.0623 px`); `refine_puzzleboard_corners` converges on all 354/354 corners
/// and cuts `local_residual_rms_px` from `0.2014 px` to `0.1211 px`.
/// Bounds: corners `>= 300`, blur coverage `>= 0.90`, `z` spread in
/// `[0.5, 1.35]`, `σ` spread `<= 0.10 px`, photometric convergence `>= 0.95`,
/// and improved `local_residual_rms_px`.
#[test]
fn foreshortened_puzzleboard_has_smooth_blur_and_improves_rms() {
    use calib_targets::blur::{refine_puzzleboard_corners, BoardPhotometricOptions};

    let img = load_gray("puzzleboard/foreshortened.png");
    let view = gray_view(&img);
    let spec = PuzzleBoardSpec::new(10, 10, 5.0).expect("board spec");
    let mut detection = detect_puzzleboard_best(&view, &PuzzleBoardParams::sweep_for_board(&spec))
        .expect("foreshortened puzzleboard decodes");
    let n_corners = detection.corners.len();
    assert!(n_corners >= 300, "unexpectedly few corners: {n_corners}");

    let blur = puzzleboard_blur(&view, &detection);
    let estimates: HashMap<(i32, i32), CornerBlur> = detection
        .corners
        .iter()
        .zip(&blur)
        .filter_map(|(c, b)| b.as_ref().ok().map(|b| ((c.grid.u, c.grid.v), *b)))
        .collect();
    let coverage = estimates.len() as f64 / n_corners as f64;
    let (z, px) = second_differences(&estimates);
    let z_spread = robust_sd(&z).expect("second differences");
    let px_spread = robust_sd(&px).expect("second differences");

    let rms_before = detection
        .quality
        .local_residual_rms_px
        .expect("baseline local_residual_rms_px");
    let refinement =
        refine_puzzleboard_corners(&view, &mut detection, &BoardPhotometricOptions::default());
    let converged = refinement.corners.iter().filter(|r| r.is_ok()).count();
    let rms_after = detection
        .quality
        .local_residual_rms_px
        .expect("refined local_residual_rms_px");

    assert_eq!(
        detection.corners.len(),
        n_corners,
        "photometric refinement must not drop corners"
    );
    assert!(coverage >= 0.90, "coverage {coverage:.2}");
    assert!(
        (0.5..=1.35).contains(&z_spread),
        "second-difference z spread {z_spread:.2}"
    );
    assert!(
        px_spread <= 0.10,
        "σ second-difference spread {px_spread:.4} px"
    );
    assert!(
        converged * 100 >= n_corners * 95,
        "only {converged}/{n_corners} corners converged"
    );
    assert!(
        rms_after < rms_before,
        "expected photometric refinement to reduce local_residual_rms_px ({rms_before:.4} -> {rms_after:.4})"
    );
}
