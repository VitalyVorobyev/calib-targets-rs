//! Criterion bench: PuzzleBoard decode stages on upscaled snaps
//! drawn from a private regression dataset.
//!
//! Phase-3 companion to the chessboard `dataset_corners` and
//! `dataset_chessboard` benches. Splits the measurement into three
//! groups so we can see which stage dominates:
//!
//!   - `puzzleboard/dataset/corners` — ChESS corner detection only.
//!   - `puzzleboard/dataset/chessboard` — chessboard graph build on
//!     cached corners.
//!   - `puzzleboard/dataset/decode` — `PuzzleBoardDetector::detect` on
//!     cached corners (includes the internal chessboard rerun; no way
//!     to isolate the decode further without API surgery).
//!
//! Skips silently when the dataset is missing. Point at a custom
//! location via `CALIB_PUZZLE_PRIVATE_DATASET`.

use std::path::PathBuf;

use calib_targets::detect::{default_chess_config, detect_corners, gray_view};
use calib_targets_chessboard::{Detector as ChessDetector, DetectorParams};
use calib_targets_core::Corner;
use calib_targets_puzzleboard::{PuzzleBoardDetector, PuzzleBoardParams, PuzzleBoardSpec};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use image::imageops::FilterType;
use image::{GenericImageView, GrayImage};

const SNAP_WIDTH: u32 = 720;
const SNAP_HEIGHT: u32 = 540;

const BOARD_ROWS: u32 = 130;
const BOARD_COLS: u32 = 130;
const BOARD_CELL_MM: f32 = 1.014;

const FIXTURES: &[(u32, u32, &str)] = &[
    (0, 0, "t0s0"),
    (5, 2, "t5s2"),
    (11, 3, "t11s3"),
    (19, 5, "t19s5"),
];

fn dataset_dir() -> PathBuf {
    if let Ok(custom) = std::env::var("CALIB_PUZZLE_PRIVATE_DATASET") {
        return PathBuf::from(custom);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../privatedata/130x130_puzzle")
}

fn load_snap(target_idx: u32, snap_idx: u32, upscale: u32) -> Option<GrayImage> {
    let path = dataset_dir().join(format!("target_{target_idx}.png"));
    if !path.exists() {
        return None;
    }
    let img = image::open(&path).ok()?.to_luma8();
    let x0 = snap_idx * SNAP_WIDTH;
    if x0 + SNAP_WIDTH > img.width() || img.height() < SNAP_HEIGHT {
        return None;
    }
    let snap = img.view(x0, 0, SNAP_WIDTH, SNAP_HEIGHT).to_image();
    if upscale == 1 {
        return Some(snap);
    }
    Some(image::imageops::resize(
        &snap,
        SNAP_WIDTH * upscale,
        SNAP_HEIGHT * upscale,
        FilterType::Triangle,
    ))
}

fn bench_all(c: &mut Criterion) {
    let cfg = default_chess_config();
    let chess_params = DetectorParams::default();
    let upscale = 2u32;
    let spec = PuzzleBoardSpec::with_origin(BOARD_ROWS, BOARD_COLS, BOARD_CELL_MM, 0, 0)
        .expect("build spec");
    let puzzle_params: PuzzleBoardParams = PuzzleBoardParams::for_board(&spec);

    let fixtures: Vec<(String, GrayImage, Vec<Corner>)> = FIXTURES
        .iter()
        .filter_map(|(t, s, label)| {
            let snap = load_snap(*t, *s, upscale)?;
            let corners = detect_corners(&snap, &cfg, 0.0);
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

    let mut corners_group = c.benchmark_group("puzzleboard/dataset/corners");
    for (label, snap, _) in &fixtures {
        corners_group.bench_with_input(BenchmarkId::from_parameter(label), snap, |b, snap| {
            b.iter(|| {
                let out = detect_corners(snap, &cfg, 0.0);
                criterion::black_box(out)
            });
        });
    }
    corners_group.finish();

    let mut chess_group = c.benchmark_group("puzzleboard/dataset/chessboard");
    for (label, _, corners) in &fixtures {
        chess_group.bench_with_input(BenchmarkId::from_parameter(label), corners, |b, corners| {
            let detector = ChessDetector::new(chess_params.clone());
            b.iter(|| {
                let frame = detector.detect_debug(corners);
                criterion::black_box(frame)
            });
        });
    }
    chess_group.finish();

    let mut decode_group = c.benchmark_group("puzzleboard/dataset/decode");
    for (label, snap, corners) in &fixtures {
        decode_group.bench_with_input(
            BenchmarkId::from_parameter(label),
            &(snap, corners),
            |b, (snap, corners)| {
                let detector =
                    PuzzleBoardDetector::new(puzzle_params.clone()).expect("puzzle detector");
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
