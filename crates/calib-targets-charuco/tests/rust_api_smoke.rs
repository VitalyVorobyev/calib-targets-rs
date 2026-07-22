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
fn downstream_can_name_and_modify_corner_redetect_params() {
    let dir = tempdir().expect("tempdir");
    let manifest_path = dir.path().join("Cargo.toml");
    let main_path = dir.path().join("src/main.rs");
    let charuco_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_root = Path::new(charuco_dir)
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let aruco_dir = workspace_root.join("crates/calib-targets-aruco");

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
chess-corners = "1.0"
chess-corners-core = "1.0"
"#,
            aruco_dir.display(),
        ),
    );

    write_file(
        &main_path,
        r#"use calib_targets_aruco::builtins;
use calib_targets_charuco::{CharucoBoardSpec, CharucoParams, MarkerLayout};
// Advanced ChESS tuning types come from `chess-corners` directly — the
// workspace crates re-export only `DetectorConfig` + `OrientationMethod`.
// Since chess-corners 1.0 the low-level `ChessParams` / `RefinerKind` live on
// the `chess-corners-core` crate root (the `chess_corners::low_level` module
// is gone), so a downstream consumer that names them needs that dependency
// directly — which is what this smoke test pins down.
use chess_corners::SaddlePointConfig;
use chess_corners_core::{ChessParams, RefinerKind};

fn main() {
    let board = CharucoBoardSpec::new(5, 7, 20.0, 0.75, builtins::DICT_4X4_50)
        .with_marker_layout(MarkerLayout::OpenCvCharuco);

    let mut params = CharucoParams::for_board(&board);
    let mut named = ChessParams::default();
    // Single absolute response floor now; the `threshold_rel` /
    // `threshold_abs` pair is gone.
    named.threshold = 0.05;
    named.min_cluster_size = 1;
    // `SaddlePointConfig` is `#[non_exhaustive]` in 1.0, so struct-update
    // syntax no longer compiles across the crate boundary.
    let mut refiner_cfg = SaddlePointConfig::default();
    refiner_cfg.radius = 3;
    named.refiner = RefinerKind::SaddlePoint(refiner_cfg);

    params.corner_redetect_params = named;
    params.corner_redetect_params.threshold = 3.0;
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
