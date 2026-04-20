//! Run the PuzzleBoard detector over a directory of stacked target
//! images (one PNG per target, 6 × 720×540 snaps per image).
//!
//! Each snap is bilinearly upscaled by `--upscale N` (default 1)
//! before detection — `N = 2` is the minimum at which ChESS corners
//! fire reliably on boards with ≲ 2 native pixels per cell. Board
//! geometry is supplied via `--rows`, `--cols`, `--cell-size-mm`
//! and an optional `--origin-row` / `--origin-col` for the master.
//!
//! Writes per-snap `PuzzleboardFrameReport` JSON to
//! `<out>/t{T}s{S}.json`. The schema is a strict superset of the
//! chessboard `CompactFrame`: it carries the same `input_corners`
//! and `chessboard_frame` (re-run with the detector's default
//! params, independent of the sweep configs used for the puzzle
//! decode), plus the puzzle `outcome`, per-stage timings, and the
//! sweep-config index that produced the best result.
//!
//! A matching `summary.json` aggregates detection rate, failure
//! breakdown by `PuzzleBoardDetectError` variant, and edges/BER/
//! confidence statistics.
//!
//! Usage:
//! ```text
//! cargo run --release -p calib-targets-puzzleboard --example run_dataset --features dataset -- \
//!     --dataset <dir-of-stacked-targets> \
//!     --out     <run-output-dir> \
//!     --upscale 2 --rows N --cols N --cell-size-mm F
//! ```

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use calib_targets::detect::{default_chess_config, detect_corners, gray_view, DetectError};
use calib_targets_chessboard::{DebugFrame, Detector as ChessDetector, DetectorParams};
use calib_targets_core::Corner;
use calib_targets_puzzleboard::{
    PuzzleBoardDetectError, PuzzleBoardDetectionResult, PuzzleBoardDetector,
};
use calib_targets_puzzleboard::{PuzzleBoardParams, PuzzleBoardSpec};
use image::imageops::FilterType;
use image::{GenericImageView, GrayImage};
use serde::Serialize;

const SNAP_WIDTH: u32 = 720;
const SNAP_HEIGHT: u32 = 540;
const SNAPS_PER_IMAGE: u32 = 6;

struct Args {
    dataset: PathBuf,
    out: PathBuf,
    upscale: u32,
    rows: u32,
    cols: u32,
    cell_size_mm: f32,
    origin_row: u32,
    origin_col: u32,
}

fn usage_and_exit() -> ! {
    eprintln!(
        "usage: run_dataset --dataset <dir> --out <dir> \
         --rows N --cols N --cell-size-mm F \
         [--upscale N] [--origin-row N] [--origin-col N]"
    );
    std::process::exit(2);
}

fn parse_args() -> Args {
    let mut dataset: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut upscale = 1u32;
    let mut rows: Option<u32> = None;
    let mut cols: Option<u32> = None;
    let mut cell_size_mm: Option<f32> = None;
    let mut origin_row = 0u32;
    let mut origin_col = 0u32;
    let mut args = env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--dataset" => dataset = args.next().map(PathBuf::from),
            "--out" => out = args.next().map(PathBuf::from),
            "--upscale" => {
                upscale = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or_else(|| usage_and_exit())
            }
            "--rows" => rows = args.next().and_then(|v| v.parse().ok()),
            "--cols" => cols = args.next().and_then(|v| v.parse().ok()),
            "--cell-size-mm" => cell_size_mm = args.next().and_then(|v| v.parse().ok()),
            "--origin-row" => origin_row = args.next().and_then(|v| v.parse().ok()).unwrap_or(0),
            "--origin-col" => origin_col = args.next().and_then(|v| v.parse().ok()).unwrap_or(0),
            "-h" | "--help" => usage_and_exit(),
            other => {
                eprintln!("unknown arg: {other}");
                usage_and_exit();
            }
        }
    }
    let Some(dataset) = dataset else {
        usage_and_exit()
    };
    let Some(out) = out else { usage_and_exit() };
    let Some(rows) = rows else { usage_and_exit() };
    let Some(cols) = cols else { usage_and_exit() };
    let Some(cell_size_mm) = cell_size_mm else {
        usage_and_exit()
    };
    if !(1..=4).contains(&upscale) {
        eprintln!("--upscale must be in 1..=4 (got {upscale})");
        std::process::exit(2);
    }
    Args {
        dataset,
        out,
        upscale,
        rows,
        cols,
        cell_size_mm,
        origin_row,
        origin_col,
    }
}

fn main() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Warn)
        .parse_default_env()
        .init();

    let args = parse_args();
    fs::create_dir_all(&args.out).expect("create out dir");

    let spec = PuzzleBoardSpec::with_origin(
        args.rows,
        args.cols,
        args.cell_size_mm,
        args.origin_row,
        args.origin_col,
    )
    .expect("build puzzleboard spec");
    let configs = PuzzleBoardParams::sweep_for_board(&spec);
    eprintln!(
        "spec: rows={} cols={} cell_size_mm={} origin=({},{}) configs={}",
        args.rows,
        args.cols,
        args.cell_size_mm,
        args.origin_row,
        args.origin_col,
        configs.len()
    );

    let targets = collect_targets(&args.dataset);
    if targets.is_empty() {
        eprintln!("no target_*.png in {:?}", args.dataset);
        std::process::exit(1);
    }
    eprintln!(
        "dataset={:?} targets={} out={:?} upscale={}",
        args.dataset,
        targets.len(),
        args.out,
        args.upscale,
    );

    let chess_cfg = default_chess_config();
    let chess_params = DetectorParams::default();

    let mut agg = Aggregate::default();

    for path in &targets {
        let target_idx = parse_target_index(path).expect("target index");
        let img = image::open(path).expect("image").to_luma8();
        for snap_idx in 0..SNAPS_PER_IMAGE {
            let native = extract_snap(&img, snap_idx);
            let snap = maybe_upscale(&native, args.upscale);

            let t0 = Instant::now();
            let corners = detect_corners(&snap, &chess_cfg);
            let t_corners = t0.elapsed();

            let t0 = Instant::now();
            let chess_detector = ChessDetector::new(chess_params.clone());
            let chessboard_frame = chess_detector.detect_debug(&corners);
            let t_chessboard = t0.elapsed();

            let (puzzle_outcome, t_puzzle, best_config_idx) =
                run_puzzle_sweep(&snap, &corners, &configs);

            let report = PuzzleboardFrameReport {
                target_index: target_idx,
                snap_index: snap_idx,
                upscale: args.upscale,
                width: snap.width(),
                height: snap.height(),
                per_stage_ms: StageTimings {
                    corners: duration_ms(t_corners),
                    chessboard: duration_ms(t_chessboard),
                    puzzleboard: duration_ms(t_puzzle),
                },
                best_config_index: best_config_idx,
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
                chessboard_frame,
                outcome: puzzle_outcome,
            };
            agg.record(&report);
            let json = serde_json::to_string(&report).expect("serialize");
            let out_path = args.out.join(format!("t{target_idx}s{snap_idx}.json"));
            fs::write(&out_path, json).expect("write");
        }
    }

    let summary_path = args.out.join("summary.json");
    fs::write(
        &summary_path,
        serde_json::to_string_pretty(&agg.into_summary()).expect("serialize summary"),
    )
    .expect("write summary");

    println!("summary: {:?}", summary_path);
}

/// Run every config in the sweep, return the best outcome along with
/// total wall time and the winning config index.
fn run_puzzle_sweep(
    image: &GrayImage,
    corners: &[Corner],
    configs: &[PuzzleBoardParams],
) -> (PuzzleboardOutcome, Duration, Option<usize>) {
    let view = gray_view(image);
    let t0 = Instant::now();
    let mut best: Option<PuzzleBoardDetectionResult> = None;
    let mut best_idx: Option<usize> = None;
    let mut last_err: Option<PuzzleBoardDetectError> = None;
    for (idx, params) in configs.iter().enumerate() {
        let detector = match PuzzleBoardDetector::new(params.clone()) {
            Ok(d) => d,
            Err(e) => {
                return (PuzzleboardOutcome::err_spec(&e), t0.elapsed(), None);
            }
        };
        match detector.detect(&view, corners) {
            Ok(r) => {
                let better = match &best {
                    None => true,
                    Some(b) => {
                        let new_key = (r.detection.corners.len(), r.decode.mean_confidence);
                        let old_key = (b.detection.corners.len(), b.decode.mean_confidence);
                        new_key.0 > old_key.0 || (new_key.0 == old_key.0 && new_key.1 > old_key.1)
                    }
                };
                if better {
                    best = Some(r);
                    best_idx = Some(idx);
                }
            }
            Err(e) => last_err = Some(e),
        }
    }
    let elapsed = t0.elapsed();
    let outcome = match best {
        Some(r) => PuzzleboardOutcome::Ok(Box::new(r)),
        None => PuzzleboardOutcome::err_detect(
            &last_err.unwrap_or(PuzzleBoardDetectError::DecodeFailed),
        ),
    };
    (outcome, elapsed, best_idx)
}

fn maybe_upscale(img: &GrayImage, upscale: u32) -> GrayImage {
    if upscale == 1 {
        return img.clone();
    }
    let (w, h) = img.dimensions();
    image::imageops::resize(img, w * upscale, h * upscale, FilterType::Triangle)
}

fn extract_snap(image: &GrayImage, snap_idx: u32) -> GrayImage {
    let x0 = snap_idx * SNAP_WIDTH;
    image.view(x0, 0, SNAP_WIDTH, SNAP_HEIGHT).to_image()
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

fn duration_ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

// ---------------------------------------------------------------------------
// JSON schema
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct PuzzleboardFrameReport {
    target_index: u32,
    snap_index: u32,
    upscale: u32,
    width: u32,
    height: u32,
    per_stage_ms: StageTimings,
    best_config_index: Option<usize>,
    input_corners: Vec<CompactInput>,
    chessboard_frame: DebugFrame,
    outcome: PuzzleboardOutcome,
}

#[derive(Serialize)]
struct StageTimings {
    corners: f64,
    chessboard: f64,
    puzzleboard: f64,
}

#[derive(Serialize)]
struct CompactInput {
    x: f32,
    y: f32,
    strength: f32,
    axes_0: [f32; 2],
    axes_1: [f32; 2],
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PuzzleboardOutcome {
    Ok(Box<PuzzleBoardDetectionResult>),
    Err {
        stage: &'static str,
        variant: &'static str,
        message: String,
    },
}

impl PuzzleboardOutcome {
    fn err_detect(e: &PuzzleBoardDetectError) -> Self {
        let (stage, variant) = classify_detect_error(e);
        PuzzleboardOutcome::Err {
            stage,
            variant,
            message: e.to_string(),
        }
    }

    fn err_spec(e: &calib_targets_puzzleboard::PuzzleBoardSpecError) -> Self {
        PuzzleboardOutcome::Err {
            stage: "spec",
            variant: "BoardSpec",
            message: e.to_string(),
        }
    }
}

fn classify_detect_error(e: &PuzzleBoardDetectError) -> (&'static str, &'static str) {
    match e {
        PuzzleBoardDetectError::BoardSpec(_) => ("spec", "BoardSpec"),
        PuzzleBoardDetectError::ChessboardNotDetected => ("chessboard", "ChessboardNotDetected"),
        PuzzleBoardDetectError::NotEnoughEdges { .. } => ("edge_sampling", "NotEnoughEdges"),
        PuzzleBoardDetectError::DecodeFailed => ("decode", "DecodeFailed"),
        PuzzleBoardDetectError::InconsistentPosition => ("decode", "InconsistentPosition"),
        // `PuzzleBoardDetectError` is `#[non_exhaustive]`; unknown variants
        // surface as a generic "decode/Unknown" classification.
        _ => ("decode", "Unknown"),
    }
}

// Unused-but-kept: DetectError→message handling in case we later swap
// `run_puzzle_sweep` back to calling the facade helper.
#[allow(dead_code)]
fn classify_facade_error(e: &DetectError) -> (&'static str, &'static str, String) {
    let msg = e.to_string();
    match e {
        DetectError::PuzzleBoardSpec(_) => ("spec", "BoardSpec", msg),
        DetectError::PuzzleBoardDetect(inner) => {
            let (s, v) = classify_detect_error(inner);
            (s, v, msg)
        }
        _ => ("other", "Other", msg),
    }
}

// ---------------------------------------------------------------------------
// Aggregate summary
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Aggregate {
    n_frames: usize,
    n_detected: usize,
    failure_counts: std::collections::BTreeMap<(String, String), usize>,
    labelled_corners: Vec<usize>,
    edges_observed: Vec<usize>,
    edges_matched: Vec<usize>,
    bit_error_rate: Vec<f32>,
    mean_confidence: Vec<f32>,
    corners_per_snap: Vec<usize>,
    stage_corners_ms: Vec<f64>,
    stage_chess_ms: Vec<f64>,
    stage_puzzle_ms: Vec<f64>,
    config_index_hist: std::collections::BTreeMap<usize, usize>,
}

impl Aggregate {
    fn record(&mut self, r: &PuzzleboardFrameReport) {
        self.n_frames += 1;
        self.corners_per_snap.push(r.input_corners.len());
        self.stage_corners_ms.push(r.per_stage_ms.corners);
        self.stage_chess_ms.push(r.per_stage_ms.chessboard);
        self.stage_puzzle_ms.push(r.per_stage_ms.puzzleboard);
        if let Some(idx) = r.best_config_index {
            *self.config_index_hist.entry(idx).or_insert(0) += 1;
        }
        match &r.outcome {
            PuzzleboardOutcome::Ok(result) => {
                self.n_detected += 1;
                self.labelled_corners.push(result.detection.corners.len());
                self.edges_observed.push(result.decode.edges_observed);
                self.edges_matched.push(result.decode.edges_matched);
                self.bit_error_rate.push(result.decode.bit_error_rate);
                self.mean_confidence.push(result.decode.mean_confidence);
            }
            PuzzleboardOutcome::Err { stage, variant, .. } => {
                *self
                    .failure_counts
                    .entry((stage.to_string(), variant.to_string()))
                    .or_insert(0) += 1;
            }
        }
    }

    fn into_summary(self) -> Summary {
        let n = self.n_frames;
        let rate = if n == 0 {
            0.0
        } else {
            100.0 * self.n_detected as f64 / n as f64
        };
        Summary {
            n_frames: n,
            n_detected: self.n_detected,
            detection_rate_pct: rate,
            failures: self
                .failure_counts
                .into_iter()
                .map(|((stage, variant), count)| FailureCount {
                    stage,
                    variant,
                    count,
                })
                .collect(),
            labelled_corners: stats_usize(&self.labelled_corners),
            edges_observed: stats_usize(&self.edges_observed),
            edges_matched: stats_usize(&self.edges_matched),
            bit_error_rate: stats_f32(&self.bit_error_rate),
            mean_confidence: stats_f32(&self.mean_confidence),
            corners_per_snap: stats_usize(&self.corners_per_snap),
            stage_ms_corners: stats_f64(&self.stage_corners_ms),
            stage_ms_chessboard: stats_f64(&self.stage_chess_ms),
            stage_ms_puzzleboard: stats_f64(&self.stage_puzzle_ms),
            best_config_histogram: self.config_index_hist.into_iter().collect(),
        }
    }
}

#[derive(Serialize)]
struct Summary {
    n_frames: usize,
    n_detected: usize,
    detection_rate_pct: f64,
    failures: Vec<FailureCount>,
    labelled_corners: StatsUsize,
    edges_observed: StatsUsize,
    edges_matched: StatsUsize,
    bit_error_rate: StatsF32,
    mean_confidence: StatsF32,
    corners_per_snap: StatsUsize,
    stage_ms_corners: StatsF64,
    stage_ms_chessboard: StatsF64,
    stage_ms_puzzleboard: StatsF64,
    best_config_histogram: Vec<(usize, usize)>,
}

#[derive(Serialize)]
struct FailureCount {
    stage: String,
    variant: String,
    count: usize,
}

#[derive(Serialize, Default)]
struct StatsUsize {
    n: usize,
    min: usize,
    p10: usize,
    median: usize,
    p90: usize,
    max: usize,
    mean: f64,
}

#[derive(Serialize, Default)]
struct StatsF32 {
    n: usize,
    min: f32,
    median: f32,
    p90: f32,
    max: f32,
    mean: f64,
}

#[derive(Serialize, Default)]
struct StatsF64 {
    n: usize,
    min: f64,
    median: f64,
    p90: f64,
    max: f64,
    mean: f64,
}

fn stats_usize(v: &[usize]) -> StatsUsize {
    if v.is_empty() {
        return StatsUsize::default();
    }
    let mut s = v.to_vec();
    s.sort_unstable();
    let n = s.len();
    let idx = |p: f64| -> usize { ((p * (n - 1) as f64).round() as usize).min(n - 1) };
    let sum: usize = s.iter().sum();
    StatsUsize {
        n,
        min: s[0],
        p10: s[idx(0.10)],
        median: s[idx(0.50)],
        p90: s[idx(0.90)],
        max: s[n - 1],
        mean: sum as f64 / n as f64,
    }
}

fn stats_f32(v: &[f32]) -> StatsF32 {
    if v.is_empty() {
        return StatsF32::default();
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = s.len();
    let idx = |p: f64| -> usize { ((p * (n - 1) as f64).round() as usize).min(n - 1) };
    let sum: f64 = s.iter().map(|x| *x as f64).sum();
    StatsF32 {
        n,
        min: s[0],
        median: s[idx(0.50)],
        p90: s[idx(0.90)],
        max: s[n - 1],
        mean: sum / n as f64,
    }
}

fn stats_f64(v: &[f64]) -> StatsF64 {
    if v.is_empty() {
        return StatsF64::default();
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = s.len();
    let idx = |p: f64| -> usize { ((p * (n - 1) as f64).round() as usize).min(n - 1) };
    let sum: f64 = s.iter().sum();
    StatsF64 {
        n,
        min: s[0],
        median: s[idx(0.50)],
        p90: s[idx(0.90)],
        max: s[n - 1],
        mean: sum / n as f64,
    }
}
