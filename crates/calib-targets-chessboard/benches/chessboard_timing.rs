//! Criterion bench: per-frame chessboard-v2 detection timing.
//!
//! Times the corner-to-detection pipeline on representative sub-frames from
//! the 3536119669 dataset:
//!
//! - `target_0 snap 0`  — a clean, well-lit frame (baseline case).
//! - `target_5 snap 2`  — moderate difficulty.
//! - `target_11 snap 2` — the frame flagged as near-failure in `docs/120issues.txt`.
//! - `target_19 snap 5` — a representative mid-dataset frame.
//!
//! The bench measures ONLY the `Detector::detect(&corners)` step — ChESS
//! corner detection is amortized into the setup phase so we measure the
//! grid-assembly pipeline in isolation.
//!
//! Run with:
//! ```text
//! cargo bench -p calib-targets-chessboard --bench chessboard_timing
//! ```

use std::path::PathBuf;

use calib_targets::detect::{default_chess_config, detect_corners};
use calib_targets_chessboard::{Detector, DetectorParams};
use calib_targets_core::Corner;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use image::GenericImageView;

const SNAP_WIDTH: u32 = 720;
const SNAP_HEIGHT: u32 = 540;

const FIXTURES: &[(u32, u32, &str)] = &[
    (0, 0, "clean_t0s0"),
    (5, 2, "moderate_t5s2"),
    (11, 2, "hard_t11s2"),
    (19, 5, "mid_t19s5"),
];

fn dataset_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/3536119669")
}

fn load_snap_corners(target_idx: u32, snap_idx: u32) -> Option<Vec<Corner>> {
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
    let cfg = default_chess_config();
    Some(detect_corners(&snap, &cfg))
}

fn bench_detection(c: &mut Criterion) {
    let mut group = c.benchmark_group("chessboard/detect");
    let params = DetectorParams::default();
    for (t, s, label) in FIXTURES {
        let Some(corners) = load_snap_corners(*t, *s) else {
            eprintln!("skipping {label}: target_{t}.png missing — run benches from repo root");
            continue;
        };
        group.bench_with_input(
            BenchmarkId::from_parameter(label),
            &corners,
            |b, corners| {
                let detector = Detector::new(params.clone());
                b.iter(|| {
                    let det = detector.detect(corners);
                    criterion::black_box(det)
                });
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_detection);
criterion_main!(benches);
