use nalgebra::{Matrix3, Point2};

use super::gauss_polygon::cdf;
use super::{
    estimate_corner_blur, fit_corner_photometric, BlurDecline, BlurPattern, DisplaySurfaceModel,
    PhotometricFitOptions, RadiometricModel,
};
use crate::{GrayImageView, Homography};

fn chessboard_pattern() -> BlurPattern {
    let mut p = BlurPattern::new().with_known_polarity(true);
    for b in -4..4 {
        for a in -4..4 {
            p = p.with_known_cell(a, b);
            if (a + b) % 2 == 0 {
                let (x, y) = (f64::from(a), f64::from(b));
                p = p.with_polygon(
                    vec![
                        Point2::new(x, y),
                        Point2::new(x + 1.0, y),
                        Point2::new(x + 1.0, y + 1.0),
                        Point2::new(x, y + 1.0),
                    ],
                    1.0,
                );
            }
        }
    }
    p
}

fn blurred_wave(x: f64, sigma: f64) -> f64 {
    if sigma <= 1e-9 {
        return if (x.floor() as i32).rem_euclid(2) == 0 {
            1.0
        } else {
            -1.0
        };
    }
    (-12..12)
        .map(|k: i32| {
            let sign = if k.rem_euclid(2) == 0 { 1.0 } else { -1.0 };
            let kf = f64::from(k);
            sign * (cdf((x - kf) / sigma) - cdf((x - kf - 1.0) / sigma))
        })
        .sum()
}

fn render_patch(
    size: usize,
    corner: [f64; 2],
    cell_px: f64,
    lens_px: f64,
    bleed_px: f64,
    radiometric: RadiometricModel,
) -> Vec<u8> {
    const SUB: usize = 4;
    let sigma = lens_px / cell_px;
    let bleed_cells = bleed_px / cell_px;
    let mut out = vec![0u8; size * size];
    for y in 0..size {
        for x in 0..size {
            let mut black = 0.0;
            for sy in 0..SUB {
                for sx in 0..SUB {
                    let px = x as f64 - 0.5 + (sx as f64 + 0.5) / SUB as f64;
                    let py = y as f64 - 0.5 + (sy as f64 + 0.5) / SUB as f64;
                    let u = (px - corner[0]) / cell_px;
                    let v = (py - corner[1]) / cell_px;
                    // Dilate black squares symmetrically when testing ink bleed:
                    // edge transitions occur at u + shift and v + shift on their outward side.
                    let wave = if bleed_cells.abs() > 1e-12 && sigma > 1e-9 {
                        let base = 0.5 * (1.0 + blurred_wave(u, sigma) * blurred_wave(v, sigma));
                        super::radiometric::bleed_eval(base, bleed_px / lens_px, true).0
                    } else {
                        0.5 * (1.0 + blurred_wave(u, sigma) * blurred_wave(v, sigma))
                    };
                    black += wave;
                }
            }
            black /= (SUB * SUB) as f64;
            let lin = 215.0 - 180.0 * black;
            let (obs, _, _) = radiometric.forward(lin, radiometric.initial_gamma());
            out[y * size + x] = obs.round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

#[test]
fn photometric_fit_refines_position_covariance_and_matches_blur() {
    const SIZE: usize = 128;
    const CORNER: [f64; 2] = [64.35, 63.72];
    const CELL_PX: f64 = 18.0;
    const SUB: usize = 4;
    let total_sigma = 2.6;
    let box_var = (1.0 - 1.0 / (SUB * SUB) as f64) / 12.0;
    let lens = (total_sigma * total_sigma - box_var).sqrt();
    let data = render_patch(SIZE, CORNER, CELL_PX, lens, 0.0, RadiometricModel::Linear);
    let img = GrayImageView {
        width: SIZE,
        height: SIZE,
        data: &data,
    };
    let pat = chessboard_pattern();
    let h_seed = Homography::new(Matrix3::new(
        CELL_PX,
        0.0,
        CORNER[0] + 0.25,
        0.0,
        CELL_PX,
        CORNER[1] - 0.22,
        0.0,
        0.0,
        1.0,
    ));

    let fit = fit_corner_photometric(&img, &pat, &h_seed, &PhotometricFitOptions::default())
        .expect("photometric fit converges");
    let pos_err =
        (f64::from(fit.position.x) - CORNER[0]).hypot(f64::from(fit.position.y) - CORNER[1]);
    assert!(pos_err < 0.01, "pos_err={pos_err:.5} ({:?})", fit.position);
    assert!(fit.position_covariance.sigma_max_px() > 0.0);
    assert!((fit.axes[0].angle - 0.0).abs() < 0.01);
    assert!((fit.axes[1].angle - std::f32::consts::FRAC_PI_2).abs() < 0.01);

    let blur = fit.blur.expect("blur resolved");
    assert!((f64::from(blur.sigma_px) - total_sigma).abs() < 0.05);
    let legacy = estimate_corner_blur(&img, &pat, &h_seed).expect("legacy blur matches");
    assert_eq!(blur, legacy);
}

#[test]
fn radiometric_srgb_and_free_gamma_recover_blur_and_gamma() {
    const SIZE: usize = 128;
    const CORNER: [f64; 2] = [64.28, 63.81];
    const CELL_PX: f64 = 18.0;
    const SUB: usize = 4;
    let total_sigma = 2.4;
    let box_var = (1.0 - 1.0 / (SUB * SUB) as f64) / 12.0;
    let lens = (total_sigma * total_sigma - box_var).sqrt();
    let pat = chessboard_pattern();
    let h_seed = Homography::new(Matrix3::new(
        CELL_PX,
        0.0,
        CORNER[0] + 0.15,
        0.0,
        CELL_PX,
        CORNER[1] - 0.15,
        0.0,
        0.0,
        1.0,
    ));

    // Case A: sRGB encoded image
    let srgb_data = render_patch(SIZE, CORNER, CELL_PX, lens, 0.0, RadiometricModel::Srgb);
    let srgb_img = GrayImageView {
        width: SIZE,
        height: SIZE,
        data: &srgb_data,
    };
    let linear_fit =
        fit_corner_photometric(&srgb_img, &pat, &h_seed, &PhotometricFitOptions::default())
            .expect("linear fit runs");
    let srgb_opts = PhotometricFitOptions::default().with_radiometric(RadiometricModel::Srgb);
    let srgb_fit =
        fit_corner_photometric(&srgb_img, &pat, &h_seed, &srgb_opts).expect("srgb fit succeeds");
    assert!(srgb_fit.fit.residual_rms < 0.5 * linear_fit.fit.residual_rms);
    let err_srgb = (f64::from(srgb_fit.blur.expect("srgb blur").sigma_px) - total_sigma).abs();
    assert!(err_srgb < 0.05, "srgb blur err={err_srgb:.4}");

    // Case B: Gamma = 2.2 power law recovered by FreeGamma seeded at 1.7
    let gam_data = render_patch(
        SIZE,
        CORNER,
        CELL_PX,
        lens,
        0.0,
        RadiometricModel::FixedGamma { gamma: 2.2 },
    );
    let gam_img = GrayImageView {
        width: SIZE,
        height: SIZE,
        data: &gam_data,
    };
    let gam_opts = PhotometricFitOptions::default()
        .with_radiometric(RadiometricModel::FreeGamma { initial_gamma: 1.7 });
    let gam_fit = fit_corner_photometric(&gam_img, &pat, &h_seed, &gam_opts)
        .expect("free gamma fit succeeds");
    let g_hat = gam_fit.gamma.expect("gamma reported");
    assert!((g_hat - 2.2).abs() < 0.08, "g_hat={g_hat}");
    assert!((f64::from(gam_fit.blur.expect("gamma blur").sigma_px) - total_sigma).abs() < 0.06);
}

#[test]
fn ink_bleed_recovers_bleed_and_preserves_corner_position() {
    const SIZE: usize = 128;
    const CORNER: [f64; 2] = [64.3, 63.7];
    const CELL_PX: f64 = 18.0;
    const SUB: usize = 4;
    let total_sigma = 2.5;
    let box_var = (1.0 - 1.0 / (SUB * SUB) as f64) / 12.0;
    let lens = (total_sigma * total_sigma - box_var).sqrt();
    let bleed_px = 0.25 * lens;
    let data = render_patch(
        SIZE,
        CORNER,
        CELL_PX,
        lens,
        bleed_px,
        RadiometricModel::Linear,
    );
    let img = GrayImageView {
        width: SIZE,
        height: SIZE,
        data: &data,
    };
    let pat = chessboard_pattern();
    let h_seed = Homography::new(Matrix3::new(
        CELL_PX,
        0.0,
        CORNER[0] + 0.1,
        0.0,
        CELL_PX,
        CORNER[1] - 0.1,
        0.0,
        0.0,
        1.0,
    ));
    let opts = PhotometricFitOptions::default().with_fit_ink_bleed(true);
    let fit = fit_corner_photometric(&img, &pat, &h_seed, &opts).expect("bleed fit converges");
    let pos_err =
        (f64::from(fit.position.x) - CORNER[0]).hypot(f64::from(fit.position.y) - CORNER[1]);
    assert!(pos_err < 0.015, "pos_err={pos_err:.4}");
    let bleed_est = fit.ink_bleed_cells.expect("bleed cells");
    let expected_bleed = (bleed_px / CELL_PX) as f32;
    assert!(
        (bleed_est - expected_bleed).abs() < 0.008,
        "bleed_est={bleed_est:.5} vs {expected_bleed:.5}"
    );
}

#[test]
fn sharp_patch_declines_blur_while_delivering_converged_position() {
    // Rotated point-sampled PSF of σ = 0.26 px (variance 0.0676 < PIXEL_VAR = 1/12 = 0.0833,
    // mimicking a sharpened image or sub-unit aperture fill factor across all sub-pixel phases):
    // blur declines with SharperThanPixel while corner geometry converges accurately.
    const SIZE: usize = 96;
    const CORNER: [f64; 2] = [48.35, 47.68];
    const CELL_PX: f64 = 16.0;
    let theta = 22.0f64.to_radians();
    let (st, ct) = theta.sin_cos();
    let sigma_cells = 0.26 / CELL_PX;
    let mut data = vec![0u8; SIZE * SIZE];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f64 - CORNER[0];
            let dy = y as f64 - CORNER[1];
            let u = (ct * dx + st * dy) / CELL_PX;
            let v = (-st * dx + ct * dy) / CELL_PX;
            let black = 0.5 * (1.0 + blurred_wave(u, sigma_cells) * blurred_wave(v, sigma_cells));
            data[y * SIZE + x] = (220.0 - 185.0 * black).round().clamp(0.0, 255.0) as u8;
        }
    }
    let img = GrayImageView {
        width: SIZE,
        height: SIZE,
        data: &data,
    };
    let pat = chessboard_pattern();
    let h_seed = Homography::new(Matrix3::new(
        CELL_PX * ct,
        -CELL_PX * st,
        CORNER[0] + 0.12,
        CELL_PX * st,
        CELL_PX * ct,
        CORNER[1] - 0.12,
        0.0,
        0.0,
        1.0,
    ));

    let fit = fit_corner_photometric(&img, &pat, &h_seed, &PhotometricFitOptions::default())
        .expect("sharp patch delivers geometry");
    assert!(fit.blur.is_none(), "expected blur decline, got {fit:?}");
    assert_eq!(fit.blur_decline, Some(BlurDecline::SharperThanPixel));
    let pos_err =
        (f64::from(fit.position.x) - CORNER[0]).hypot(f64::from(fit.position.y) - CORNER[1]);
    assert!(pos_err < 0.05, "pos_err={pos_err:.4}");
    assert_eq!(
        estimate_corner_blur(&img, &pat, &h_seed).err(),
        Some(BlurDecline::SharperThanPixel)
    );
}

#[test]
fn display_surface_model_subtracts_foreshortened_display_covariance() {
    let disp = DisplaySurfaceModel::pocketbook_eink(32.0);
    // Perspective tilt foreshortens x by 0.6 relative to y (18 px vs 30 px per cell).
    let disp_cov = disp.image_covariance([[18.0, 0.0], [0.0, 30.0]]);
    assert!(disp_cov.yy > disp_cov.xx);
    let expected_factor = (1.0 / 12.0 + 0.25) / (32.0 * 32.0);
    assert!((f64::from(disp_cov.xx) - expected_factor * 324.0).abs() < 1e-5);
    assert!((f64::from(disp_cov.yy) - expected_factor * 900.0).abs() < 1e-5);
}
