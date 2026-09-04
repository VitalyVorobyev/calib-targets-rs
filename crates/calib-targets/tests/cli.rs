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
    let dxf_path = stem.with_extension("dxf");
    assert!(dxf_path.is_file(), "expected DXF output at {dxf_path:?}");
    // DXF must carry the photolith-handoff fingerprint: AC1015 and mm units.
    let dxf = fs::read_to_string(&dxf_path).expect("read dxf");
    assert!(dxf.contains("AC1015"), "DXF should declare AC1015 ACADVER");
    assert!(
        dxf.contains("$INSUNITS\n 70\n4\n"),
        "DXF should declare $INSUNITS = 4 (mm)"
    );
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
fn gen_chessboard_with_inner_square_rel_writes_bundle_with_more_geometry() {
    let dir = tempdir().expect("tempdir");
    let baseline_stem = dir.path().join("baseline");
    let inset_stem = dir.path().join("inset");

    bin()
        .args([
            "gen",
            "chessboard",
            "--out-stem",
            baseline_stem.to_str().expect("utf8"),
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
        .args([
            "gen",
            "chessboard",
            "--out-stem",
            inset_stem.to_str().expect("utf8"),
            "--inner-rows",
            "6",
            "--inner-cols",
            "8",
            "--square-size-mm",
            "20",
            "--inner-square-rel",
            "0.4",
        ])
        .assert()
        .success();

    assert_bundle_written(&baseline_stem);
    assert_bundle_written(&inset_stem);

    let baseline_svg = fs::read_to_string(baseline_stem.with_extension("svg")).expect("svg");
    let inset_svg = fs::read_to_string(inset_stem.with_extension("svg")).expect("svg");
    assert!(
        inset_svg.matches("<rect ").count() > baseline_svg.matches("<rect ").count(),
        "the inset bundle should contain strictly more <rect> elements"
    );

    let baseline_dxf = fs::read_to_string(baseline_stem.with_extension("dxf")).expect("dxf");
    let inset_dxf = fs::read_to_string(inset_stem.with_extension("dxf")).expect("dxf");
    assert!(
        inset_dxf.matches("LWPOLYLINE").count() > baseline_dxf.matches("LWPOLYLINE").count(),
        "the inset bundle's DXF should contain strictly more LWPOLYLINE entities \
         (the hole must reach the DXF, not just SVG/PNG)"
    );

    let inset_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(inset_stem.with_extension("json")).expect("json"))
            .expect("valid json");
    assert_eq!(
        inset_json["target"]["inner_square_rel"],
        serde_json::json!(0.4)
    );
}

#[test]
fn init_charuco_and_marker_board_accept_inner_square_rel() {
    let dir = tempdir().expect("tempdir");

    let charuco_spec = dir.path().join("charuco.json");
    bin()
        .args([
            "init",
            "charuco",
            "--out",
            charuco_spec.to_str().expect("utf8"),
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
            "--inner-square-rel",
            "0.3",
        ])
        .assert()
        .success();
    let charuco_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&charuco_spec).expect("read")).expect("json");
    assert_eq!(
        charuco_json["target"]["inner_square_rel"],
        serde_json::json!(0.3)
    );

    let marker_spec = dir.path().join("marker.json");
    bin()
        .args([
            "init",
            "marker-board",
            "--out",
            marker_spec.to_str().expect("utf8"),
            "--inner-rows",
            "6",
            "--inner-cols",
            "8",
            "--square-size-mm",
            "20",
            "--inner-square-rel",
            "0.4",
        ])
        .assert()
        .success();
    let marker_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&marker_spec).expect("read")).expect("json");
    assert_eq!(
        marker_json["target"]["inner_square_rel"],
        serde_json::json!(0.4)
    );
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
