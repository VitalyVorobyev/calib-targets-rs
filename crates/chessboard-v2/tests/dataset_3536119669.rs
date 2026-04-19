//! Regression tests against the 3536119669 dataset.
//!
//! The smoke test (`smoke_first_subframe_detects`) runs the detector on a
//! single 720×540 sub-frame and asserts it produces a labelled grid without
//! panicking. This is cheap and runs in every `cargo test --workspace`.
//!
//! The full-sweep test (`full_dataset_precision_contract`) processes all 120
//! sub-frames (20 targets × 6 snaps) and enforces the precision contract:
//! ≥ 119 detections and no duplicate `(i, j)` labels in any detection. It is
//! marked `#[ignore]` because it reads 20 images and is slow; run it with
//! `cargo test -p chessboard-v2 --release -- --ignored`.

use std::collections::HashSet;
use std::path::PathBuf;

use calib_targets::detect::{default_chess_config, detect_corners};
use chessboard_v2::{Detector, DetectorParams};
use image::GenericImageView;

const SNAP_WIDTH: u32 = 720;
const SNAP_HEIGHT: u32 = 540;
const SNAPS_PER_IMAGE: u32 = 6;
const NUM_TARGETS: u32 = 20;
const MIN_DETECTIONS: usize = 119;

fn dataset_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/3536119669")
}

fn target_path(idx: u32) -> PathBuf {
    dataset_dir().join(format!("target_{idx}.png"))
}

fn extract_snap(image: &image::GrayImage, snap_idx: u32) -> image::GrayImage {
    let x0 = snap_idx * SNAP_WIDTH;
    image.view(x0, 0, SNAP_WIDTH, SNAP_HEIGHT).to_image()
}

fn assert_no_duplicate_labels(detection: &chessboard_v2::Detection, context: &str) {
    let mut seen: HashSet<(i32, i32)> = HashSet::new();
    for lc in &detection.target.corners {
        let g = lc
            .grid
            .expect("chessboard-v2 emits grid coords on every labelled corner");
        let (i, j) = (g.i, g.j);
        assert!(
            seen.insert((i, j)),
            "{context}: duplicate (i, j) = ({i}, {j}) — v2 precision contract violated"
        );
    }
}

fn assert_grid_rebased_to_origin(detection: &chessboard_v2::Detection, context: &str) {
    let mut min_i = i32::MAX;
    let mut min_j = i32::MAX;
    for lc in &detection.target.corners {
        let g = lc.grid.expect("grid coords present");
        min_i = min_i.min(g.i);
        min_j = min_j.min(g.j);
    }
    assert_eq!(
        (min_i, min_j),
        (0, 0),
        "{context}: grid labels must be rebased so bbox min is (0, 0), got ({min_i}, {min_j})"
    );
}

#[test]
fn smoke_first_subframe_detects() {
    let path = target_path(0);
    if !path.exists() {
        eprintln!(
            "skipping smoke test: testdata/3536119669/target_0.png missing ({})",
            path.display()
        );
        return;
    }
    let img = image::open(&path).expect("decode target_0.png").to_luma8();
    let snap = extract_snap(&img, 0);

    let chess_cfg = default_chess_config();
    let corners = detect_corners(&snap, &chess_cfg);

    let detector = Detector::new(DetectorParams::default());
    let detection = detector
        .detect(&corners)
        .expect("target_0 snap 0 must produce a detection");

    assert!(
        detection.target.corners.len() >= 10,
        "expected at least 10 labelled corners on a clean snap, got {}",
        detection.target.corners.len()
    );
    assert_no_duplicate_labels(&detection, "smoke target_0 snap_0");
    assert_grid_rebased_to_origin(&detection, "smoke target_0 snap_0");
}

#[test]
#[ignore = "full 120-snap dataset sweep; run with --ignored"]
fn full_dataset_precision_contract() {
    let dir = dataset_dir();
    if !dir.exists() {
        panic!(
            "dataset dir missing: {} — run from repo root",
            dir.display()
        );
    }

    let chess_cfg = default_chess_config();
    let detector = Detector::new(DetectorParams::default());
    let mut n_frames = 0usize;
    let mut n_detected = 0usize;

    for target_idx in 0..NUM_TARGETS {
        let path = target_path(target_idx);
        if !path.exists() {
            panic!("missing target_{target_idx}.png at {}", path.display());
        }
        let img = image::open(&path)
            .unwrap_or_else(|e| panic!("decode target_{target_idx}: {e}"))
            .to_luma8();
        for snap_idx in 0..SNAPS_PER_IMAGE {
            let snap = extract_snap(&img, snap_idx);
            let corners = detect_corners(&snap, &chess_cfg);
            n_frames += 1;
            let Some(detection) = detector.detect(&corners) else {
                continue;
            };
            n_detected += 1;
            let ctx = format!("t{target_idx}s{snap_idx}");
            assert_no_duplicate_labels(&detection, &ctx);
            assert_grid_rebased_to_origin(&detection, &ctx);
        }
    }

    assert_eq!(
        n_frames,
        (NUM_TARGETS * SNAPS_PER_IMAGE) as usize,
        "dataset layout changed: expected {} frames",
        NUM_TARGETS * SNAPS_PER_IMAGE
    );
    assert!(
        n_detected >= MIN_DETECTIONS,
        "precision contract regression: detected {n_detected}/{n_frames} (expected ≥ {MIN_DETECTIONS})"
    );
}
