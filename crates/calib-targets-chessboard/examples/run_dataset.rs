//! Run the chessboard detector over a directory of stacked target
//! images (one PNG per target, 6 × 720×540 snaps per image — the
//! layout of our private flagship datasets).
//!
//! Writes per-snap `DebugFrame` JSON to `<out>/{t{T}s{S}.json}`. The
//! Python overlay scripts consume these.
//!
//! Optional `--upscale N` (1–4, default 1) bilinearly resizes each
//! extracted snap to `(W*N, H*N)` before running the ChESS corner
//! detector and the chessboard graph builder. Corner positions,
//! width and height in the emitted JSON reflect the upscaled
//! coordinate frame; the new `upscale` field lets overlays recover
//! native pixel coordinates by dividing through.
//!
//! Usage:
//! ```text
//! cargo run --release -p calib-targets-chessboard --example run_dataset --features dataset -- \
//!     --dataset <dir-of-stacked-targets> --out bench_results/chessboard_overlays [--upscale 2]
//! ```

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use calib_targets_chessboard::diagnostics::DebugFrame;
use calib_targets_chessboard::{Detector, DetectorParams, GraphBuildAlgorithm};
use image::imageops::FilterType;
use image::{GenericImageView, GrayImage};

use calib_targets::detect::{default_chess_config, detect_corners};
use calib_targets_chessboard::ChessCorner as Corner;

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
    let mut upscale: u32 = 1;
    let mut algorithm = GraphBuildAlgorithm::ChessboardV2;
    let mut timing_only = false;
    let mut min_corner_strength: f32 = 0.0;
    let mut args = env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--dataset" => dataset = args.next().map(PathBuf::from),
            "--out" => out = args.next().map(PathBuf::from),
            "--algorithm" => {
                let raw = args.next().expect("--algorithm needs a value");
                algorithm = parse_algorithm(&raw);
            }
            "--upscale" => {
                let raw = args.next().expect("--upscale needs N");
                upscale = raw.parse().expect("--upscale value must be u32");
            }
            "--timing-only" => timing_only = true,
            "--min-corner-strength" => {
                let raw = args.next().expect("--min-corner-strength needs a value");
                min_corner_strength = raw.parse().expect("--min-corner-strength must be f32");
            }
            other => {
                eprintln!("unknown arg: {other}");
                std::process::exit(2);
            }
        }
    }
    let dataset = dataset.expect("--dataset");
    let out = out.expect("--out");
    if !(1..=4).contains(&upscale) {
        eprintln!("--upscale must be in 1..=4 (got {upscale})");
        std::process::exit(2);
    }
    fs::create_dir_all(&out).expect("create out dir");

    let targets = collect_targets(&dataset);
    if targets.is_empty() {
        eprintln!("no target_*.png in {dataset:?}");
        std::process::exit(1);
    }
    eprintln!(
        "dataset={dataset:?} targets={} out={out:?} upscale={upscale} algorithm={algorithm:?} timing_only={timing_only}",
        targets.len()
    );

    let chess_cfg = default_chess_config();
    let mut detector_params = DetectorParams::default();
    detector_params.graph_build_algorithm = algorithm;
    detector_params.tuning.min_corner_strength = min_corner_strength;
    eprintln!("min_corner_strength={min_corner_strength}");

    let mut n_frames = 0usize;
    let mut n_detected = 0usize;
    let mut sum_labelled = 0usize;
    let mut corners_per_snap: Vec<usize> =
        Vec::with_capacity(targets.len() * SNAPS_PER_IMAGE as usize);
    let mut elapsed_ms: Vec<f64> = Vec::with_capacity(targets.len() * SNAPS_PER_IMAGE as usize);

    for path in &targets {
        let target_idx = parse_target_index(path).expect("target index");
        let img = image::open(path).expect("image").to_luma8();
        for snap_idx in 0..SNAPS_PER_IMAGE {
            let snap_native = extract_snap(&img, snap_idx);
            let snap = maybe_upscale(&snap_native, upscale);
            let started = Instant::now();
            let corners = detect_corners(&snap, &chess_cfg);
            corners_per_snap.push(corners.len());
            let detector = Detector::new(detector_params.clone());
            let (labelled, frame) = match algorithm {
                GraphBuildAlgorithm::ChessboardV2 => {
                    let frame = detector.detect_with_diagnostics(&corners);
                    let labelled = frame
                        .detection
                        .as_ref()
                        .map(|d| d.corners.len())
                        .unwrap_or(0);
                    (labelled, Some(frame))
                }
                GraphBuildAlgorithm::Topological => {
                    let labelled = detector
                        .detect(&corners)
                        .map(|d| d.corners.len())
                        .unwrap_or(0);
                    (labelled, None)
                }
                _ => {
                    eprintln!("unsupported graph-build algorithm");
                    std::process::exit(2);
                }
            };
            let elapsed = started.elapsed().as_secs_f64() * 1e3;
            elapsed_ms.push(elapsed);
            n_frames += 1;
            if labelled > 0 {
                n_detected += 1;
                sum_labelled += labelled;
            }
            if !timing_only {
                let Some(frame) = frame else {
                    eprintln!(
                        "topological --algorithm only supports JSON output with --timing-only"
                    );
                    std::process::exit(2);
                };
                let compact = CompactFrame::from_frame(
                    target_idx,
                    snap_idx,
                    upscale,
                    snap.width(),
                    snap.height(),
                    &corners,
                    &frame,
                    elapsed,
                );
                let json = serde_json::to_string(&compact).expect("serialize");
                let out_path = out.join(format!("t{target_idx}s{snap_idx}.json"));
                fs::write(&out_path, json).expect("write");
            }
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

    print_corner_histogram(&corners_per_snap);
    print_timing_summary(&elapsed_ms);
}

fn parse_algorithm(raw: &str) -> GraphBuildAlgorithm {
    match raw {
        "chessboard-v2" | "chessboard_v2" => GraphBuildAlgorithm::ChessboardV2,
        "topological" => GraphBuildAlgorithm::Topological,
        other => {
            eprintln!("unknown --algorithm {other:?}; expected chessboard-v2 or topological");
            std::process::exit(2);
        }
    }
}

fn maybe_upscale(img: &GrayImage, upscale: u32) -> GrayImage {
    if upscale == 1 {
        return img.clone();
    }
    let (w, h) = img.dimensions();
    image::imageops::resize(img, w * upscale, h * upscale, FilterType::Triangle)
}

fn print_corner_histogram(counts: &[usize]) {
    if counts.is_empty() {
        return;
    }
    let mut sorted: Vec<usize> = counts.to_vec();
    sorted.sort_unstable();
    let n = sorted.len();
    let min = *sorted.first().unwrap();
    let max = *sorted.last().unwrap();
    let sum: usize = sorted.iter().sum();
    let mean = sum as f64 / n as f64;
    let pct = |p: f64| -> usize {
        let idx = ((p / 100.0) * (n - 1) as f64).round() as usize;
        sorted[idx.min(n - 1)]
    };
    let n_lt_50 = sorted.iter().filter(|&&c| c < 50).count();
    let n_lt_200 = sorted.iter().filter(|&&c| c < 200).count();
    let n_ge_500 = sorted.iter().filter(|&&c| c >= 500).count();
    println!(
        "corners/snap: n={n} min={min} p10={} median={} p90={} max={max} mean={mean:.1}",
        pct(10.0),
        pct(50.0),
        pct(90.0)
    );
    println!(
        "  <50: {} ({:.1}%)   <200: {} ({:.1}%)   ≥500: {} ({:.1}%)",
        n_lt_50,
        100.0 * n_lt_50 as f64 / n as f64,
        n_lt_200,
        100.0 * n_lt_200 as f64 / n as f64,
        n_ge_500,
        100.0 * n_ge_500 as f64 / n as f64
    );
}

fn print_timing_summary(values: &[f64]) {
    if values.is_empty() {
        return;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len();
    let sum: f64 = sorted.iter().sum();
    let pct = |p: f64| -> f64 {
        let idx = ((p / 100.0) * (n - 1) as f64).round() as usize;
        sorted[idx.min(n - 1)]
    };
    println!(
        "elapsed_ms/snap: n={n} mean={:.2} p50={:.2} p90={:.2} p95={:.2} max={:.2}",
        sum / n as f64,
        pct(50.0),
        pct(90.0),
        pct(95.0),
        *sorted.last().unwrap()
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
///
/// `width` and `height` refer to the image the detector ran on
/// (post-upscale). `upscale` is the factor applied to the native
/// snap, so overlays can recover native-pixel coordinates by
/// dividing corner positions through.
#[derive(serde::Serialize)]
struct CompactFrame {
    target_index: u32,
    snap_index: u32,
    upscale: u32,
    width: u32,
    height: u32,
    input_corners: Vec<CompactInput>,
    frame: DebugFrame,
    elapsed_ms: f64,
}

impl CompactFrame {
    fn from_frame(
        target_index: u32,
        snap_index: u32,
        upscale: u32,
        width: u32,
        height: u32,
        corners: &[Corner],
        frame: &DebugFrame,
        elapsed_ms: f64,
    ) -> Self {
        Self {
            target_index,
            snap_index,
            upscale,
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
            elapsed_ms,
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
