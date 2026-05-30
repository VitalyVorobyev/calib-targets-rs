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
use calib_targets_chessboard::{
    ChessboardDetection, Detector, DetectorParams, GraphBuildAlgorithm,
};
use image::imageops::FilterType;
use image::{GenericImageView, GrayImage};

const SNAP_WIDTH: u32 = 720;
const SNAP_HEIGHT: u32 = 540;
const SNAPS_PER_IMAGE: u32 = 6;
const NUM_TARGETS: u32 = 20;
const UPSCALE: u32 = 2;
const MIN_FULL_SWEEP_DETECTIONS: usize = 119;

/// Topological-pipeline any-detection floor on the 120-snap sweep — the
/// same metric the seed-and-grow contract enforces. A "detection" is
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

/// Dataset-wide ceiling on independent wrong-label-edge audit hits across
/// the topological sweep. The topological wrong-label check
/// (`geometry_check::topological_wrong_label_drops`) brought the
/// grossly-overlong cardinal-edge count from 78 (gate disabled) down to a
/// single sparse-frontier edge that sits below the check's local-sample
/// floor. This audit is deliberately *independent* of the detector's own
/// predicate (it uses the per-frame global median edge length, not the
/// local same-direction reference the check uses), so it is a real
/// regression tripwire. Bumping it higher is a precision regression —
/// fix the check, not the gate.
const MAX_TOPOLOGICAL_OVERLONG_EDGES: usize = 2;

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

fn default_seed_and_grow_detector() -> Detector {
    let mut params = DetectorParams::default();
    params.graph_build_algorithm = GraphBuildAlgorithm::SeedAndGrow;
    Detector::new(params)
}

fn default_topological_detector() -> Detector {
    let mut params = DetectorParams::default();
    params.graph_build_algorithm = GraphBuildAlgorithm::Topological;
    Detector::new(params)
}

fn assert_detection_invariants(detection: &ChessboardDetection, context: &str) {
    let mut seen = HashSet::<(i32, i32)>::new();
    let mut min_i = i32::MAX;
    let mut min_j = i32::MAX;
    for corner in &detection.corners {
        assert!(
            corner.position.x.is_finite() && corner.position.y.is_finite(),
            "{context}: non-finite labelled corner position"
        );
        let grid = corner.grid;
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

/// Independent wrong-label-edge audit — deliberately NOT reusing the
/// detector's own predicate. Returns `(overlong, collapsed)`:
/// - `overlong`: cardinal edges longer than 1.6× the per-frame **global**
///   median cardinal-edge length (a skipped-corner / diagonal boundary).
/// - `collapsed`: pairs of distinct `(i, j)` labels closer than 0.2× that
///   median in pixels (the duplicate-pixel fold the topological grid can
///   produce in defocused bands).
///
/// Both are wrong-label signatures the topological geometry gate must
/// remove. Using the global median (rather than the local same-direction
/// reference the check itself uses) keeps this an independent verifier.
fn audit_wrong_label_edges(detection: &ChessboardDetection) -> (usize, usize) {
    let by_grid: std::collections::HashMap<(i32, i32), (f32, f32)> = detection
        .corners
        .iter()
        .map(|c| ((c.grid.i, c.grid.j), (c.position.x, c.position.y)))
        .collect();
    let mut lens: Vec<f32> = Vec::new();
    for (&(i, j), &(x, y)) in &by_grid {
        for (di, dj) in [(1, 0), (0, 1)] {
            if let Some(&(nx, ny)) = by_grid.get(&(i + di, j + dj)) {
                lens.push(((nx - x).powi(2) + (ny - y).powi(2)).sqrt());
            }
        }
    }
    if lens.is_empty() {
        return (0, 0);
    }
    lens.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let l_med = lens[lens.len() / 2];
    let overlong = lens.iter().filter(|&&l| l > 1.6 * l_med).count();

    let pts: Vec<(f32, f32)> = by_grid.values().copied().collect();
    let eps2 = (0.2 * l_med).powi(2);
    let mut collapsed = 0usize;
    for (a, &(ax, ay)) in pts.iter().enumerate() {
        for &(bx, by) in &pts[a + 1..] {
            if (ax - bx).powi(2) + (ay - by).powi(2) < eps2 {
                collapsed += 1;
            }
        }
    }
    (overlong, collapsed)
}

#[test]
fn puzzle130_smoke_target15_snap0_keeps_large_grid() {
    if !dataset_present_or_skip("puzzle130_smoke_target15_snap0_keeps_large_grid") {
        return;
    }

    let snap = load_snap(15, 0);
    let corners = detect_corners(&snap, &default_chess_config());
    let detector = default_seed_and_grow_detector();
    let detection = detector
        .detect(&corners)
        .expect("target_15 snap 0 must produce a chessboard detection");
    assert!(
        detection.corners.len() >= 500,
        "target_15 snap 0 labelled {} corners, expected at least 500",
        detection.corners.len()
    );
    assert_detection_invariants(&detection, "target_15 snap 0");
}

/// Fast topological-path gate. The default `cargo test` smoke above
/// exercises `SeedAndGrow`; this one exercises the topological builder —
/// the puzzle default — on a frame (`target_13` snap 0) that, with the
/// wrong-label check disabled, carries a duplicate-pixel label fold and
/// interior skipped-corner edges. The check must remove them while
/// keeping the dense grid, so this locks the structural-check contract
/// into the default test pass (not just the `#[ignore]` sweep).
#[test]
fn puzzle130_topological_smoke_target13_snap0_rejects_wrong_labels() {
    if !dataset_present_or_skip("puzzle130_topological_smoke_target13_snap0_rejects_wrong_labels") {
        return;
    }

    let snap = load_snap(13, 0);
    let corners = detect_corners(&snap, &default_chess_config());
    let detection = default_topological_detector()
        .detect(&corners)
        .expect("target_13 snap 0 must produce a topological detection");
    assert!(
        detection.corners.len() >= MIN_TOPOLOGICAL_LABELLED_PER_SNAP,
        "target_13 snap 0 labelled {} corners, expected >= {MIN_TOPOLOGICAL_LABELLED_PER_SNAP}",
        detection.corners.len()
    );
    assert_detection_invariants(&detection, "target_13 snap 0 topological");
    let (overlong, collapsed) = audit_wrong_label_edges(&detection);
    assert_eq!(
        collapsed, 0,
        "target_13 snap 0: {collapsed} duplicate-pixel label pair(s) survived the topological wrong-label check"
    );
    assert_eq!(
        overlong, 0,
        "target_13 snap 0: {overlong} overlong wrong-label edge(s) survived the topological wrong-label check"
    );
}

#[test]
#[ignore = "private 120-snap 130x130_puzzle sweep; run with --ignored"]
fn puzzle130_full_seed_and_grow_recall_contract() {
    if !dataset_present_or_skip("puzzle130_full_seed_and_grow_recall_contract") {
        return;
    }

    let chess_cfg = default_chess_config();
    let detector = default_seed_and_grow_detector();
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
            let corners = detect_corners(&snap, &chess_cfg);
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
    eprintln!("130x130_puzzle seed-and-grow detected {detected}/{frames} snaps");
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
    let mut overlong_total = 0usize;
    let mut collapsed_total = 0usize;

    for target_idx in 0..NUM_TARGETS {
        let path = target_path(target_idx);
        assert!(
            path.exists(),
            "missing target_{target_idx}.png at {}",
            path.display()
        );
        for snap_idx in 0..SNAPS_PER_IMAGE {
            let snap = load_snap(target_idx, snap_idx);
            let corners = detect_corners(&snap, &chess_cfg);
            frames += 1;
            let Some(detection) = detector.detect(&corners) else {
                eprintln!("target_{target_idx} snap {snap_idx}: no detection");
                continue;
            };
            any_detection += 1;
            let labelled = detection.corners.len();
            total_labelled += labelled;
            let context = format!("target_{target_idx} snap {snap_idx}");
            assert_detection_invariants(&detection, &context);
            let (overlong, collapsed) = audit_wrong_label_edges(&detection);
            overlong_total += overlong;
            collapsed_total += collapsed;
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
    eprintln!(
        "130x130_puzzle topological wrong-label audit: overlong edges = {overlong_total}, duplicate-pixel pairs = {collapsed_total}"
    );
    assert_eq!(
        collapsed_total, 0,
        "130x130_puzzle topological produced {collapsed_total} duplicate-pixel label pair(s) (expected 0)"
    );
    assert!(
        overlong_total <= MAX_TOPOLOGICAL_OVERLONG_EDGES,
        "130x130_puzzle topological wrong-label-edge regression: {overlong_total} overlong edges > {MAX_TOPOLOGICAL_OVERLONG_EDGES}"
    );
}
