use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn init_then_generate_chessboard_bundle() {
    let dir = tempdir().expect("tempdir");
    let spec_path = dir.path().join("board.json");
    let out_stem = dir.path().join("generated/board");

    Command::cargo_bin("calib-targets")
        .expect("binary")
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

    Command::cargo_bin("calib-targets")
        .expect("binary")
        .args([
            "generate",
            "--spec",
            spec_path.to_str().expect("utf8"),
            "--out-stem",
            out_stem.to_str().expect("utf8"),
        ])
        .assert()
        .success();

    assert!(out_stem.with_extension("json").is_file());
    assert!(out_stem.with_extension("svg").is_file());
    assert!(out_stem.with_extension("png").is_file());
}

#[test]
fn generate_rejects_bad_spec() {
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

    Command::cargo_bin("calib-targets")
        .expect("binary")
        .args([
            "generate",
            "--spec",
            spec_path.to_str().expect("utf8"),
            "--out-stem",
            dir.path().join("out").to_str().expect("utf8"),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("board does not fit page"));
}
