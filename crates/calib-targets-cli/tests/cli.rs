use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn top_level_help_lists_productized_commands() {
    Command::cargo_bin("calib-targets")
        .expect("binary")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Repo-local CLI for printable calibration target generation",
        ))
        .stdout(predicate::str::contains("validate"))
        .stdout(predicate::str::contains("list-dictionaries"));
}

#[test]
fn list_dictionaries_is_sorted_and_includes_known_name() {
    let output = Command::cargo_bin("calib-targets")
        .expect("binary")
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
        .args(["validate", "--spec", spec_path.to_str().expect("utf8")])
        .assert()
        .success()
        .stdout(predicate::str::contains("valid chessboard"));

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

    Command::cargo_bin("calib-targets")
        .expect("binary")
        .args(["validate", "--spec", spec_path.to_str().expect("utf8")])
        .assert()
        .failure()
        .stderr(predicate::str::contains("board does not fit page"));
}
