//! Criterion bench: chessboard grid build on upscaled snaps drawn
//! from a private PuzzleBoard regression dataset.
//!
//! Phase-2 companion to `dataset_corners.rs`. Amortizes ChESS corner
//! detection into setup and measures only
//! `Detector::detect_debug(&corners)` on a spread of snaps. Skips
//! silently when the private dataset is absent; override the default
//! path with `CALIB_PUZZLE_PRIVATE_DATASET`.
//!
//! Run with:
//! ```text
//! cargo bench -p calib-targets-chessboard --bench dataset_chessboard
//! ```

use std::path::PathBuf;

use calib_targets::detect::{default_chess_config, detect_corners};
use calib_targets_chessboard::{Detector, DetectorParams};
use calib_targets_core::Corner;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use image::imageops::FilterType;
use image::{GenericImageView, GrayImage};

const SNAP_WIDTH: u32 = 720;
const SNAP_HEIGHT: u32 = 540;

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

fn bench_grid(c: &mut Criterion) {
    let mut group = c.benchmark_group("chessboard/dataset/upscale2");
    let params = DetectorParams::default();
    let cfg = default_chess_config();
    let upscale = 2u32;
    let mut loaded_any = false;
    for (t, s, label) in FIXTURES {
        let Some(snap) = load_snap(*t, *s, upscale) else {
            eprintln!(
                "skipping {label}: target_{t}.png missing under {:?} — \
                 set CALIB_PUZZLE_PRIVATE_DATASET to point at the dataset",
                dataset_dir()
            );
            continue;
        };
        let corners: Vec<Corner> = detect_corners(&snap, &cfg, 0.0);
        loaded_any = true;
        group.bench_with_input(
            BenchmarkId::from_parameter(label),
            &corners,
            |b, corners| {
                let detector = Detector::new(params.clone());
                b.iter(|| {
                    let frame = detector.detect_debug(corners);
                    criterion::black_box(frame)
                });
            },
        );
    }
    if !loaded_any {
        eprintln!(
            "dataset_chessboard: no fixtures loaded — skipping (dir={:?}).",
            dataset_dir()
        );
    }
    group.finish();
}

criterion_group!(benches, bench_grid);
criterion_main!(benches);
