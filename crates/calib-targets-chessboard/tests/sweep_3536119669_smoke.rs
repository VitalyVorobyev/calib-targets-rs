//! Smoke test that keeps the `chessboard_sweep_3536119669` harness wiring
//! alive in CI: loads `testdata/3536119669/target_0.png`, slices the first
//! 720x540 sub-frame, and runs the workspace chessboard detector on it.
//!
//! The test does NOT gate on the detector actually finding a grid. The full
//! dataset is evaluated by the dedicated example binary (see the plan file).
//! All we check here is that:
//!
//! 1. the test image exists and loads cleanly,
//! 2. the split into sub-frames stays 720×540,
//! 3. the detector runs a schema-v2 visible-subset report path to completion.
//!
//! When the harness binary is fully wired into the workspace, CI runs this
//! smoke test on every push. The full 120-frame sweep remains opt-in via
//! `cargo run --release --example chessboard_sweep_3536119669`.

use std::path::PathBuf;

use calib_targets_chessboard::{
    score_frame, ChessboardDetector, ChessboardParams, GridGraphParams,
    VISIBLE_SUBSET_GATE_3536119669,
};
use calib_targets_core::{AxisEstimate, Corner};
use chess_corners::{find_chess_corners_image, ChessConfig, ThresholdMode, UpscaleConfig};
use image::{GenericImageView, ImageReader};
use nalgebra::Point2;
use serde::Serialize;

const SMOKE_SCHEMA_VERSION: u32 = 2;

fn testdata_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/3536119669/target_0.png")
}

fn adapt_chess_corner(c: &chess_corners::CornerDescriptor) -> Corner {
    Corner {
        position: Point2::new(c.x, c.y),
        orientation_cluster: None,
        axes: [
            AxisEstimate {
                angle: c.axes[0].angle,
                sigma: c.axes[0].sigma,
            },
            AxisEstimate {
                angle: c.axes[1].angle,
                sigma: c.axes[1].sigma,
            },
        ],
        contrast: c.contrast,
        fit_rms: c.fit_rms,
        strength: c.response,
    }
}

#[derive(Serialize)]
struct SmokeReport {
    schema_version: u32,
    candidates: usize,
    selected_valid_visible_subset: bool,
}

#[test]
fn harness_runs_without_panic_on_first_subframe() {
    let path = testdata_path();
    if !path.exists() {
        eprintln!(
            "testdata/3536119669/target_0.png not found — skipping smoke test ({})",
            path.display()
        );
        return;
    }

    let full = ImageReader::open(&path)
        .expect("open target_0.png")
        .decode()
        .expect("decode target_0.png")
        .to_luma8();
    assert_eq!(full.height(), 540, "expected 540-pixel tall merged image");
    assert!(
        full.width() >= 720,
        "expected at least one 720-pixel-wide snap"
    );

    let sub = full.view(0, 0, 720, 540).to_image();
    assert_eq!(sub.width(), 720);
    assert_eq!(sub.height(), 540);

    let mut configs = Vec::new();
    for threshold in [0.20, 0.08] {
        let mut cfg = ChessConfig::single_scale();
        cfg.threshold_mode = ThresholdMode::Relative;
        cfg.threshold_value = threshold;
        configs.push(cfg);
    }
    let mut upscaled = ChessConfig::single_scale();
    upscaled.threshold_mode = ThresholdMode::Relative;
    upscaled.threshold_value = 0.08;
    upscaled.upscale = UpscaleConfig::fixed(2);
    configs.push(upscaled);

    let mut candidates = 0usize;
    let mut selected_valid_visible_subset = false;
    for chess_cfg in configs {
        let raw = find_chess_corners_image(&sub, &chess_cfg);
        let corners: Vec<Corner> = raw.iter().map(adapt_chess_corner).collect();

        for use_orientation_clustering in [true, false] {
            let params = ChessboardParams {
                expected_rows: None,
                expected_cols: None,
                use_orientation_clustering,
                graph: GridGraphParams {
                    min_spacing_pix: 8.0,
                    max_spacing_pix: 60.0,
                    ..GridGraphParams::default()
                },
                ..ChessboardParams::default()
            };

            let detector = ChessboardDetector::new(params);
            if let Some(res) = detector.detect_from_corners(&corners) {
                candidates += 1;
                let metrics = score_frame(&res.detection, 21, 21);
                selected_valid_visible_subset |=
                    metrics.passes_visible_subset(VISIBLE_SUBSET_GATE_3536119669);
            }
        }
    }

    let report = SmokeReport {
        schema_version: SMOKE_SCHEMA_VERSION,
        candidates,
        selected_valid_visible_subset,
    };
    let json = serde_json::to_value(&report).expect("serialize smoke report");
    assert_eq!(json["schema_version"], SMOKE_SCHEMA_VERSION);
}
