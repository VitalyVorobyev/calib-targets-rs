//! Run the v2 detector on a single image and emit debug JSON for the
//! Python overlay. Runs the default `DetectorParams` and (optionally)
//! a best-of-3 sweep (`DetectorParams::sweep_default()`), writing one
//! [`CompactFrame`] JSON per configuration.
//!
//! Used by `scripts/chessboard_regression_overlays.sh` to produce the
//! per-image testdata inspection set. Single-image mode, real
//! `(width, height)` in the emitted JSON (not the dataset's fixed
//! 720×540) — the overlay script reads dimensions from the JSON.
//!
//! Usage:
//! ```text
//! cargo run --release -p calib-targets-chessboard --example debug_single \
//!     --features dataset -- \
//!     --image testdata/mid.png \
//!     --out-default bench_results/.../mid_default.json \
//!     --out-sweep   bench_results/.../mid_sweep.json
//! ```
//!
//! Either `--out-default` or `--out-sweep` (or both) may be provided;
//! at least one is required. Stdout receives one TSV row per written
//! run:
//!
//! ```text
//! <image>\t<config>\tdet=<bool>\tlabelled=<N>\tblacklisted=<M>\tcomponents=<K>
//! ```
//!
//! `components` is reported only for the default config (a separate
//! `detect_all` pass). The sweep row reports `components=-`.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process;

use calib_targets_chessboard::{CornerStage, DebugFrame, Detector, DetectorParams};

use calib_targets::detect::{default_chess_config, detect_corners};
use calib_targets_core::Corner;

fn main() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Warn)
        .parse_default_env()
        .init();

    let Cli {
        image,
        out_default,
        out_sweep,
    } = Cli::parse_or_exit();

    if out_default.is_none() && out_sweep.is_none() {
        eprintln!("error: provide at least one of --out-default / --out-sweep");
        process::exit(2);
    }

    let img = image::open(&image)
        .unwrap_or_else(|e| {
            eprintln!("error: failed to open {image:?}: {e}");
            process::exit(1);
        })
        .to_luma8();
    let width = img.width();
    let height = img.height();

    let chess_cfg = default_chess_config();
    let corners = detect_corners(&img, &chess_cfg);
    let n_input_corners = corners.len();
    let image_tag = image
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("image")
        .to_string();

    if let Some(path) = &out_default {
        let params = DetectorParams::default();
        let detector = Detector::new(params.clone());
        let frame = detector.detect_debug(&corners);
        let components = detector.detect_all(&corners).len();
        write_compact_frame(path, &image_tag, width, height, &corners, &frame);
        let (detected, labelled, blacklisted) = frame_stats(&frame);
        println!(
            "{}\tdefault\tdet={}\tlabelled={}\tblacklisted={}\tcomponents={}\tinput={}",
            image.display(),
            detected,
            labelled,
            blacklisted,
            components,
            n_input_corners
        );
    }

    if let Some(path) = &out_sweep {
        let configs = DetectorParams::sweep_default();
        let best = configs
            .iter()
            .map(|params| Detector::new(params.clone()).detect_debug(&corners))
            .max_by(|a, b| frame_quality(a).cmp(&frame_quality(b)))
            .expect("sweep_default returns at least one config");
        write_compact_frame(path, &image_tag, width, height, &corners, &best);
        let (detected, labelled, blacklisted) = frame_stats(&best);
        println!(
            "{}\tsweep\tdet={}\tlabelled={}\tblacklisted={}\tcomponents=-\tinput={}",
            image.display(),
            detected,
            labelled,
            blacklisted,
            n_input_corners
        );
    }
}

struct Cli {
    image: PathBuf,
    out_default: Option<PathBuf>,
    out_sweep: Option<PathBuf>,
}

impl Cli {
    fn parse_or_exit() -> Self {
        let mut image: Option<PathBuf> = None;
        let mut out_default: Option<PathBuf> = None;
        let mut out_sweep: Option<PathBuf> = None;
        let mut args = env::args().skip(1);
        while let Some(a) = args.next() {
            match a.as_str() {
                "--image" => image = args.next().map(PathBuf::from),
                "--out-default" => out_default = args.next().map(PathBuf::from),
                "--out-sweep" => out_sweep = args.next().map(PathBuf::from),
                "--help" | "-h" => {
                    eprintln!(
                        "usage: debug_single --image <png> [--out-default <json>] [--out-sweep <json>]"
                    );
                    process::exit(0);
                }
                other => {
                    eprintln!("error: unknown arg: {other}");
                    process::exit(2);
                }
            }
        }
        let image = image.unwrap_or_else(|| {
            eprintln!("error: --image required");
            process::exit(2);
        });
        Self {
            image,
            out_default,
            out_sweep,
        }
    }
}

/// Sort key for "best" frame in the sweep pass. Primary: detection
/// exists and has more labelled corners. Fallback (no detection):
/// more `Labeled`-stage corners in the augmented list — a partially-
/// grown frame is still more informative to inspect than an empty
/// one. Ties resolved by fewer blacklisted corners.
fn frame_quality(frame: &DebugFrame) -> (u8, usize, i64) {
    let (has_det, labelled_count, blacklisted) = frame_stats(frame);
    let has_det_key = if has_det { 1u8 } else { 0u8 };
    // Higher blacklisted count is worse — negate so `max_by` picks the
    // frame with fewer blacklisted corners.
    (has_det_key, labelled_count, -(blacklisted as i64))
}

fn frame_stats(frame: &DebugFrame) -> (bool, usize, usize) {
    let has_det = frame.detection.is_some();
    let labelled_count = if let Some(det) = &frame.detection {
        det.target.corners.len()
    } else {
        frame
            .corners
            .iter()
            .filter(|c| matches!(c.stage, CornerStage::Labeled { .. }))
            .count()
    };
    let blacklisted = frame
        .corners
        .iter()
        .filter(|c| matches!(c.stage, CornerStage::LabeledThenBlacklisted { .. }))
        .count();
    (has_det, labelled_count, blacklisted)
}

fn write_compact_frame(
    path: &PathBuf,
    image_tag: &str,
    width: u32,
    height: u32,
    corners: &[Corner],
    frame: &DebugFrame,
) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent dir");
    }
    let compact = CompactFrame {
        image_tag: image_tag.to_string(),
        target_index: 0,
        snap_index: 0,
        width,
        height,
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
    };
    let json = serde_json::to_string(&compact).expect("serialize");
    fs::write(path, json).unwrap_or_else(|e| {
        eprintln!("error: write {path:?}: {e}");
        process::exit(1);
    });
}

/// Per-image JSON schema consumed by the Python overlay. Extends the
/// `run_dataset.rs` `CompactFrame` with a human-readable `image_tag`
/// and real image dimensions; keeps `target_index`/`snap_index` for
/// schema compatibility with the dataset consumer.
#[derive(serde::Serialize)]
struct CompactFrame {
    image_tag: String,
    target_index: u32,
    snap_index: u32,
    width: u32,
    height: u32,
    input_corners: Vec<CompactInput>,
    frame: DebugFrame,
}

#[derive(serde::Serialize)]
struct CompactInput {
    x: f32,
    y: f32,
    strength: f32,
    axes_0: [f32; 2],
    axes_1: [f32; 2],
}
