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

/// A downstream crate can name the public params surface — `CharucoParams`,
/// `CharucoAdvancedTuning`, and the `with_advanced` builder — and set the
/// opt-in advanced tuning knobs across the crate boundary.
///
/// `corner_redetect_params` is deliberately **not** part of this contract: it
/// is `pub(crate)` (an internal knob that leaks the upstream `chess_corners`
/// parameter type and is `#[serde(skip)]`), so a downstream consumer can no
/// longer name or mutate it. That revocation is exercised implicitly here — the
/// generated crate builds without any `chess-corners*` dependency.
#[test]
fn downstream_can_name_and_configure_advanced_tuning() {
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
name = "workspace_owned_charuco_advanced"
version = "0.1.0"
edition = "2021"
# Pin the workspace MSRV and opt into the MSRV-aware resolver so this
# generated crate resolves the dependency versions a downstream consumer at
# our stated MSRV would get. Without it the resolver picks the newest cached
# version of every transitive dep, and a dep that raises its own rust-version
# breaks this test for reasons unrelated to our API.
rust-version = "1.91"
resolver = "3"

[dependencies]
calib-targets-aruco = {{ path = '{}' }}
calib-targets-charuco = {{ path = '{charuco_dir}' }}
"#,
            aruco_dir.display(),
        ),
    );

    write_file(
        &main_path,
        r#"use calib_targets_aruco::builtins;
use calib_targets_charuco::{
    CharucoAdvancedTuning, CharucoBoardSpec, CharucoParams, MarkerLayout,
};

fn main() {
    let board = CharucoBoardSpec::new(5, 7, 20.0, 0.75, builtins::DICT_4X4_50)
        .with_marker_layout(MarkerLayout::OpenCvCharuco);

    // The opt-in, unstable advanced knobs are settable through the public
    // builder; the moved knobs (grid smoothness / corner validation /
    // secondary inliers) now live here rather than on the stable core.
    let mut advanced = CharucoAdvancedTuning::default();
    advanced.grid_smoothness_threshold_rel = 0.06;
    advanced.corner_validation_threshold_rel = 0.1;
    advanced.min_secondary_marker_inliers = 2;

    let params = CharucoParams::for_board(board).with_advanced(advanced);
    // `effective_tuning()` resolves the configured knobs.
    assert_eq!(params.effective_tuning().min_secondary_marker_inliers, 2);
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
