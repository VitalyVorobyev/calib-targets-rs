#![cfg(feature = "image")]

use calib_targets::aruco::builtins;
use calib_targets::blur::{
    charuco_blur, puzzleboard_blur, refine_charuco_corners, refine_puzzleboard_corners,
    BoardPhotometricOptions, DisplaySurfaceModel, PhotometricFitOptions, RadiometricModel,
};
use calib_targets::charuco::{CharucoBoardSpec, CharucoParams};
use calib_targets::detect::{self, gray_view};
use calib_targets::printable::{
    render_screen_target, CharucoTargetSpec, PuzzleBoardTargetSpec, ScreenTargetOptions, TargetSpec,
};
use calib_targets::puzzleboard::{PuzzleBoardParams, PuzzleBoardSpec};

fn blur_and_contrast_scale(
    raw: &[u8],
    width: usize,
    height: usize,
    sigma_px: f64,
    radiometric: RadiometricModel,
) -> Vec<u8> {
    let fbuf: Vec<f64> = raw
        .iter()
        .map(|&v| {
            // Map digital display reflectance [0, 255] -> physical e-ink gray [35, 220].
            35.0 + (220.0 - 35.0) * (f64::from(v) / 255.0)
        })
        .collect();
    let blurred = if sigma_px > 0.1 {
        let radius = (4.0 * sigma_px).ceil() as isize;
        let mut kernel = Vec::with_capacity((2 * radius + 1) as usize);
        let mut sum = 0.0f64;
        for d in -radius..=radius {
            let w = (-0.5 * (d as f64 / sigma_px).powi(2)).exp();
            kernel.push(w);
            sum += w;
        }
        for w in &mut kernel {
            *w /= sum;
        }
        let mut tmp = vec![0.0f64; width * height];
        for y in 0..height {
            for x in 0..width {
                let mut acc = 0.0;
                for (ki, &w) in kernel.iter().enumerate() {
                    let sx = (x as isize + ki as isize - radius).clamp(0, width as isize - 1);
                    acc += w * fbuf[y * width + sx as usize];
                }
                tmp[y * width + x] = acc;
            }
        }
        let mut out = vec![0.0f64; width * height];
        for y in 0..height {
            for x in 0..width {
                let mut acc = 0.0;
                for (ki, &w) in kernel.iter().enumerate() {
                    let sy = (y as isize + ki as isize - radius).clamp(0, height as isize - 1);
                    acc += w * tmp[sy as usize * width + x];
                }
                out[y * width + x] = acc;
            }
        }
        out
    } else {
        fbuf
    };

    blurred
        .into_iter()
        .map(|lin| {
            let obs = match radiometric {
                RadiometricModel::Srgb => {
                    let u = lin / 255.0;
                    if u <= 0.003_130_8 {
                        255.0 * 12.92 * u
                    } else {
                        255.0 * (1.055 * u.powf(1.0 / 2.4) - 0.055)
                    }
                }
                _ => lin,
            };
            obs.round().clamp(0.0, 255.0) as u8
        })
        .collect()
}

fn srgb_encode_with_illumination(
    raw: &[u8],
    width: usize,
    height: usize,
    sigma_eink_dpx: f64,
    sigma_cam_px: f64,
    levels: u16,
) -> Vec<u8> {
    // Quantize display panel to 16 e-ink gray levels + microcapsule MTF.
    let quant: Vec<u8> = raw
        .iter()
        .map(|&v| {
            let l = f64::from(levels - 1);
            (((f64::from(v) / 255.0) * l).round() / l * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8
        })
        .collect();
    let total_sigma = sigma_eink_dpx.hypot(sigma_cam_px);
    blur_and_contrast_scale(&quant, width, height, total_sigma, RadiometricModel::Srgb)
}

#[test]
fn puzzleboard_dot_leakage_removed_by_photometric_refinement() {
    let spec = TargetSpec::PuzzleBoard(PuzzleBoardTargetSpec::new(10, 10, 2.4));
    let screen = ScreenTargetOptions::new(320, 320, 0.1).with_margin_px(20);
    let generated = render_screen_target(&spec, &screen).expect("screen target");
    let pitch = f64::from(generated.metadata.square_size_px);
    let origin = generated.metadata.board_origin_px;
    assert_eq!(pitch as u32, 24);

    let png_img = image::load_from_memory(&generated.png_bytes)
        .expect("decode png")
        .to_luma8();
    let (w, h) = (png_img.width() as usize, png_img.height() as usize);
    let sigma_px = 2.0;
    let data = blur_and_contrast_scale(png_img.as_raw(), w, h, sigma_px, RadiometricModel::Linear);
    let view = calib_targets::core::GrayImageView {
        width: w,
        height: h,
        data: &data,
    };

    let board = PuzzleBoardSpec::new(10, 10, 2.4).expect("valid board");
    let mut detection = detect::detect_puzzleboard(&view, &PuzzleBoardParams::for_board(board))
        .expect("blurred puzzleboard detects");
    assert!(!detection.corners.is_empty());

    let rms_err = |det: &calib_targets::puzzleboard::PuzzleBoardDetection| -> f64 {
        let mut sum2 = 0.0;
        let mut count = 0usize;
        for c in &det.corners {
            // Skip border corners that might lack enough neighbours, evaluate all detected.
            let gx = f64::from(origin[0]) + f64::from(c.grid.u) * pitch - 0.5;
            let gy = f64::from(origin[1]) + f64::from(c.grid.v) * pitch - 0.5;
            let e2 =
                (f64::from(c.position.x) - gx).powi(2) + (f64::from(c.position.y) - gy).powi(2);
            sum2 += e2;
            count += 1;
        }
        (sum2 / count as f64).sqrt()
    };
    let pre_rms = rms_err(&detection);

    let blurs = puzzleboard_blur(&view, &detection);
    let blur_vals: Vec<f32> = blurs
        .iter()
        .filter_map(|r| r.as_ref().ok().map(|b| b.sigma_px))
        .collect();
    assert!(
        !blur_vals.is_empty(),
        "puzzleboard_blur should measure interior corners"
    );

    let refinement =
        refine_puzzleboard_corners(&view, &mut detection, &BoardPhotometricOptions::default());
    let converged = refinement.corners.iter().filter(|r| r.is_ok()).count();
    assert!(converged >= 20, "converged={converged}");
    let post_rms = rms_err(&detection);
    assert!(
        post_rms < pre_rms && post_rms < 0.03,
        "pre_rms={pre_rms:.4} post_rms={post_rms:.4}"
    );
}

#[test]
fn charuco_blur_and_corner_refinement_recover_truth() {
    let spec = TargetSpec::Charuco(CharucoTargetSpec::new(
        7,
        7,
        3.5,
        0.70,
        builtins::DICT_4X4_50,
    ));
    let screen = ScreenTargetOptions::new(400, 400, 0.1).with_margin_px(25);
    let generated = render_screen_target(&spec, &screen).expect("charuco screen");
    let pitch = f64::from(generated.metadata.square_size_px);
    let origin = generated.metadata.board_origin_px;
    let board_spec = match generated.metadata.target {
        TargetSpec::Charuco(ref cs) => CharucoBoardSpec::new(
            cs.rows,
            cs.cols,
            cs.square_size_mm as f32,
            cs.marker_size_rel as f32,
            cs.dictionary,
        )
        .with_border_bits(cs.border_bits),
        _ => unreachable!(),
    };

    let png_img = image::load_from_memory(&generated.png_bytes)
        .expect("decode png")
        .to_luma8();
    let (w, h) = (png_img.width() as usize, png_img.height() as usize);
    let sigma_px = 2.0;
    let data = blur_and_contrast_scale(png_img.as_raw(), w, h, sigma_px, RadiometricModel::Linear);
    let view = calib_targets::core::GrayImageView {
        width: w,
        height: h,
        data: &data,
    };

    let mut detection = detect::detect_charuco(&view, &CharucoParams::for_board(board_spec))
        .expect("charuco detects");
    let blurs = charuco_blur(&view, &board_spec, &detection);
    let sigmas: Vec<f64> = blurs
        .iter()
        .filter_map(|r| r.as_ref().ok().map(|b| f64::from(b.sigma_px)))
        .collect();
    assert!(!sigmas.is_empty(), "charuco_blur must measure corners");
    let mean_sigma = sigmas.iter().sum::<f64>() / sigmas.len() as f64;
    let expected_total = (sigma_px * sigma_px + 1.0 / 12.0).sqrt();
    assert!(
        (mean_sigma - expected_total).abs() / expected_total < 0.06,
        "mean_sigma={mean_sigma:.3} vs expected {expected_total:.3}"
    );

    let refinement = refine_charuco_corners(
        &view,
        &board_spec,
        &mut detection,
        &BoardPhotometricOptions::default(),
    );
    assert!(refinement.corners.iter().any(|r| r.is_ok()));
    let mut sum2 = 0.0;
    let mut count = 0usize;
    for (c, res) in detection.corners.iter().zip(&refinement.corners) {
        if res.is_ok() {
            let gx = f64::from(origin[0]) + f64::from(c.grid.u) * pitch - 0.5;
            let gy = f64::from(origin[1]) + f64::from(c.grid.v) * pitch - 0.5;
            sum2 += (f64::from(c.position.x) - gx).powi(2) + (f64::from(c.position.y) - gy).powi(2);
            count += 1;
        }
    }
    let post_rms = (sum2 / count as f64).sqrt();
    assert!(post_rms < 0.04, "charuco post_rms={post_rms:.4}");
}

#[test]
fn pocketbook_eink_display_model_recovers_camera_blur_and_srgb_corners() {
    let spec = TargetSpec::PuzzleBoard(PuzzleBoardTargetSpec::new(9, 9, 3.2));
    let screen = ScreenTargetOptions::new(360, 360, 0.1).with_margin_px(20);
    let generated = render_screen_target(&spec, &screen).expect("eink screen");
    let pitch = generated.metadata.square_size_px;
    let origin = generated.metadata.board_origin_px;

    let png_img = image::load_from_memory(&generated.png_bytes)
        .expect("decode png")
        .to_luma8();
    let (w, h) = (png_img.width() as usize, png_img.height() as usize);
    let eink_mtf_dpx = 0.5;
    let cam_sigma_px = 2.0;
    let data =
        srgb_encode_with_illumination(png_img.as_raw(), w, h, eink_mtf_dpx, cam_sigma_px, 16);
    let view = calib_targets::core::GrayImageView {
        width: w,
        height: h,
        data: &data,
    };
    let board = PuzzleBoardSpec::new(9, 9, 3.2).expect("valid board");
    let mut detection = detect::detect_puzzleboard(&view, &PuzzleBoardParams::for_board(board))
        .expect("sRGB eink target detects");
    let _ = &gray_view(&png_img); // Ensure gray_view remains wired cleanly.

    let options = BoardPhotometricOptions::default().with_fit(
        PhotometricFitOptions::default()
            .with_radiometric(RadiometricModel::Srgb)
            .with_display(DisplaySurfaceModel::pocketbook_eink(f64::from(pitch))),
    );
    let refinement = refine_puzzleboard_corners(&view, &mut detection, &options);
    let cam_sigmas: Vec<f64> = refinement
        .corners
        .iter()
        .filter_map(|r| {
            let fit = r.as_ref().ok()?;
            let k = fit.camera_blur()?;
            Some((0.5 * (f64::from(k.xx) + f64::from(k.yy))).sqrt())
        })
        .collect();
    assert!(!cam_sigmas.is_empty(), "camera_blur should succeed");
    let mean_cam = cam_sigmas.iter().sum::<f64>() / cam_sigmas.len() as f64;
    // Camera sigma + pixel aperture ≈ sqrt(cam_sigma_px^2 + 1/12) ≈ 2.021 px.
    let expected_cam = (cam_sigma_px * cam_sigma_px + 1.0 / 12.0).sqrt();
    assert!(
        (mean_cam - expected_cam).abs() < 0.15,
        "mean_cam={mean_cam:.3} vs expected {expected_cam:.3}"
    );

    let mut sum2 = 0.0;
    let mut count = 0usize;
    for (c, res) in detection.corners.iter().zip(&refinement.corners) {
        if res.is_ok() {
            let gx = f64::from(origin[0]) + f64::from(c.grid.u) * f64::from(pitch) - 0.5;
            let gy = f64::from(origin[1]) + f64::from(c.grid.v) * f64::from(pitch) - 0.5;
            sum2 += (f64::from(c.position.x) - gx).powi(2) + (f64::from(c.position.y) - gy).powi(2);
            count += 1;
        }
    }
    let rms = (sum2 / count as f64).sqrt();
    assert!(rms < 0.05, "e-ink sRGB rms={rms:.4}");
}
