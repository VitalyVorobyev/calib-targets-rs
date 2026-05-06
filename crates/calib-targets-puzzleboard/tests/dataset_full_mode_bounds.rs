//! Dataset-gated regression for PuzzleBoard `Full` vs `FixedBoard` decode
//! contracts on a known printed-board dataset.
//!
//! A handoff from a sibling calibration repo (kept locally under the
//! gitignored `docs/datasets/` tree) reported that `Full` mode could pick a
//! wrong master origin under partial views, putting decoded
//! `target_position` values far outside the declared board. Re-measurement
//! against this branch shows `Full` and `FixedBoard` agree on every
//! decoded snap of the local `130x130_puzzle` set; this file freezes that
//! contract so any future detector change that splits the two modes apart
//! is caught immediately.
//!
//! Three tests share one fixture set:
//!
//! - [`fixed_board_keeps_target_positions_in_board`] — `FixedBoard +
//!   SoftLogLikelihood` keeps every decoded `target_position` inside the
//!   declared board's bounds and stays at low BER.
//! - [`full_mode_origin_matches_fixed_board`] — `Full` and `FixedBoard`
//!   pick the same `(D4, master_origin_row, master_origin_col)` on the
//!   pinned fixtures.
//! - [`full_vs_fixed_board_origin_match_on_all_snaps`] (`#[ignore]`) —
//!   on-demand sweep over every `target_*.png` × 6 snaps; reports any
//!   disagreement. Run with `cargo test ... -- --ignored`.
//!
//! All tests skip silently when the dataset directory is missing — this
//! matches the bench at `benches/dataset_decode.rs` so fresh clones and CI
//! without the private dataset stay green. Override the dataset path with
//! `CALIB_PUZZLE_PRIVATE_DATASET=/path/to/130x130_puzzle`.

use std::path::PathBuf;

use calib_targets::detect::{default_chess_config, detect_corners, gray_view};
use calib_targets_puzzleboard::{
    PuzzleBoardDetectionResult, PuzzleBoardDetector, PuzzleBoardParams, PuzzleBoardScoringMode,
    PuzzleBoardSearchMode, PuzzleBoardSpec,
};
use image::imageops::FilterType;
use image::{GenericImageView, GrayImage};

const SNAP_WIDTH: u32 = 720;
const SNAP_HEIGHT: u32 = 540;

const BOARD_ROWS: u32 = 130;
const BOARD_COLS: u32 = 130;
const BOARD_CELL_MM: f32 = 1.014;
const UPSCALE: u32 = 2;
const MAX_BIT_ERROR_RATE: f32 = 0.05;

/// Same fixture set as `benches/dataset_decode.rs` so the bounds regression
/// stays in lockstep with the bench-pinned snaps.
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
    let snap = img.view(x0, 0, SNAP_WIDTH, SNAP_HEIGHT).to_image();
    if UPSCALE == 1 {
        return Some(snap);
    }
    Some(image::imageops::resize(
        &snap,
        SNAP_WIDTH * UPSCALE,
        SNAP_HEIGHT * UPSCALE,
        FilterType::Triangle,
    ))
}

fn detect_one(
    image: &GrayImage,
    search_mode: PuzzleBoardSearchMode,
    scoring_mode: PuzzleBoardScoringMode,
) -> Option<PuzzleBoardDetectionResult> {
    let spec =
        PuzzleBoardSpec::with_origin(BOARD_ROWS, BOARD_COLS, BOARD_CELL_MM, 0, 0).expect("spec");
    let mut params = PuzzleBoardParams::for_board(&spec);
    params.decode.search_mode = search_mode;
    params.decode.scoring_mode = scoring_mode;
    let detector = PuzzleBoardDetector::new(params).expect("detector");

    let chess_cfg = default_chess_config();
    let corners = detect_corners(image, &chess_cfg, 0.0);
    detector.detect(&gray_view(image), &corners).ok()
}

fn dataset_present_or_skip(test_name: &str) -> bool {
    if dataset_dir().exists() {
        return true;
    }
    eprintln!(
        "[skipped] {test_name}: dataset {:?} missing — set \
         CALIB_PUZZLE_PRIVATE_DATASET to enable",
        dataset_dir()
    );
    false
}

#[test]
fn fixed_board_keeps_target_positions_in_board() {
    if !dataset_present_or_skip("fixed_board_keeps_target_positions_in_board") {
        return;
    }

    let max_x_mm = (BOARD_COLS as f32 - 1.0) * BOARD_CELL_MM;
    let max_y_mm = (BOARD_ROWS as f32 - 1.0) * BOARD_CELL_MM;

    let mut successes = 0usize;
    for &(target_idx, snap_idx, label) in FIXTURES {
        let Some(image) = load_snap(target_idx, snap_idx) else {
            panic!("fixture {label} (target {target_idx} snap {snap_idx}) missing on disk");
        };
        let Some(result) = detect_one(
            &image,
            PuzzleBoardSearchMode::FixedBoard,
            PuzzleBoardScoringMode::SoftLogLikelihood,
        ) else {
            eprintln!("{label}: FixedBoard+Soft decode failed — skipping bounds check");
            continue;
        };
        successes += 1;

        assert!(
            result.decode.bit_error_rate <= MAX_BIT_ERROR_RATE,
            "{label}: BER {:.3} > {MAX_BIT_ERROR_RATE} — decode below confidence floor",
            result.decode.bit_error_rate,
        );

        for lc in &result.detection.corners {
            let Some(tp) = lc.target_position else {
                continue;
            };
            assert!(
                (0.0..=max_x_mm).contains(&tp.x),
                "{label}: target_position.x = {:.3} mm out of [0, {max_x_mm:.3}] \
                 (origin r={}, c={}, transform={:?})",
                tp.x,
                result.decode.master_origin_row,
                result.decode.master_origin_col,
                result.alignment.transform,
            );
            assert!(
                (0.0..=max_y_mm).contains(&tp.y),
                "{label}: target_position.y = {:.3} mm out of [0, {max_y_mm:.3}] \
                 (origin r={}, c={}, transform={:?})",
                tp.y,
                result.decode.master_origin_row,
                result.decode.master_origin_col,
                result.alignment.transform,
            );
        }
    }

    assert!(
        successes >= 3,
        "FixedBoard+Soft decoded only {successes}/{} fixtures — recall regression",
        FIXTURES.len(),
    );
}

#[test]
fn full_mode_origin_matches_fixed_board() {
    if !dataset_present_or_skip("full_mode_origin_matches_fixed_board") {
        return;
    }

    let mut compared = 0usize;
    let mut disagreements = Vec::<String>::new();
    for &(target_idx, snap_idx, label) in FIXTURES {
        let Some(image) = load_snap(target_idx, snap_idx) else {
            panic!("fixture {label} (target {target_idx} snap {snap_idx}) missing on disk");
        };
        let full = detect_one(
            &image,
            PuzzleBoardSearchMode::Full,
            PuzzleBoardScoringMode::SoftLogLikelihood,
        );
        let fixed = detect_one(
            &image,
            PuzzleBoardSearchMode::FixedBoard,
            PuzzleBoardScoringMode::SoftLogLikelihood,
        );
        match (full, fixed) {
            (Some(f), Some(x)) => {
                compared += 1;
                if f.alignment.transform != x.alignment.transform
                    || f.decode.master_origin_row != x.decode.master_origin_row
                    || f.decode.master_origin_col != x.decode.master_origin_col
                {
                    disagreements.push(format!(
                        "{label}: full=({},{},{:?}) fixed=({},{},{:?})",
                        f.decode.master_origin_row,
                        f.decode.master_origin_col,
                        f.alignment.transform,
                        x.decode.master_origin_row,
                        x.decode.master_origin_col,
                        x.alignment.transform,
                    ));
                }
            }
            _ => {
                eprintln!("{label}: skipping (one mode failed to decode)");
            }
        }
    }

    assert!(
        compared >= 1,
        "no fixture decoded under both modes — cannot evaluate origin match",
    );
    assert!(
        disagreements.is_empty(),
        "Full vs FixedBoard origin mismatch on {}/{compared} fixtures:\n  {}",
        disagreements.len(),
        disagreements.join("\n  "),
    );
}

const SNAPS_PER_IMAGE: u32 = 6;
const MAX_TARGET_IDX: u32 = 32;

/// On-demand sweep over every available `target_*.png` × 6 snaps, comparing
/// `Full` and `FixedBoard` master origins under both scoring modes.
///
/// Marked `#[ignore]` so it stays out of the default test pass — the
/// pinned-fixture variant already locks the contract; this one is the
/// broader spot-check used when investigating a real-world wrong-origin
/// report. Run with:
/// ```text
/// cargo test -p calib-targets-puzzleboard --test dataset_full_mode_bounds \
///     -- --ignored full_vs_fixed_board_origin_match_on_all_snaps --nocapture
/// ```
#[test]
#[ignore = "broad dataset sweep (~minute); run on demand with --ignored"]
fn full_vs_fixed_board_origin_match_on_all_snaps() {
    if !dataset_present_or_skip("full_vs_fixed_board_origin_match_on_all_snaps") {
        return;
    }

    let modes = [
        ("hard", PuzzleBoardScoringMode::HardWeighted),
        ("soft", PuzzleBoardScoringMode::SoftLogLikelihood),
    ];

    for (mode_name, scoring) in modes {
        let mut compared = 0usize;
        let mut full_only = 0usize;
        let mut fixed_only = 0usize;
        let mut both_fail = 0usize;
        let mut disagreements = Vec::<String>::new();
        for target_idx in 0..MAX_TARGET_IDX {
            for snap_idx in 0..SNAPS_PER_IMAGE {
                let Some(image) = load_snap(target_idx, snap_idx) else {
                    break; // out of frames for this target_idx
                };
                let label = format!("t{target_idx}s{snap_idx}");
                let full = detect_one(&image, PuzzleBoardSearchMode::Full, scoring);
                let fixed = detect_one(&image, PuzzleBoardSearchMode::FixedBoard, scoring);
                match (full, fixed) {
                    (Some(f), Some(x)) => {
                        compared += 1;
                        if f.alignment.transform != x.alignment.transform
                            || f.decode.master_origin_row != x.decode.master_origin_row
                            || f.decode.master_origin_col != x.decode.master_origin_col
                        {
                            disagreements.push(format!(
                                "{label} ({mode_name}): full=({},{},{:?}) fixed=({},{},{:?})",
                                f.decode.master_origin_row,
                                f.decode.master_origin_col,
                                f.alignment.transform,
                                x.decode.master_origin_row,
                                x.decode.master_origin_col,
                                x.alignment.transform,
                            ));
                        }
                    }
                    (Some(_), None) => full_only += 1,
                    (None, Some(_)) => fixed_only += 1,
                    (None, None) => both_fail += 1,
                }
            }
        }
        eprintln!(
            "{mode_name}: compared={compared} full_only={full_only} \
             fixed_only={fixed_only} both_fail={both_fail} \
             disagreements={}",
            disagreements.len(),
        );
        for d in &disagreements {
            eprintln!("  {d}");
        }
        assert!(
            disagreements.is_empty(),
            "Full vs FixedBoard mismatch under {mode_name} scoring \
             on {} of {compared} doubly-decoded snaps",
            disagreements.len(),
        );
    }
}
