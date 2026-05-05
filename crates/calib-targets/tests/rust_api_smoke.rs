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
fn downstream_can_name_and_configure_chess_config() {
    let dir = tempdir().expect("tempdir");
    let manifest_path = dir.path().join("Cargo.toml");
    let main_path = dir.path().join("src/main.rs");
    let crate_dir = env!("CARGO_MANIFEST_DIR");

    write_file(
        &manifest_path,
        &format!(
            r#"[package]
name = "chess_config_downstream"
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
    self, ChessConfig, DetectorMode, DescriptorMode, RefinementMethod, RefinerConfig,
    ThresholdMode,
};

fn main() {
    // `ChessConfig` is `#[non_exhaustive]` (re-exported from `chess-corners`),
    // so downstream crates must seed it via a preset and assign the fields
    // they care about; struct literal syntax is intentionally forbidden.
    let _named_default: ChessConfig = detect::default_chess_config();
    let mut cfg = ChessConfig::single_scale();
    cfg.detector_mode = DetectorMode::Broad;
    cfg.descriptor_mode = DescriptorMode::Canonical;
    cfg.threshold_mode = ThresholdMode::Relative;
    cfg.threshold_value = 0.15;
    cfg.min_cluster_size = 1;
    cfg.refiner = RefinerConfig::saddle_point();
    cfg.pyramid_levels = 2;
    cfg.pyramid_min_size = 64;
    assert_eq!(cfg.refiner.kind, RefinementMethod::SaddlePoint);

    let img = image::GrayImage::new(16, 16);
    let _ = detect::detect_corners(&img, &cfg, 0.0);
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
