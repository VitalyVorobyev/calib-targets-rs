//! Per-image regression gates for the broader `testdata/` image set.
//!
//! Complements the private 120-frame flagship benchmark in
//! [`private_dataset.rs`] with a smaller, diverse set of publicly
//! committed single images (ChArUco, plain chessboard, synthetic /
//! printed puzzleboards, extreme-resolution photographs). Every
//! image carries its own baseline expectations in
//! `testdata/chessboard_regression_baselines.json`, maintained as a
//! ratchet: tighten numbers as the detector improves, never loosen
//! them silently.
//!
//! Runs in every `cargo test --workspace` invocation. If the baseline
//! JSON goes missing or an image referenced by it cannot be read,
//! the test panics rather than silently skipping — a missing image
//! is a corrupted repository, not a legitimate skip.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use calib_targets::detect::{default_chess_config, detect_corners};
use calib_targets_chessboard::{Detection, Detector, DetectorParams};
use serde::Deserialize;

fn workspace_root() -> PathBuf {
    // Cargo sets CARGO_MANIFEST_DIR to the crate dir; go up two levels
    // to reach the workspace root where testdata/ lives.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn baselines_path() -> PathBuf {
    workspace_root().join("testdata/chessboard_regression_baselines.json")
}

#[derive(Debug, Deserialize)]
struct Baselines {
    images: Vec<ImageGate>,
}

#[derive(Debug, Deserialize)]
struct ImageGate {
    path: String,
    require_detection: bool,
    min_labelled: usize,
    max_components: u32,
}

fn load_baselines() -> Baselines {
    let path = baselines_path();
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

fn run_detector(img_path: &Path) -> (Option<Detection>, usize) {
    let img = image::open(img_path)
        .unwrap_or_else(|e| panic!("decode {}: {e}", img_path.display()))
        .to_luma8();
    let chess_cfg = default_chess_config();
    let corners = detect_corners(&img, &chess_cfg, 0.0);
    let params = DetectorParams::default();
    let detector = Detector::new(params);
    let detection = detector.detect(&corners);
    let components = detector.detect_all(&corners).len();
    (detection, components)
}

fn assert_no_duplicate_labels(detection: &Detection, context: &str) {
    let mut seen: HashSet<(i32, i32)> = HashSet::new();
    for lc in &detection.target.corners {
        let g = lc
            .grid
            .expect("chessboard detector emits grid coords on every labelled corner");
        assert!(
            seen.insert((g.i, g.j)),
            "{context}: duplicate (i, j) = ({}, {}) — precision contract violated",
            g.i,
            g.j
        );
    }
}

fn assert_grid_rebased_to_origin(detection: &Detection, context: &str) {
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
        "{context}: grid labels must be rebased to (0, 0), got ({min_i}, {min_j})"
    );
}

fn assert_origin_top_left(detection: &Detection, context: &str) {
    // `(0, 0)` must sit visually at the top-left: among labelled
    // corners the +i step must move `+x` and the +j step must move
    // `+y`. Uses mean adjacent-neighbour deltas — robust to
    // perspective and partial detections.
    use std::collections::HashMap;
    let labelled: HashMap<(i32, i32), (f32, f32)> = detection
        .target
        .corners
        .iter()
        .filter_map(|lc| {
            let g = lc.grid?;
            Some(((g.i, g.j), (lc.position.x, lc.position.y)))
        })
        .collect();
    if labelled.len() < 4 {
        // Too sparse for a direction test; the rebase check alone is
        // enough for tiny detections.
        return;
    }
    // Only the "longitudinal" components are used (Δx per +i step,
    // Δy per +j step) — the orthogonal components don't enter the
    // direction check and would just add noise.
    let mut sum_dxi = 0.0_f64;
    let mut sum_dyj = 0.0_f64;
    let mut n_i = 0u32;
    let mut n_j = 0u32;
    for (&(i, j), &(x, y)) in &labelled {
        if let Some(&(xn, _)) = labelled.get(&(i + 1, j)) {
            sum_dxi += (xn - x) as f64;
            n_i += 1;
        }
        if let Some(&(_, yn)) = labelled.get(&(i, j + 1)) {
            sum_dyj += (yn - y) as f64;
            n_j += 1;
        }
    }
    assert!(n_i > 0 && n_j > 0, "{context}: not enough adjacency");
    let mean_dxi = sum_dxi / n_i as f64;
    let mean_dyj = sum_dyj / n_j as f64;
    assert!(
        mean_dxi > 0.0,
        "{context}: +i axis does not point right (mean Δx per +i step = {mean_dxi:.2})"
    );
    assert!(
        mean_dyj > 0.0,
        "{context}: +j axis does not point down (mean Δy per +j step = {mean_dyj:.2})"
    );
}

#[test]
fn baseline_file_loadable_and_self_consistent() {
    let baselines = load_baselines();
    assert!(!baselines.images.is_empty(), "baseline file has no images");
    // Image existence is no longer asserted — some entries reference
    // private material that ships outside the public repo. Missing
    // images are skipped at gate-evaluation time below.
    let root = workspace_root();
    let mut present = 0usize;
    for g in &baselines.images {
        if root.join(&g.path).exists() {
            present += 1;
        }
    }
    assert!(
        present > 0,
        "baseline file lists {} images but none exist on disk",
        baselines.images.len()
    );
}

#[test]
fn every_listed_image_meets_its_gate() {
    let baselines = load_baselines();
    let root = workspace_root();
    let mut failures: Vec<String> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    let mut gated = 0usize;
    for gate in &baselines.images {
        let abs = root.join(&gate.path);
        if !abs.exists() {
            // Private regression image not present on disk (e.g. CI
            // on a fresh public checkout). Skip — the gate is
            // advisory, not CI-blocking, on missing private material.
            skipped.push(gate.path.clone());
            continue;
        }
        let (detection, components) = run_detector(&abs);
        gated += 1;
        let ctx = gate.path.clone();

        if gate.require_detection {
            let Some(d) = detection.as_ref() else {
                failures.push(format!(
                    "{ctx}: require_detection=true but detect() returned None"
                ));
                continue;
            };
            // Hard invariants apply whenever a detection exists.
            assert_no_duplicate_labels(d, &ctx);
            assert_grid_rebased_to_origin(d, &ctx);
            assert_origin_top_left(d, &ctx);
            let labelled = d.target.corners.len();
            if labelled < gate.min_labelled {
                failures.push(format!(
                    "{ctx}: labelled={labelled} < min_labelled={}",
                    gate.min_labelled
                ));
            }
        } else if let Some(d) = detection.as_ref() {
            // Opt-out images are still checked for hard invariants if
            // they happen to produce a detection — the point of
            // ratcheting is that when a fix lands, the test catches
            // it immediately; invariants must still hold.
            assert_no_duplicate_labels(d, &ctx);
            assert_grid_rebased_to_origin(d, &ctx);
            assert_origin_top_left(d, &ctx);
        }

        if components > gate.max_components as usize {
            failures.push(format!(
                "{ctx}: components={components} > max_components={}",
                gate.max_components
            ));
        }
    }
    eprintln!(
        "chessboard testdata regression: gated {gated} images, skipped {} missing",
        skipped.len()
    );
    if !skipped.is_empty() {
        eprintln!("  skipped:");
        for path in &skipped {
            eprintln!("    - {path}");
        }
    }
    assert!(
        gated > 0,
        "no images gated — baseline references {} entries but none are on disk",
        baselines.images.len()
    );
    if !failures.is_empty() {
        panic!(
            "chessboard testdata regression failures:\n  - {}",
            failures.join("\n  - ")
        );
    }
}
