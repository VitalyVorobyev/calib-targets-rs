//! Behavioural tests for the unified multi-config sweep semantics of
//! `detect_charuco_best` / `detect_marker_board_best` / `detect_puzzleboard_best`.
//!
//! Each sweep helper honours every config's own `chess` front-end while running
//! corner detection once per *distinct* front-end. These tests pin two
//! contracts:
//!
//! 1. **Mixed front-ends no longer short-circuit.** Before the sweep-unification
//!    revision, `detect_charuco_best` errored and `detect_marker_board_best`
//!    silently returned `None` the moment two sweep configs' `chess` differed —
//!    even when a board was plainly present. Now a sweep succeeds exactly when
//!    at least one of its configs succeeds.
//! 2. **Shared front-ends are byte-identical to the single-config path.** A
//!    sweep whose configs all request the same `chess` runs one corner pass and
//!    returns the same result the single-config helper would.

#![cfg(feature = "image")]

use calib_targets::aruco::builtins;
use calib_targets::charuco::{CharucoBoardSpec, CharucoParams, MarkerLayout};
use calib_targets::detect::{self, default_chess_config, DetectorConfig};
use calib_targets::marker::{
    CellCoords, CirclePolarity, MarkerBoardParams, MarkerBoardSpec, MarkerCircleSpec,
};
use calib_targets::puzzleboard::{PuzzleBoardParams, PuzzleBoardSpec};
use std::path::PathBuf;

fn load_gray(name: &str) -> image::GrayImage {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("testdata")
        .join(name);
    image::ImageReader::open(&path)
        .unwrap_or_else(|e| panic!("open {}: {e}", path.display()))
        .decode()
        .unwrap_or_else(|e| panic!("decode {}: {e}", path.display()))
        .to_luma8()
}

/// Two ChESS front-ends that differ only in threshold. A sweep mixing them
/// produces two *distinct* cache entries (so the per-front-end dedup path is
/// exercised for real), and both are lenient enough to still find a board.
fn mixed_chess() -> (DetectorConfig, DetectorConfig) {
    let a = default_chess_config();
    let b = default_chess_config().with_threshold(10.0);
    assert_ne!(a, b, "the two chess front-ends must actually differ");
    (a, b)
}

#[test]
fn charuco_best_mixed_chess_configs_do_not_short_circuit() {
    let img = load_gray("small2.png");
    let board = CharucoBoardSpec::new(22, 22, 1.0, 0.75, builtins::DICT_4X4_1000)
        .with_marker_layout(MarkerLayout::OpenCvCharuco);
    let (chess_a, chess_b) = mixed_chess();
    let mut cfg_a = CharucoParams::for_board(board);
    cfg_a.chess = chess_a;
    let mut cfg_b = CharucoParams::for_board(board);
    cfg_b.chess = chess_b;

    // Each config's single-config outcome; the sweep must reproduce their OR.
    let single_a = detect::detect_charuco(&img, &cfg_a).is_ok();
    let single_b = detect::detect_charuco(&img, &cfg_b).is_ok();
    let sweep_ok = detect::detect_charuco_best(&img, &[cfg_a, cfg_b]).is_ok();

    // The differing `chess` no longer poisons the whole sweep: `Ok` iff some
    // config produced `Ok` (the old `InconsistentChessConfig` path is gone).
    assert_eq!(
        sweep_ok,
        single_a || single_b,
        "charuco sweep Ok must equal (config A Ok || config B Ok)"
    );
}

#[test]
fn marker_best_mixed_chess_configs_do_not_short_circuit() {
    let img = load_gray("markerboard.png");
    let board = MarkerBoardSpec::new(
        22,
        22,
        [
            MarkerCircleSpec::new(CellCoords { i: 11, j: 11 }, CirclePolarity::White),
            MarkerCircleSpec::new(CellCoords { i: 12, j: 11 }, CirclePolarity::Black),
            MarkerCircleSpec::new(CellCoords { i: 12, j: 12 }, CirclePolarity::White),
        ],
    )
    .with_cell_size(1.0);
    let (chess_a, chess_b) = mixed_chess();
    let mut cfg_a = MarkerBoardParams::for_board(board.clone());
    cfg_a.chess = chess_a;
    let mut cfg_b = MarkerBoardParams::for_board(board);
    cfg_b.chess = chess_b;

    let single_a = detect::detect_marker_board(&img, &cfg_a).is_ok();
    let single_b = detect::detect_marker_board(&img, &cfg_b).is_ok();
    let sweep_ok = detect::detect_marker_board_best(&img, &[cfg_a, cfg_b]).is_ok();

    // No short-circuit to `Err` on `chess` disagreement: `Ok` iff some
    // config found a board.
    assert_eq!(
        sweep_ok,
        single_a || single_b,
        "marker sweep Ok must equal (config A Ok || config B Ok)"
    );
}

#[test]
fn puzzleboard_best_identical_configs_equal_single_config() {
    let img = load_gray("puzzleboard_small.png");
    // testdata/puzzleboard_detect_config.json describes this image: 10x10,
    // 12.0 mm cell, master origin (0, 0).
    let board = PuzzleBoardSpec::with_origin(10, 10, 12.0, 0, 0).expect("board spec");
    let params = PuzzleBoardParams::for_board(board);

    let single = detect::detect_puzzleboard(&img, &params);
    // Two identical configs => one corner pass in the cache; the second config
    // is never strictly better, so the first (== single-config) result wins.
    let sweep = detect::detect_puzzleboard_best(&img, &[params.clone(), params.clone()]);

    match (single, sweep) {
        (Ok(single), Ok(sweep)) => {
            assert_eq!(
                single.corners.len(),
                sweep.corners.len(),
                "labelled corner count must match the single-config path"
            );
            for (s, w) in single.corners.iter().zip(sweep.corners.iter()) {
                assert_eq!(s.position, w.position, "corner position");
                assert_eq!(s.grid, w.grid, "corner grid label");
                assert_eq!(s.id, w.id, "corner id");
                assert_eq!(s.target_position, w.target_position, "target position");
            }
            assert_eq!(
                single.decode.master_origin_row, sweep.decode.master_origin_row,
                "decoded master origin row"
            );
            assert_eq!(
                single.decode.master_origin_col, sweep.decode.master_origin_col,
                "decoded master origin col"
            );
        }
        (Err(_), Err(_)) => {
            // Both paths agree the board is not decodable with these params —
            // still identical outcomes, but the reference image is expected to
            // decode, so flag it rather than pass silently.
            panic!(
                "puzzleboard_small.png failed to decode on both paths — reference image regressed"
            );
        }
        (single, sweep) => panic!(
            "identical-config sweep disagreed with single-config: single_ok={}, sweep_ok={}",
            single.is_ok(),
            sweep.is_ok()
        ),
    }
}
