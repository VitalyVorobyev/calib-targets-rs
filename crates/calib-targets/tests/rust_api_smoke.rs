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
    self, ChessRefiner, ChessRing, DescriptorRing, DetectionStrategy, DetectorConfig,
    MultiscaleConfig, Threshold,
};

fn main() {
    // `DetectorConfig` is `#[non_exhaustive]` (re-exported from
    // `chess-corners`), so downstream crates must seed it via a preset and
    // mutate the fields they care about; struct literal syntax is
    // intentionally forbidden.
    let _named_default: DetectorConfig = detect::default_chess_config();
    let cfg = DetectorConfig::chess()
        .with_threshold(Threshold::Relative(0.15))
        .with_multiscale(MultiscaleConfig::pyramid(2, 64, 3))
        .with_chess(|c| {
            c.ring = ChessRing::Broad;
            c.descriptor_ring = DescriptorRing::Canonical;
            c.min_cluster_size = 1;
            c.refiner = ChessRefiner::saddle_point();
        });
    assert!(matches!(cfg.strategy, DetectionStrategy::Chess(_)));

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
