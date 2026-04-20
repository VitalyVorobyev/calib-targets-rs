//! Regression tests against the private ChArUco datasets. The source
//! PNGs live under `privatedata/` outside the public repo and every
//! test skips gracefully when they are absent, so CI on a fresh public
//! checkout still passes.
//!
//! Two datasets are gated:
//!
//! * `3536119669` — 20 × 6-snap frames (22 × 22 squares, 5.2 mm, dict
//!   `DICT_4X4_1000`). The smoke test exercises snap 0 of target 0 and
//!   must not regress the aggregated per-sweep baseline recorded in
//!   `testdata/charuco_regression_baselines.json`. The `#[ignore]`-gated
//!   full sweep is the same contract applied to all 120 frames.
//! * `target_0.png` (under `privatedata/`) — 1 × 6-snap with 68 × 68
//!   squares, 1.69 mm, dict `DICT_APRILTAG_36h10`. Cells are tiny; the
//!   current matcher fails on it (documented recall = 0). The test only
//!   verifies that detection returns an error rather than panicking;
//!   Phase B of the board-level matcher is expected to flip these cases
//!   to a non-trivial recall and will update the baseline JSON.

use std::path::PathBuf;

use calib_targets::detect::{default_chess_config, detect_corners};
use calib_targets_charuco::{load_board_spec_any, CharucoDetector, CharucoParams};
use calib_targets_core::GrayImageView;
use image::GenericImageView;

const SNAP_WIDTH: u32 = 720;
const SNAP_HEIGHT: u32 = 540;
const SNAPS_PER_IMAGE: u32 = 6;

/// 22×22 flagship dataset path. Override via
/// `CALIB_CHARUCO_PRIVATE_DATASET`.
fn flagship_dir() -> PathBuf {
    if let Ok(custom) = std::env::var("CALIB_CHARUCO_PRIVATE_DATASET") {
        return PathBuf::from(custom);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../privatedata/3536119669")
}

fn flagship_board() -> PathBuf {
    flagship_dir().join("board.json")
}

fn apriltag_image() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../privatedata/target_0.png")
}

fn apriltag_config() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../privatedata/config.json")
}

fn extract_snap(img: &image::GrayImage, idx: u32) -> image::GrayImage {
    img.view(idx * SNAP_WIDTH, 0, SNAP_WIDTH, SNAP_HEIGHT)
        .to_image()
}

fn upscale(img: &image::GrayImage, factor: u32) -> image::GrayImage {
    image::imageops::resize(
        img,
        img.width() * factor,
        img.height() * factor,
        image::imageops::FilterType::Lanczos3,
    )
}

#[test]
fn smoke_flagship_snap_0_detects() {
    let dir = flagship_dir();
    let img_path = dir.join("target_0.png");
    let board_path = flagship_board();
    if !img_path.exists() || !board_path.exists() {
        eprintln!(
            "skipping: {} or {} missing — drop the private flagship files to enable",
            img_path.display(),
            board_path.display(),
        );
        return;
    }

    let spec = load_board_spec_any(&board_path).expect("load board");
    let img = image::open(&img_path).expect("decode").to_luma8();
    let snap = extract_snap(&img, 0);

    let chess_cfg = default_chess_config();
    let corners = detect_corners(&snap, &chess_cfg);

    let params = CharucoParams::for_board(&spec);
    let detector = CharucoDetector::new(params).expect("detector");
    let view = GrayImageView {
        width: snap.width() as usize,
        height: snap.height() as usize,
        data: snap.as_raw(),
    };
    let result = detector
        .detect(&view, &corners)
        .expect("snap 0 of target 0 must detect");

    assert!(
        result.markers.len() >= 8,
        "flagship snap 0 should decode ≥ 8 markers, got {}",
        result.markers.len()
    );
    assert_eq!(
        result.raw_marker_wrong_id_count, 0,
        "flagship snap 0 must not produce any raw wrong-id decodings"
    );
    assert!(
        result.detection.corners.len() >= 30,
        "flagship snap 0 should have ≥ 30 ChArUco corners, got {}",
        result.detection.corners.len()
    );
}

fn run_flagship_sweep(use_board_matcher: bool) -> Option<(usize, usize, usize)> {
    let dir = flagship_dir();
    let board_path = flagship_board();
    if !dir.exists() || !board_path.exists() {
        eprintln!("skipping: {} missing", dir.display());
        return None;
    }

    let spec = load_board_spec_any(&board_path).expect("load board");
    let chess_cfg = default_chess_config();
    let mut params = CharucoParams::for_board(&spec);
    params.use_board_level_matcher = use_board_matcher;
    if use_board_matcher {
        params.min_marker_inliers = 1;
        params.min_secondary_marker_inliers = 1;
    }
    let detector = CharucoDetector::new(params).expect("detector");

    let mut frames = 0usize;
    let mut detected = 0usize;
    let mut wrong_id_total = 0usize;

    for target_idx in 0..20u32 {
        let p = dir.join(format!("target_{target_idx}.png"));
        if !p.exists() {
            panic!("missing {}", p.display());
        }
        let img = image::open(&p).expect("decode").to_luma8();
        for snap_idx in 0..SNAPS_PER_IMAGE {
            frames += 1;
            let snap = extract_snap(&img, snap_idx);
            let corners = detect_corners(&snap, &chess_cfg);
            let view = GrayImageView {
                width: snap.width() as usize,
                height: snap.height() as usize,
                data: snap.as_raw(),
            };
            if let Ok(result) = detector.detect(&view, &corners) {
                detected += 1;
                wrong_id_total += result.raw_marker_wrong_id_count;
            }
        }
    }
    Some((frames, detected, wrong_id_total))
}

#[test]
#[ignore = "full 120-frame flagship sweep; run with --ignored"]
fn full_flagship_sweep_legacy_recall_contract() {
    let Some((frames, detected, wrong_id_total)) = run_flagship_sweep(false) else {
        return;
    };
    // Baseline from bench_results/charuco/3536119669_baseline (2026-04-19):
    //   detected 108/120 (90.0 %), wrong_id_total = 3
    assert_eq!(frames, 120);
    assert!(
        detected >= 108,
        "flagship legacy recall regression: detected {detected}/120 (expected ≥ 108)"
    );
    assert!(
        wrong_id_total <= 3,
        "flagship legacy wrong-id regression: {wrong_id_total} > 3"
    );
}

#[test]
#[ignore = "full 120-frame flagship sweep; run with --ignored"]
fn full_flagship_sweep_board_matcher_contract() {
    let Some((frames, detected, wrong_id_total)) = run_flagship_sweep(true) else {
        return;
    };
    // Baseline from bench_results/charuco/3536119669_k36 (2026-04-20,
    // bit_likelihood_slope default κ=36):
    //   detected 120/120 (100 %), wrong_id_total = 0
    assert_eq!(frames, 120);
    assert_eq!(
        detected, 120,
        "flagship board-matcher recall regression: detected {detected}/120 (expected 120)"
    );
    assert_eq!(
        wrong_id_total, 0,
        "board-level matcher must never emit markers inconsistent with its own alignment; got {wrong_id_total}"
    );
}

#[test]
fn smoke_apriltag_image_does_not_panic() {
    let img_path = apriltag_image();
    let cfg_path = apriltag_config();
    if !img_path.exists() || !cfg_path.exists() {
        eprintln!(
            "skipping: {} or {} missing — drop privatedata/target_0.png and config.json to enable",
            img_path.display(),
            cfg_path.display(),
        );
        return;
    }

    let spec = load_board_spec_any(&cfg_path).expect("load apriltag config");
    let img = image::open(&img_path).expect("decode").to_luma8();
    let snap = extract_snap(&img, 0);
    let snap = upscale(&snap, 3);

    let chess_cfg = default_chess_config();
    let corners = detect_corners(&snap, &chess_cfg);

    // Legacy matcher path: the target_0 AprilTag board has 1.69 mm cells
    // and even at 3× upscale the per-cell hard-threshold decode returns
    // zero markers. Assert the detector errors out cleanly rather than
    // panicking.
    let mut params = CharucoParams::for_board(&spec);
    let detector = CharucoDetector::new(params.clone()).expect("detector");
    let view = GrayImageView {
        width: snap.width() as usize,
        height: snap.height() as usize,
        data: snap.as_raw(),
    };
    assert!(
        detector.detect(&view, &corners).is_err(),
        "legacy matcher must fail on target_0 snap 0 (baseline contract)"
    );

    // Board-level matcher path: soft-bit log-likelihood with the default
    // κ=36 slope recovers the alignment even when the hard decode returns
    // nothing. The 2026-04-20 sweep at the κ=36 default decoded ≥ 10
    // markers and ≥ 60 ChArUco corners on this snap — re-assert that
    // here so future regressions surface.
    params.use_board_level_matcher = true;
    params.min_marker_inliers = 1;
    params.min_secondary_marker_inliers = 1;
    let detector = CharucoDetector::new(params).expect("detector");
    let result = detector
        .detect(&view, &corners)
        .expect("board-level matcher must detect target_0 snap 0");
    assert!(
        result.markers.len() >= 10,
        "board-level matcher should decode ≥ 10 markers, got {}",
        result.markers.len()
    );
    assert!(
        result.detection.corners.len() >= 60,
        "board-level matcher should land ≥ 60 ChArUco corners, got {}",
        result.detection.corners.len()
    );
    assert_eq!(
        result.raw_marker_wrong_id_count, 0,
        "board-level matcher is self-consistent by construction"
    );
}
