//! Criterion bench: ChArUco decode stages on snaps drawn from a private
//! regression dataset.
//!
//! Companion to the chessboard `dataset_corners` / `dataset_chessboard`
//! and the puzzleboard `dataset_decode` benches. Splits the measurement
//! into three groups so we can see which stage dominates:
//!
//!   - `charuco/dataset/corners` — ChESS corner detection only.
//!   - `charuco/dataset/chessboard` — chessboard graph build on cached
//!     corners.
//!   - `charuco/dataset/decode` — `CharucoDetector::detect` on cached
//!     corners (includes the internal chessboard rerun + the board-level
//!     marker matcher; decode cost ≈ total − corners − chessboard).
//!
//! Skips silently when the dataset is missing. Point at a custom
//! location via `CALIB_CHARUCO_PRIVATE_DATASET` (must contain
//! `target_*.png` plus a `board.json` spec).

use std::path::PathBuf;

use calib_targets::detect::{default_chess_config, detect_corners, gray_view};
use calib_targets_charuco::{load_board_spec_any, CharucoDetector, CharucoParams};
use calib_targets_chessboard::ChessCorner as Corner;
use calib_targets_chessboard::{Detector as ChessDetector, DetectorParams};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use image::{GenericImageView, GrayImage};

const SNAP_WIDTH: u32 = 720;
const SNAP_HEIGHT: u32 = 540;

/// Representative `(target, snap, label)` spread over the 20×6-snap
/// flagship set (native resolution — no upscale).
const FIXTURES: &[(u32, u32, &str)] = &[
    (0, 0, "t0s0"),
    (5, 2, "t5s2"),
    (11, 3, "t11s3"),
    (19, 5, "t19s5"),
];

fn dataset_dir() -> PathBuf {
    if let Ok(custom) = std::env::var("CALIB_CHARUCO_PRIVATE_DATASET") {
        return PathBuf::from(custom);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../privatedata/3536119669")
}

fn load_snap(target_idx: u32, snap_idx: u32) -> Option<GrayImage> {
    let path = dataset_dir().join(format!("target_{target_idx}.png"));
    if !path.exists() {
        return None;
    }
    let img = image::open(&path).ok()?.to_luma8();
    let x0 = snap_idx * SNAP_WIDTH;
    if x0 + SNAP_WIDTH > img.width() || img.height() < SNAP_HEIGHT {
        return None;
    }
    Some(img.view(x0, 0, SNAP_WIDTH, SNAP_HEIGHT).to_image())
}

fn bench_all(c: &mut Criterion) {
    let board_path = dataset_dir().join("board.json");
    let Ok(spec) = load_board_spec_any(&board_path) else {
        eprintln!(
            "dataset_decode: board spec missing — skipping (expected {:?}).",
            board_path
        );
        return;
    };

    let cfg = default_chess_config();
    let chess_params = DetectorParams::default();
    // `for_board` defaults to the production board-level matcher.
    let charuco_params = CharucoParams::for_board(&spec);

    let fixtures: Vec<(String, GrayImage, Vec<Corner>)> = FIXTURES
        .iter()
        .filter_map(|(t, s, label)| {
            let snap = load_snap(*t, *s)?;
            let corners = detect_corners(&snap, &cfg);
            Some(((*label).to_string(), snap, corners))
        })
        .collect();

    if fixtures.is_empty() {
        eprintln!(
            "dataset_decode: no fixtures loaded — skipping (dir={:?}).",
            dataset_dir()
        );
        return;
    }

    let mut corners_group = c.benchmark_group("charuco/dataset/corners");
    for (label, snap, _) in &fixtures {
        corners_group.bench_with_input(BenchmarkId::from_parameter(label), snap, |b, snap| {
            b.iter(|| {
                let out = detect_corners(snap, &cfg);
                criterion::black_box(out)
            });
        });
    }
    corners_group.finish();

    let mut chess_group = c.benchmark_group("charuco/dataset/chessboard");
    for (label, _, corners) in &fixtures {
        chess_group.bench_with_input(BenchmarkId::from_parameter(label), corners, |b, corners| {
            let detector = ChessDetector::new(chess_params.clone()).expect("valid detector params");
            b.iter(|| {
                let detection = detector.detect(corners);
                criterion::black_box(detection)
            });
        });
    }
    chess_group.finish();

    let mut decode_group = c.benchmark_group("charuco/dataset/decode");
    for (label, snap, corners) in &fixtures {
        decode_group.bench_with_input(
            BenchmarkId::from_parameter(label),
            &(snap, corners),
            |b, (snap, corners)| {
                let detector =
                    CharucoDetector::new(charuco_params.clone()).expect("charuco detector");
                let view = gray_view(snap);
                b.iter(|| {
                    let out = detector.detect(&view, corners);
                    criterion::black_box(out)
                });
            },
        );
    }
    decode_group.finish();
}

criterion_group!(benches, bench_all);
criterion_main!(benches);
