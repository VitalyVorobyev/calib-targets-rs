use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent dirs");
    }
    fs::write(path, contents).expect("write file");
}

#[test]
fn downstream_can_name_and_modify_corner_redetect_params_with_workspace_types() {
    let dir = tempdir().expect("tempdir");
    let manifest_path = dir.path().join("Cargo.toml");
    let main_path = dir.path().join("src/main.rs");
    let charuco_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_root = Path::new(charuco_dir)
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let aruco_dir = workspace_root.join("crates/calib-targets-aruco");
    let core_dir = workspace_root.join("crates/calib-targets-core");

    write_file(
        &manifest_path,
        &format!(
            r#"[package]
name = "workspace_owned_charuco_redetect"
version = "0.1.0"
edition = "2021"

[dependencies]
calib-targets-aruco = {{ path = '{}' }}
calib-targets-charuco = {{ path = '{charuco_dir}' }}
calib-targets-core = {{ path = '{}' }}
"#,
            aruco_dir.display(),
            core_dir.display()
        ),
    );

    write_file(
        &main_path,
        r#"use calib_targets_aruco::builtins;
use calib_targets_charuco::{CharucoBoardSpec, CharucoParams, MarkerLayout};
use calib_targets_core::{ChessCornerParams, RefinerKind, SaddlePointConfig};

fn main() {
    let board = CharucoBoardSpec {
        rows: 5,
        cols: 7,
        cell_size: 20.0,
        marker_size_rel: 0.75,
        dictionary: builtins::DICT_4X4_50,
        marker_layout: MarkerLayout::OpenCvCharuco,
    };

    let mut params = CharucoParams::for_board(&board);
    let mut named = ChessCornerParams::default();
    named.threshold_rel = 0.05;
    named.min_cluster_size = 1;
    named.refiner = RefinerKind::SaddlePoint(SaddlePointConfig {
        radius: 3,
        ..SaddlePointConfig::default()
    });

    params.corner_redetect_params = named;
    params.corner_redetect_params.threshold_abs = Some(3.0);
}
"#,
    );

    let output = Command::new("cargo")
        .arg("check")
        .arg("--offline")
        .arg("--manifest-path")
        .arg(&manifest_path)
        .env("CARGO_TARGET_DIR", dir.path().join("target"))
        .output()
        .expect("run cargo check");

    assert!(
        output.status.success(),
        "cargo check failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
