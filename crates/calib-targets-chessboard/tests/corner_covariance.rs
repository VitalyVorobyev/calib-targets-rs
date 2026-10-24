//! Per-corner position covariance through the corner front end and the
//! chessboard detector, on public test images.
//!
//! The front end estimates the frame's noise once, up front, and runs
//! `chess-corners` with that level filled in (or with the covariance off when
//! the estimate declines). These tests pin that this is *exactly* what
//! `chess-corners` reports on its own — positions and covariances bit for bit —
//! and that labelled corners carry the covariance of the corner they came from.

use std::path::PathBuf;

use calib_targets::detect::{default_chess_config, gray_view};
use calib_targets_chessboard::{
    detect_corner_frame, detect_corners, ChessCorner, ChessboardDetector, ChessboardParams,
    FrameNoise, PositionCovariance,
};
use chess_corners::{Detector, Uncertainty, UpscaleConfig};

fn load(name: &str) -> image::GrayImage {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata")
        .join(name);
    image::open(&path)
        .unwrap_or_else(|e| panic!("decode {}: {e}", path.display()))
        .to_luma8()
}

fn bits(c: &Option<PositionCovariance>) -> Option<[u32; 3]> {
    c.map(|c| [c.xx.to_bits(), c.xy.to_bits(), c.yy.to_bits()])
}

fn corner_bits(c: &ChessCorner) -> ([u32; 2], Option<[u32; 3]>) {
    (
        [c.position.x.to_bits(), c.position.y.to_bits()],
        bits(&c.position_covariance),
    )
}

/// `chess-corners` run directly on `img` with `cfg`, nothing pre-resolved.
fn upstream(img: &image::GrayImage, cfg: chess_corners::DetectorConfig) -> Vec<ChessCorner> {
    let corners = Detector::new(cfg)
        .unwrap()
        .detect_u8(img.as_raw(), img.width(), img.height())
        .unwrap();
    corners
        .iter()
        .map(|c| {
            ChessCorner::from_position(nalgebra::Point2::new(c.x, c.y))
                .with_position_covariance(c.position_covariance.map(PositionCovariance::from))
        })
        .collect()
}

#[test]
fn front_end_matches_upstream_bit_for_bit() {
    // small.png / mid.png: the noise estimate succeeds. oblique.png is a JPEG-
    // like render where it declines, so neither path reports a covariance.
    for (name, expect_cov) in [
        ("small.png", true),
        ("mid.png", true),
        ("puzzleboard/oblique.png", false),
    ] {
        let img = load(name);
        let cfg = default_chess_config();
        let ours = detect_corners(&gray_view(&img), &cfg);
        let theirs = upstream(&img, cfg);
        assert!(!ours.is_empty(), "{name}: no corners");
        assert_eq!(ours.len(), theirs.len(), "{name}: corner count");
        for (a, b) in ours.iter().zip(&theirs) {
            assert_eq!(corner_bits(a), corner_bits(b), "{name}: corner differs");
        }
        let with_cov = ours
            .iter()
            .filter(|c| c.position_covariance.is_some())
            .count();
        if expect_cov {
            assert_eq!(
                with_cov,
                ours.len(),
                "{name}: every corner has a covariance"
            );
        } else {
            assert_eq!(with_cov, 0, "{name}: declined estimate, no covariance");
        }

        // Positions do not depend on the uncertainty tier.
        let mut off = cfg;
        off.uncertainty = Uncertainty::Off;
        let ours_off = detect_corners(&gray_view(&img), &off);
        assert_eq!(ours_off.len(), ours.len());
        for (a, b) in ours_off.iter().zip(&ours) {
            assert_eq!(corner_bits(a).0, corner_bits(b).0, "{name}: position moved");
            assert!(a.position_covariance.is_none());
        }
    }
}

#[test]
fn frame_noise_reports_how_sigma_was_obtained() {
    let img = load("small.png");
    let view = gray_view(&img);
    let cfg = default_chess_config();

    let estimated = detect_corner_frame(&view, &cfg).noise;
    let sigma = match estimated {
        FrameNoise::Estimated(s) => s,
        other => panic!("expected an estimate on small.png, got {other:?}"),
    };
    let direct = chess_corners::estimate_noise_sigma_u8(img.as_raw(), img.width(), img.height())
        .unwrap()
        .unwrap();
    assert_eq!(sigma.to_bits(), direct.to_bits());

    let supplied = cfg.with_noise_sigma(2.0);
    assert_eq!(
        detect_corner_frame(&view, &supplied).noise,
        FrameNoise::Supplied(2.0)
    );

    let mut off = cfg;
    off.uncertainty = Uncertainty::Off;
    assert_eq!(
        detect_corner_frame(&view, &off).noise,
        FrameNoise::NotRequested
    );

    // Upscaling correlates the noise of interpolated pixels: no covariance,
    // and no estimate is made for it.
    let mut up = cfg;
    up.upscale = UpscaleConfig::fixed(2);
    let frame = detect_corner_frame(&view, &up);
    assert_eq!(frame.noise, FrameNoise::NotRequested);
    assert!(!frame.corners.is_empty());
    assert!(frame
        .corners
        .iter()
        .all(|c| c.position_covariance.is_none()));

    let oblique = load("puzzleboard/oblique.png");
    assert_eq!(
        detect_corner_frame(&gray_view(&oblique), &cfg).noise,
        FrameNoise::Declined
    );
}

#[test]
fn covariance_is_deterministic() {
    let img = load("small.png");
    let cfg = default_chess_config();
    let a = detect_corners(&gray_view(&img), &cfg);
    let b = detect_corners(&gray_view(&img), &cfg);
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(&b) {
        assert_eq!(corner_bits(x), corner_bits(y));
    }
}

#[test]
fn labelled_corners_carry_their_input_covariance() {
    let img = load("mid.png");
    let corners = detect_corners(&gray_view(&img), &default_chess_config());
    let mut params = ChessboardParams::default();
    params.min_corner_strength = 0.5;
    let detection = ChessboardDetector::new(params)
        .unwrap()
        .detect(&corners)
        .expect("mid.png detects");
    assert!(detection.corners.len() > 20);
    let mut with_cov = 0;
    for c in &detection.corners {
        let input = &corners[c.input_index];
        assert_eq!(
            bits(&c.position_covariance),
            bits(&input.position_covariance)
        );
        if let Some(cov) = c.position_covariance {
            with_cov += 1;
            // A sane localization: sub-pixel, positive definite.
            let s = cov.sigma_max_px();
            assert!(s > 0.0 && s < 0.5, "sigma_max {s} px");
            assert!(cov.mahalanobis_sq(0.1, 0.1).is_some());
        }
    }
    // The default config gives most labelled corners a covariance.
    assert!(
        with_cov * 10 >= detection.corners.len() * 9,
        "{with_cov} of {} labelled corners carry a covariance",
        detection.corners.len()
    );
    // And the shared carrier keeps it.
    let labeled = detection.corners[0].to_labeled();
    assert_eq!(
        bits(&labeled.position_covariance),
        bits(&detection.corners[0].position_covariance)
    );
}
