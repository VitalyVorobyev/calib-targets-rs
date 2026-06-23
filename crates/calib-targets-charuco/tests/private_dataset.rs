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
//!   squares, 1.69 mm, dict `DICT_APRILTAG_36h10`. Cells are tiny; on the
//!   topological builder the board-level matcher recovers a small,
//!   self-consistent subset (≈ 3 markers / 20 corners at 3× upscale). The
//!   test asserts the decode does not collapse to nothing and stays
//!   self-consistent (`wrong_id == 0`); closing the dense/tiny-cell recall
//!   gap is a deferred topological-decode improvement.

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
    let (result, diagnostics) = detector.detect_with_diagnostics(&view, &corners);
    let result = result.expect("snap 0 of target 0 must detect");

    assert!(
        result.markers.len() >= 8,
        "flagship snap 0 should decode ≥ 8 markers, got {}",
        result.markers.len()
    );
    assert_eq!(
        diagnostics.raw_marker_wrong_id_count, 0,
        "flagship snap 0 must not produce any raw wrong-id decodings"
    );
    assert!(
        result.corners.len() >= 30,
        "flagship snap 0 should have ≥ 30 ChArUco corners, got {}",
        result.corners.len()
    );
}

/// Run the full 120-frame flagship sweep on the (only) topological builder.
/// Returns `(frames, detected, wrong_id_total)`, or `None` when the private
/// dataset is absent.
fn run_flagship_sweep() -> Option<(usize, usize, usize)> {
    let dir = flagship_dir();
    let board_path = flagship_board();
    if !dir.exists() || !board_path.exists() {
        eprintln!("skipping: {} missing", dir.display());
        return None;
    }

    let spec = load_board_spec_any(&board_path).expect("load board");
    let chess_cfg = default_chess_config();
    let mut params = CharucoParams::for_board(&spec);
    params.min_marker_inliers = 1;
    params.min_secondary_marker_inliers = 1;
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
            let (result, diagnostics) = detector.detect_with_diagnostics(&view, &corners);
            if result.is_ok() {
                detected += 1;
                wrong_id_total += diagnostics.raw_marker_wrong_id_count;
            }
        }
    }
    Some((frames, detected, wrong_id_total))
}

#[test]
#[ignore = "full 120-frame flagship sweep; run with --ignored"]
fn full_flagship_sweep_board_matcher_contract() {
    // THE flagship board-level-matcher contract (the sole matcher), on the
    // topological builder (the only builder; ChArUco no longer pins
    // seed-and-grow). The 2026-06-13 head-to-head showed topological decode
    // precision matches the historical seed-and-grow gold — zero
    // self-consistency wrong-ids.
    //
    // Recall floor is `>= 119`, not `== 120`: a deterministic lexicographic
    // tie-break in the multi-component merge fixed one `HashMap`
    // iteration-order site (three separate-process runs then gave 120/120/120),
    // but the full single-process `--ignored` suite still tips to 119 on at
    // least one more seed-dependent site in the topological→charuco path.
    // Precision is solid regardless (`wrong_id == 0`); the floor stays `>= 119`
    // until determinism is fully hardened.
    let Some((frames, detected, wrong_id_total)) = run_flagship_sweep() else {
        return;
    };
    assert_eq!(frames, 120);
    assert!(
        detected >= 119,
        "flagship board-matcher recall regression: detected {detected}/120 (expected >= 119)"
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

    let view = GrayImageView {
        width: snap.width() as usize,
        height: snap.height() as usize,
        data: snap.as_raw(),
    };

    // Board-level matcher path (the sole matcher): soft-bit log-likelihood
    // with the default κ=36 slope recovers an alignment even when a hard
    // per-cell decode would return nothing.
    //
    // RECALL FLOOR re-baselined to the TOPOLOGICAL builder (the workspace
    // default; ChArUco no longer pins seed-and-grow). On this 68×68
    // DICT_APRILTAG_36h10 board (1.69 mm cells, 3× upscale) the topological
    // cell test is defeated by the dense marker-internal bits at tiny cell
    // sizes — it recovers ≈ 3 markers / 20 corners here, versus seed-and-grow's
    // ≈ 14 / 76. That recall loss is an accepted consequence of retiring
    // seed-and-grow (a *miss* is allowed by the asymmetric detection contract;
    // a false positive is not — and the matcher stays self-consistent,
    // `wrong_id == 0`). Closing the dense/tiny-cell gap is a deferred
    // topological-decode improvement, NOT a regression to guard against here;
    // this floor only asserts the decode does not collapse to nothing and stays
    // self-consistent.
    let mut params = CharucoParams::for_board(&spec);
    params.min_marker_inliers = 1;
    params.min_secondary_marker_inliers = 1;
    let detector = CharucoDetector::new(params).expect("detector");
    let (result, diagnostics) = detector.detect_with_diagnostics(&view, &corners);
    let result = result.expect("board-level matcher must detect target_0 snap 0");
    assert!(
        result.markers.len() >= 2,
        "board-level matcher should decode ≥ 2 markers on topological, got {} \
         (measured 3; seed-and-grow reached 14 — dense-board topological gap is deferred)",
        result.markers.len()
    );
    assert!(
        result.corners.len() >= 12,
        "board-level matcher should land ≥ 12 ChArUco corners on topological, got {} \
         (measured 20; seed-and-grow reached 76 — dense-board topological gap is deferred)",
        result.corners.len()
    );
    assert_eq!(
        diagnostics.raw_marker_wrong_id_count, 0,
        "board-level matcher is self-consistent by construction"
    );
}

/// Owner-reviewed marker-bit false corners on the 22×22 flagship set:
/// weak ChESS responses on defocused ArUco bits that align with a grid
/// extrapolation and survive into the ChArUco product as biased corners.
/// Pixel positions are at `upscale = 1`. The `min_corner_strength = 33`
/// floor in [`CharucoParams::for_board`] must keep all of these out of the
/// product. Counterpart to the chessboard-side
/// `private_3536119669.rs::topological_rejects_reviewed_3536119669_false_labels`.
type FalsePx = (f32, f32);
type FalsePxCase = (u32, u32, &'static [FalsePx]);

const REVIEWED_FALSE_PX: &[FalsePxCase] = &[
    (13, 5, &[(411.9, 429.1), (474.3, 435.9)]),
    (15, 3, &[(90.6, 108.7)]),
    (18, 0, &[(439.5, 130.8)]),
    (18, 5, &[(493.8, 449.3), (553.2, 460.4)]),
];

#[test]
fn flagship_rejects_reviewed_marker_bit_false_corners() {
    // The topological builder + `min_corner_strength = 33` floor must keep
    // every reviewed marker-bit false corner out of the product.
    assert_reviewed_false_corners_rejected();
}

fn assert_reviewed_false_corners_rejected() {
    let dir = flagship_dir();
    let board_path = flagship_board();
    if !dir.exists() || !board_path.exists() {
        eprintln!("skipping: {} missing", dir.display());
        return;
    }
    let spec = load_board_spec_any(&board_path).expect("load board");
    let chess_cfg = default_chess_config();
    let mut params = CharucoParams::for_board(&spec);
    params.min_marker_inliers = 1;
    params.min_secondary_marker_inliers = 1;
    let detector = CharucoDetector::new(params).expect("detector");

    for &(target_idx, snap_idx, falses) in REVIEWED_FALSE_PX {
        let img = image::open(dir.join(format!("target_{target_idx}.png")))
            .expect("decode")
            .to_luma8();
        let snap = extract_snap(&img, snap_idx);
        let corners = detect_corners(&snap, &chess_cfg);
        let view = GrayImageView {
            width: snap.width() as usize,
            height: snap.height() as usize,
            data: snap.as_raw(),
        };
        let Ok(result) = detector.detect(&view, &corners) else {
            // A missing detection trivially carries no false corner.
            continue;
        };
        for &(fx, fy) in falses {
            let nearest = result
                .corners
                .iter()
                .map(|c| ((c.position.x - fx).powi(2) + (c.position.y - fy).powi(2)).sqrt())
                .fold(f32::INFINITY, f32::min);
            assert!(
                nearest > 8.0,
                "t{target_idx}s{snap_idx}: marker-bit false corner at \
                 ({fx:.0},{fy:.0}) survived into product (nearest {nearest:.1} px)"
            );
        }
    }
}
