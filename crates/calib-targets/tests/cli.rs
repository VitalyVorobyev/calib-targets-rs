//! Integration tests for the `calib-targets` CLI binary.

#![cfg(feature = "cli")]

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

fn bin() -> Command {
    Command::cargo_bin("calib-targets").expect("binary")
}

fn assert_bundle_written(stem: &std::path::Path) {
    assert!(stem.with_extension("json").is_file());
    assert!(stem.with_extension("svg").is_file());
    assert!(stem.with_extension("png").is_file());
}

#[test]
fn top_level_help_lists_productized_commands() {
    bin()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "CLI for printable calibration target generation",
        ))
        .stdout(predicate::str::contains("validate"))
        .stdout(predicate::str::contains("list-dictionaries"))
        .stdout(predicate::str::contains("gen"));
}

#[test]
fn list_dictionaries_is_sorted_and_includes_known_name() {
    let output = bin()
        .arg("list-dictionaries")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let stdout = String::from_utf8(output).expect("utf8");
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(!lines.is_empty(), "expected at least one dictionary name");
    assert!(lines.contains(&"DICT_4X4_50"));

    let mut sorted = lines.clone();
    sorted.sort_unstable();
    assert_eq!(lines, sorted, "dictionary names should be printed in order");
}

#[test]
fn init_validate_then_generate_chessboard_bundle() {
    let dir = tempdir().expect("tempdir");
    let spec_path = dir.path().join("board.json");
    let out_stem = dir.path().join("generated/board");

    bin()
        .args([
            "init",
            "chessboard",
            "--out",
            spec_path.to_str().expect("utf8"),
            "--inner-rows",
            "6",
            "--inner-cols",
            "8",
            "--square-size-mm",
            "20",
        ])
        .assert()
        .success();

    bin()
        .args(["validate", "--spec", spec_path.to_str().expect("utf8")])
        .assert()
        .success()
        .stdout(predicate::str::contains("valid chessboard"));

    bin()
        .args([
            "generate",
            "--spec",
            spec_path.to_str().expect("utf8"),
            "--out-stem",
            out_stem.to_str().expect("utf8"),
        ])
        .assert()
        .success();

    assert_bundle_written(&out_stem);
}

#[test]
fn init_validate_then_generate_puzzleboard_bundle() {
    let dir = tempdir().expect("tempdir");
    let spec_path = dir.path().join("puzzle.json");
    let out_stem = dir.path().join("generated/puzzle");

    bin()
        .args([
            "init",
            "puzzleboard",
            "--out",
            spec_path.to_str().expect("utf8"),
            "--rows",
            "8",
            "--cols",
            "10",
            "--square-size-mm",
            "15",
        ])
        .assert()
        .success();

    bin()
        .args(["validate", "--spec", spec_path.to_str().expect("utf8")])
        .assert()
        .success()
        .stdout(predicate::str::contains("valid puzzleboard"));

    bin()
        .args([
            "generate",
            "--spec",
            spec_path.to_str().expect("utf8"),
            "--out-stem",
            out_stem.to_str().expect("utf8"),
        ])
        .assert()
        .success();

    assert_bundle_written(&out_stem);
}

#[test]
fn validate_rejects_bad_spec() {
    let dir = tempdir().expect("tempdir");
    let spec_path = dir.path().join("bad.json");
    fs::write(
        &spec_path,
        r#"{
  "schema_version": 1,
  "target": {
    "kind": "chessboard",
    "inner_rows": 6,
    "inner_cols": 8,
    "square_size_mm": 20.0
  },
  "page": {
    "size": { "kind": "custom", "width_mm": 50.0, "height_mm": 50.0 },
    "orientation": "portrait",
    "margin_mm": 10.0
  },
  "render": {
    "debug_annotations": false,
    "png_dpi": 300
  }
}"#,
    )
    .expect("write spec");

    bin()
        .args(["validate", "--spec", spec_path.to_str().expect("utf8")])
        .assert()
        .failure()
        .stderr(predicate::str::contains("board does not fit page"));
}

#[test]
fn gen_chessboard_writes_bundle() {
    let dir = tempdir().expect("tempdir");
    let out_stem = dir.path().join("chessboard");

    bin()
        .args([
            "gen",
            "chessboard",
            "--out-stem",
            out_stem.to_str().expect("utf8"),
            "--inner-rows",
            "6",
            "--inner-cols",
            "8",
            "--square-size-mm",
            "20",
        ])
        .assert()
        .success();

    assert_bundle_written(&out_stem);
}

#[test]
fn gen_charuco_writes_bundle() {
    let dir = tempdir().expect("tempdir");
    let out_stem = dir.path().join("charuco");

    bin()
        .args([
            "gen",
            "charuco",
            "--out-stem",
            out_stem.to_str().expect("utf8"),
            "--rows",
            "5",
            "--cols",
            "7",
            "--square-size-mm",
            "20",
            "--marker-size-rel",
            "0.75",
            "--dictionary",
            "DICT_4X4_50",
        ])
        .assert()
        .success();

    assert_bundle_written(&out_stem);
}

#[test]
fn gen_puzzleboard_writes_bundle() {
    let dir = tempdir().expect("tempdir");
    let out_stem = dir.path().join("puzzle");

    bin()
        .args([
            "gen",
            "puzzleboard",
            "--out-stem",
            out_stem.to_str().expect("utf8"),
            "--rows",
            "8",
            "--cols",
            "10",
            "--square-size-mm",
            "15",
        ])
        .assert()
        .success();

    assert_bundle_written(&out_stem);
}

#[test]
fn gen_marker_board_writes_bundle() {
    let dir = tempdir().expect("tempdir");
    let out_stem = dir.path().join("marker");

    bin()
        .args([
            "gen",
            "marker-board",
            "--out-stem",
            out_stem.to_str().expect("utf8"),
            "--inner-rows",
            "6",
            "--inner-cols",
            "8",
            "--square-size-mm",
            "20",
        ])
        .assert()
        .success();

    assert_bundle_written(&out_stem);
}

#[test]
fn gen_rejects_unknown_dictionary() {
    let dir = tempdir().expect("tempdir");
    let out_stem = dir.path().join("charuco");

    bin()
        .args([
            "gen",
            "charuco",
            "--out-stem",
            out_stem.to_str().expect("utf8"),
            "--rows",
            "5",
            "--cols",
            "7",
            "--square-size-mm",
            "20",
            "--marker-size-rel",
            "0.75",
            "--dictionary",
            "DICT_DOES_NOT_EXIST",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown dictionary"));
}
