//! Run the v2 detector over a directory of stacked target images
//! (one PNG per target, 6 × 720×540 snaps per image — matches the
//! `testdata/3536119669` layout).
//!
//! Writes per-snap `DebugFrame` JSON to `<out>/{t{T}s{S}.json}`. The
//! Python overlay script consumes these.
//!
//! Usage:
//! ```text
//! cargo run --release -p calib-targets-chessboard --example run_dataset --features dataset -- \
//!     --dataset testdata/3536119669 --out bench_results/chessboard_overlays
//! ```

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use calib_targets_chessboard::{DebugFrame, Detector, DetectorParams};
use image::GenericImageView;

use calib_targets::detect::{default_chess_config, detect_corners};
use calib_targets_core::Corner;

const SNAP_WIDTH: u32 = 720;
const SNAP_HEIGHT: u32 = 540;
const SNAPS_PER_IMAGE: u32 = 6;

fn main() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Warn)
        .parse_default_env()
        .init();

    let mut dataset: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut args = env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--dataset" => dataset = args.next().map(PathBuf::from),
            "--out" => out = args.next().map(PathBuf::from),
            other => {
                eprintln!("unknown arg: {other}");
                std::process::exit(2);
            }
        }
    }
    let dataset = dataset.expect("--dataset");
    let out = out.expect("--out");
    fs::create_dir_all(&out).expect("create out dir");

    let targets = collect_targets(&dataset);
    if targets.is_empty() {
        eprintln!("no target_*.png in {dataset:?}");
        std::process::exit(1);
    }
    eprintln!("dataset={dataset:?} targets={} out={out:?}", targets.len());

    let chess_cfg = default_chess_config();
    let detector_params = DetectorParams::default();

    let mut n_frames = 0usize;
    let mut n_detected = 0usize;
    let mut sum_labelled = 0usize;

    for path in &targets {
        let target_idx = parse_target_index(path).expect("target index");
        let img = image::open(path).expect("image").to_luma8();
        for snap_idx in 0..SNAPS_PER_IMAGE {
            let snap = extract_snap(&img, snap_idx);
            let corners = detect_corners(&snap, &chess_cfg);
            let detector = Detector::new(detector_params.clone());
            let frame = detector.detect_debug(&corners);
            n_frames += 1;
            if let Some(d) = &frame.detection {
                n_detected += 1;
                sum_labelled += d.target.corners.len();
            }
            let compact = CompactFrame::from_frame(target_idx, snap_idx, &corners, &frame);
            let json = serde_json::to_string(&compact).expect("serialize");
            let out_path = out.join(format!("t{target_idx}s{snap_idx}.json"));
            fs::write(&out_path, json).expect("write");
        }
    }

    let pct = if n_frames == 0 {
        0.0
    } else {
        100.0 * n_detected as f32 / n_frames as f32
    };
    let avg_labelled = if n_detected == 0 {
        0.0
    } else {
        sum_labelled as f32 / n_detected as f32
    };
    println!(
        "frames={n_frames} detected={n_detected} rate={pct:.1}% avg_labelled_in_detected={avg_labelled:.1}"
    );
}

fn parse_target_index(path: &Path) -> Option<u32> {
    path.file_stem()
        .and_then(|s| s.to_str())
        .and_then(|s| s.strip_prefix("target_"))
        .and_then(|s| s.parse::<u32>().ok())
}

fn collect_targets(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).expect("read dir").flatten() {
        let p = entry.path();
        if p.is_file()
            && p.file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.starts_with("target_") && !s.contains(' '))
                .unwrap_or(false)
            && p.extension().map(|e| e == "png").unwrap_or(false)
            && parse_target_index(&p).is_some()
        {
            out.push(p);
        }
    }
    out.sort_by_key(|p| parse_target_index(p).unwrap_or(u32::MAX));
    out
}

fn extract_snap(image: &image::GrayImage, snap_idx: u32) -> image::GrayImage {
    let x0 = snap_idx * SNAP_WIDTH;
    let view = image.view(x0, 0, SNAP_WIDTH, SNAP_HEIGHT);
    view.to_image()
}

/// Compact per-frame JSON schema consumed by the Python overlay
/// script. Keeps the raw input corners too so overlays can show the
/// full input cloud next to the labelled subset.
#[derive(serde::Serialize)]
struct CompactFrame {
    target_index: u32,
    snap_index: u32,
    width: u32,
    height: u32,
    input_corners: Vec<CompactInput>,
    frame: DebugFrame,
}

impl CompactFrame {
    fn from_frame(
        target_index: u32,
        snap_index: u32,
        corners: &[Corner],
        frame: &DebugFrame,
    ) -> Self {
        Self {
            target_index,
            snap_index,
            width: SNAP_WIDTH,
            height: SNAP_HEIGHT,
            input_corners: corners
                .iter()
                .map(|c| CompactInput {
                    x: c.position.x,
                    y: c.position.y,
                    strength: c.strength,
                    axes_0: [c.axes[0].angle, c.axes[0].sigma],
                    axes_1: [c.axes[1].angle, c.axes[1].sigma],
                })
                .collect(),
            frame: frame.clone(),
        }
    }
}

#[derive(serde::Serialize)]
struct CompactInput {
    x: f32,
    y: f32,
    strength: f32,
    axes_0: [f32; 2],
    axes_1: [f32; 2],
}
