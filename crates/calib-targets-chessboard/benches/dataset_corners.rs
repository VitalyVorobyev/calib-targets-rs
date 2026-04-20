//! Criterion bench: ChESS corner detection on a real-world
//! high-density puzzle snap.
//!
//! Phase-1 companion to the private PuzzleBoard dataset runner.
//! Times `detect_corners` on a representative 720×540 snap upscaled
//! by 2× (the minimum factor at which ChESS responses fire reliably
//! on very small cells). The snap is re-loaded and re-upscaled on
//! every criterion iteration group setup; the measured step is only
//! the detector call, so the bench reflects production per-frame
//! cost.
//!
//! Private data is not committed. This bench silently skips when the
//! dataset is absent (in CI or on machines without the private tree),
//! matching the convention of `chessboard_timing.rs`. Point at a
//! custom location via `CALIB_PUZZLE_PRIVATE_DATASET` if the default
//! path is not your layout.
//!
//! Run with:
//! ```text
//! cargo bench -p calib-targets-chessboard --bench dataset_corners
//! ```

use std::path::PathBuf;

use calib_targets::detect::{default_chess_config, detect_corners};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use image::imageops::FilterType;
use image::{GenericImageView, GrayImage};

const SNAP_WIDTH: u32 = 720;
const SNAP_HEIGHT: u32 = 540;

/// Representative snaps to measure — pick a spread of difficulty once
/// the user has reported which frames are easy / marginal / failing.
/// Until then, benchmark four evenly-spaced frames across the dataset.
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

fn bench_corners(c: &mut Criterion) {
    let mut group = c.benchmark_group("corners/dataset/upscale2");
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
        loaded_any = true;
        group.bench_with_input(BenchmarkId::from_parameter(label), &snap, |b, snap| {
            b.iter(|| {
                let corners = detect_corners(snap, &cfg);
                criterion::black_box(corners)
            });
        });
    }
    if !loaded_any {
        eprintln!(
            "dataset_corners: no fixtures loaded — skipping (dir={:?}).",
            dataset_dir()
        );
    }
    group.finish();
}

criterion_group!(benches, bench_corners);
criterion_main!(benches);
