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
fn downstream_can_name_and_construct_workspace_owned_chess_config() {
    let dir = tempdir().expect("tempdir");
    let manifest_path = dir.path().join("Cargo.toml");
    let main_path = dir.path().join("src/main.rs");
    let crate_dir = env!("CARGO_MANIFEST_DIR");

    write_file(
        &manifest_path,
        &format!(
            r#"[package]
name = "workspace_owned_chess_config"
version = "0.1.0"
edition = "2021"

[dependencies]
calib-targets = {{ path = '{crate_dir}' }}
image = "0.25"
"#
        ),
    );

    write_file(
        &main_path,
        r#"use calib_targets::detect::{
    self, ChessConfig, ChessCornerParams, CoarseToFineParams, PyramidParams, RefinerConfig,
    SaddlePointConfig,
};

fn main() {
    let _named_default: ChessConfig = detect::default_chess_config();
    let cfg = ChessConfig {
        params: ChessCornerParams {
            threshold_rel: 0.15,
            min_cluster_size: 1,
            refiner: RefinerConfig::SaddlePoint(SaddlePointConfig::default()),
            ..ChessCornerParams::default()
        },
        multiscale: CoarseToFineParams {
            pyramid: PyramidParams {
                num_levels: 2,
                min_size: 64,
            },
            ..CoarseToFineParams::default()
        },
    };

    let img = image::GrayImage::new(16, 16);
    let _ = detect::detect_corners(&img, &cfg);
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
