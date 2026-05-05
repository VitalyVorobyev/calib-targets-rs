//! Chessboard-only regression gates for the private `130x130_puzzle` dataset.
//!
//! The PuzzleBoard decoder depends on the chessboard grid builder, but this
//! test intentionally stops at chessboard detection. The historical main-branch
//! contract on this dataset is 119-120 detections across 20 stitched target
//! images × 6 snaps, after the same 2× upscaling used by the bench harness.
//! A regression here is blocking even when the experimental topological grid
//! builder is still opt-in.
//!
//! The dataset is private and lives under `privatedata/130x130_puzzle` in a
//! local checkout. Fresh public clones skip these tests. Override the location
//! with `CALIB_PUZZLE_PRIVATE_DATASET=/path/to/130x130_puzzle`.

use std::collections::HashSet;
use std::path::PathBuf;

use calib_targets::detect::{default_chess_config, detect_corners};
use calib_targets_chessboard::{Detection, Detector, DetectorParams, GraphBuildAlgorithm};
use image::imageops::FilterType;
use image::{GenericImageView, GrayImage};

const SNAP_WIDTH: u32 = 720;
const SNAP_HEIGHT: u32 = 540;
const SNAPS_PER_IMAGE: u32 = 6;
const NUM_TARGETS: u32 = 20;
const UPSCALE: u32 = 2;
const MIN_FULL_SWEEP_DETECTIONS: usize = 119;

/// Topological-pipeline any-detection floor on the 120-snap sweep — the
/// same metric the chessboard-v2 contract enforces. A "detection" is
/// any frame where `Detector::detect()` returns `Some(_)` whose grid
/// labels satisfy the workspace invariants (no duplicates, finite
/// positions, origin rebased). Ratchets up phase by phase per
/// `.claude/plans/we-changed-topological-grid-eager-valiant.md`.
///
/// History:
/// - Pre-Phase-A baseline (22°/18°, no cluster gate): 119/120.
/// - Phase A (cluster gate at 16°): 117/120 (slight regression as the
///   gate filters borderline corners; meant as precision insurance for
///   subsequent tightening).
/// - Phase B (geometry check on topological output): 117/120 with
///   ~4 fewer labelled per detection on average (drops residual
///   outliers — pure precision).
/// - Phase C (15°/15° tolerances paired with cluster gate): 119/120,
///   mean labelled per detection up from ~405 to ~590. The narrower
///   angle window shed spurious-quad pollution that was fragmenting
///   components, which is what the cluster gate was designed to make
///   safe.
/// - Phase D2 (per-component cell-size upper bound at 1.8×): 120/120,
///   mean labelled steady. Catches the double-cell hop case that
///   produced oversized quads at component edges.
/// - Chessboard-v2 on the same images: 119–120/120 (unchanged).
///
/// Bumping this constant lower is a regression — fix the algorithm,
/// not the gate.
const MIN_FULL_SWEEP_TOPOLOGICAL_DETECTIONS: usize = 119;

/// Soft "meaningful detection" threshold reported alongside the gate.
/// A 130×130 board projected at the snap viewing angle should yield
/// several hundred labelled corners; tiny detections (≪ 250) are
/// useful primarily as fragments. Reported on stderr for visibility
/// but **not** asserted on — the any-detection floor above is the
/// hard contract.
const MIN_TOPOLOGICAL_LABELLED_PER_SNAP: usize = 250;

fn dataset_dir() -> PathBuf {
    if let Ok(custom) = std::env::var("CALIB_PUZZLE_PRIVATE_DATASET") {
        return PathBuf::from(custom);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../privatedata/130x130_puzzle")
}

fn target_path(idx: u32) -> PathBuf {
    dataset_dir().join(format!("target_{idx}.png"))
}

fn dataset_present_or_skip(test_name: &str) -> bool {
    if dataset_dir().exists() {
        return true;
    }
    eprintln!(
        "[skipped] {test_name}: 130x130_puzzle dataset missing at {}",
        dataset_dir().display()
    );
    false
}

fn load_snap(target_idx: u32, snap_idx: u32) -> GrayImage {
    let path = target_path(target_idx);
    let img = image::open(&path)
        .unwrap_or_else(|e| panic!("decode {}: {e}", path.display()))
        .to_luma8();
    let x0 = snap_idx * SNAP_WIDTH;
    let snap = img.view(x0, 0, SNAP_WIDTH, SNAP_HEIGHT).to_image();
    image::imageops::resize(
        &snap,
        SNAP_WIDTH * UPSCALE,
        SNAP_HEIGHT * UPSCALE,
        FilterType::Triangle,
    )
}

fn default_chessboard_v2_detector() -> Detector {
    let mut params = DetectorParams::default();
    params.graph_build_algorithm = GraphBuildAlgorithm::ChessboardV2;
    Detector::new(params)
}

fn default_topological_detector() -> Detector {
    let mut params = DetectorParams::default();
    params.graph_build_algorithm = GraphBuildAlgorithm::Topological;
    Detector::new(params)
}

fn assert_detection_invariants(detection: &Detection, context: &str) {
    let mut seen = HashSet::<(i32, i32)>::new();
    let mut min_i = i32::MAX;
    let mut min_j = i32::MAX;
    for corner in &detection.target.corners {
        assert!(
            corner.position.x.is_finite() && corner.position.y.is_finite(),
            "{context}: non-finite labelled corner position"
        );
        let grid = corner
            .grid
            .expect("chessboard detections carry grid coordinates");
        assert!(
            seen.insert((grid.i, grid.j)),
            "{context}: duplicate grid label ({}, {})",
            grid.i,
            grid.j
        );
        min_i = min_i.min(grid.i);
        min_j = min_j.min(grid.j);
    }
    assert_eq!(
        (min_i, min_j),
        (0, 0),
        "{context}: grid labels must be rebased to origin"
    );
}

#[test]
fn puzzle130_smoke_target15_snap0_keeps_large_grid() {
    if !dataset_present_or_skip("puzzle130_smoke_target15_snap0_keeps_large_grid") {
        return;
    }

    let snap = load_snap(15, 0);
    let corners = detect_corners(&snap, &default_chess_config(), 0.0);
    let detector = default_chessboard_v2_detector();
    let detection = detector
        .detect(&corners)
        .expect("target_15 snap 0 must produce a chessboard detection");
    assert!(
        detection.target.corners.len() >= 500,
        "target_15 snap 0 labelled {} corners, expected at least 500",
        detection.target.corners.len()
    );
    assert_detection_invariants(&detection, "target_15 snap 0");
}

#[test]
#[ignore = "private 120-snap 130x130_puzzle sweep; run with --ignored"]
fn puzzle130_full_chessboard_v2_recall_contract() {
    if !dataset_present_or_skip("puzzle130_full_chessboard_v2_recall_contract") {
        return;
    }

    let chess_cfg = default_chess_config();
    let detector = default_chessboard_v2_detector();
    let mut frames = 0usize;
    let mut detected = 0usize;

    for target_idx in 0..NUM_TARGETS {
        let path = target_path(target_idx);
        assert!(
            path.exists(),
            "missing target_{target_idx}.png at {}",
            path.display()
        );
        for snap_idx in 0..SNAPS_PER_IMAGE {
            let snap = load_snap(target_idx, snap_idx);
            let corners = detect_corners(&snap, &chess_cfg, 0.0);
            frames += 1;
            let Some(detection) = detector.detect(&corners) else {
                continue;
            };
            detected += 1;
            let context = format!("target_{target_idx} snap {snap_idx}");
            assert_detection_invariants(&detection, &context);
        }
    }

    assert_eq!(
        frames,
        (NUM_TARGETS * SNAPS_PER_IMAGE) as usize,
        "dataset layout changed"
    );
    eprintln!("130x130_puzzle chessboard-v2 detected {detected}/{frames} snaps");
    assert!(
        detected >= MIN_FULL_SWEEP_DETECTIONS,
        "130x130_puzzle chessboard recall regression: detected {detected}/{frames}, expected >= {MIN_FULL_SWEEP_DETECTIONS}"
    );
}

#[test]
#[ignore = "private 120-snap 130x130_puzzle topological sweep; run with --ignored"]
fn puzzle130_full_topological_recall_contract() {
    if !dataset_present_or_skip("puzzle130_full_topological_recall_contract") {
        return;
    }

    let chess_cfg = default_chess_config();
    let detector = default_topological_detector();
    let mut frames = 0usize;
    let mut any_detection = 0usize;
    let mut meaningful = 0usize;
    let mut total_labelled = 0usize;

    for target_idx in 0..NUM_TARGETS {
        let path = target_path(target_idx);
        assert!(
            path.exists(),
            "missing target_{target_idx}.png at {}",
            path.display()
        );
        for snap_idx in 0..SNAPS_PER_IMAGE {
            let snap = load_snap(target_idx, snap_idx);
            let corners = detect_corners(&snap, &chess_cfg, 0.0);
            frames += 1;
            let Some(detection) = detector.detect(&corners) else {
                eprintln!("target_{target_idx} snap {snap_idx}: no detection");
                continue;
            };
            any_detection += 1;
            let labelled = detection.target.corners.len();
            total_labelled += labelled;
            let context = format!("target_{target_idx} snap {snap_idx}");
            assert_detection_invariants(&detection, &context);
            let meaningful_marker = if labelled >= MIN_TOPOLOGICAL_LABELLED_PER_SNAP {
                meaningful += 1;
                "OK"
            } else {
                "small"
            };
            eprintln!(
                "target_{target_idx} snap {snap_idx}: labelled={labelled} ({meaningful_marker})"
            );
        }
    }

    assert_eq!(
        frames,
        (NUM_TARGETS * SNAPS_PER_IMAGE) as usize,
        "dataset layout changed"
    );
    eprintln!(
        "130x130_puzzle topological: any-detection {any_detection}/{frames}, meaningful (>= {MIN_TOPOLOGICAL_LABELLED_PER_SNAP} labelled) {meaningful}/{frames}, mean labelled per detection = {:.1}",
        if any_detection > 0 { total_labelled as f32 / any_detection as f32 } else { 0.0 }
    );
    assert!(
        any_detection >= MIN_FULL_SWEEP_TOPOLOGICAL_DETECTIONS,
        "130x130_puzzle topological recall regression: any-detection {any_detection}/{frames}, expected >= {MIN_FULL_SWEEP_TOPOLOGICAL_DETECTIONS}"
    );
}
