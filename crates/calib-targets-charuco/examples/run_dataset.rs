//! Run the ChArUco detector over a directory of stacked target images
//! (one PNG per target, N × WxH snaps per image — the layout of our
//! private flagship datasets).
//!
//! Writes a per-snap `CharucoFrameReport` JSON to
//! `<out>/t{T}s{S}.json`, plus an aggregate `summary.json` recording
//! detection rate, mean marker recall, and the self-consistency
//! wrong-id total across the sweep.
//!
//! `board.json` and `config.json` (under `privatedata/`) both use the
//! printing-tool schema (`{ncols, nrows, cellsize_mm, marker_scale,
//! dict}`), which [`calib_targets_charuco::load_board_spec_any`]
//! accepts both flat and nested under a `target` key.
//!
//! Usage:
//! ```text
//! cargo run --release -p calib-targets-charuco --features dataset \
//!     --example run_dataset -- \
//!     --dataset privatedata/<dataset-dir> \
//!     --board   privatedata/<dataset-dir>/board.json \
//!     --out     bench_results/charuco/<dataset-dir>
//! ```

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use calib_targets::detect::{default_chess_config, detect_corners};
use calib_targets_charuco::{
    load_board_spec_any, CharucoBoardSpec, CharucoDetectDiagnostics, CharucoDetectError,
    CharucoDetectionResult, CharucoDetector, CharucoParams,
};
use calib_targets_core::{Corner, GrayImageView};
use image::GenericImageView;
use serde::Serialize;

const DEFAULT_SNAP_WIDTH: u32 = 720;
const DEFAULT_SNAP_HEIGHT: u32 = 540;
const DEFAULT_SNAPS_PER_IMAGE: u32 = 6;

struct Args {
    dataset: PathBuf,
    board: PathBuf,
    out: PathBuf,
    upscale: u32,
    snaps: u32,
    snap_width: u32,
    snap_height: u32,
    use_board_matcher: bool,
    emit_diag: bool,
    save_snaps: bool,
    bit_slope: Option<f32>,
    min_margin: Option<f32>,
}

fn usage_and_exit() -> ! {
    eprintln!(
        "usage: run_dataset --dataset <dir> --board <path> --out <dir> \
         [--upscale N] [--snaps N] [--snap-width N] [--snap-height N] \
         [--use-board-matcher] [--emit-diag] [--save-snaps]"
    );
    std::process::exit(2);
}

fn parse_args() -> Args {
    let mut dataset: Option<PathBuf> = None;
    let mut board: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut upscale = 1u32;
    let mut snaps = DEFAULT_SNAPS_PER_IMAGE;
    let mut snap_width = DEFAULT_SNAP_WIDTH;
    let mut snap_height = DEFAULT_SNAP_HEIGHT;

    let mut use_board_matcher = false;
    let mut emit_diag = false;
    let mut save_snaps = false;
    let mut bit_slope: Option<f32> = None;
    let mut min_margin: Option<f32> = None;

    let mut it = env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--dataset" => dataset = it.next().map(PathBuf::from),
            "--board" => board = it.next().map(PathBuf::from),
            "--out" => out = it.next().map(PathBuf::from),
            "--upscale" => upscale = it.next().and_then(|v| v.parse().ok()).unwrap_or(1),
            "--snaps" => {
                snaps = it.next().and_then(|v| v.parse().ok()).unwrap_or(snaps);
            }
            "--snap-width" => {
                snap_width = it.next().and_then(|v| v.parse().ok()).unwrap_or(snap_width);
            }
            "--snap-height" => {
                snap_height = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(snap_height);
            }
            "--use-board-matcher" => use_board_matcher = true,
            "--emit-diag" => emit_diag = true,
            "--save-snaps" => save_snaps = true,
            "--bit-slope" => bit_slope = it.next().and_then(|v| v.parse().ok()),
            "--min-margin" => min_margin = it.next().and_then(|v| v.parse().ok()),
            "-h" | "--help" => usage_and_exit(),
            other => {
                eprintln!("unknown arg: {other}");
                usage_and_exit();
            }
        }
    }

    if !(1..=4).contains(&upscale) {
        eprintln!("--upscale must be in 1..=4 (got {upscale})");
        std::process::exit(2);
    }
    Args {
        dataset: dataset.unwrap_or_else(|| usage_and_exit()),
        board: board.unwrap_or_else(|| usage_and_exit()),
        out: out.unwrap_or_else(|| usage_and_exit()),
        upscale,
        snaps,
        snap_width,
        snap_height,
        use_board_matcher,
        emit_diag,
        save_snaps,
        bit_slope,
        min_margin,
    }
}

fn main() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Warn)
        .parse_default_env()
        .init();

    let args = parse_args();
    fs::create_dir_all(&args.out).expect("create out dir");

    let spec = load_board_spec_any(&args.board).expect("load board spec");
    eprintln!(
        "board: {}x{} cells={:.3} mm dict={} marker_scale={:.3}",
        spec.cols, spec.rows, spec.cell_size, spec.dictionary.name, spec.marker_size_rel
    );

    let targets = collect_targets(&args.dataset);
    if targets.is_empty() {
        eprintln!("no target_*.png in {:?}", args.dataset);
        std::process::exit(1);
    }
    eprintln!(
        "dataset={:?} targets={} upscale={} out={:?}",
        args.dataset,
        targets.len(),
        args.upscale,
        args.out
    );

    let chess_cfg = default_chess_config();
    let mut params = CharucoParams::for_board(&spec);
    params.use_board_level_matcher = args.use_board_matcher;
    if args.use_board_matcher {
        // The board-level matcher is its own inlier gate — don't add a
        // second threshold on top. Set a floor of 1 so single-cell
        // detections still land (matcher will reject them via the margin
        // gate).
        params.min_marker_inliers = 1;
        params.min_secondary_marker_inliers = 1;
    }
    if let Some(slope) = args.bit_slope {
        params.bit_likelihood_slope = slope;
    }
    if let Some(min_margin) = args.min_margin {
        params.alignment_min_margin = min_margin;
    }
    eprintln!(
        "matcher: {}",
        if args.use_board_matcher {
            "board-level (soft-bit log-likelihood)"
        } else {
            "legacy (rotation + translation vote)"
        }
    );
    let detector = CharucoDetector::new(params.clone()).expect("build detector");

    let mut agg = Aggregate::default();

    for path in &targets {
        let target_idx = parse_target_index(path).expect("target index");
        let img = image::open(path)
            .unwrap_or_else(|e| panic!("decode {}: {e}", path.display()))
            .to_luma8();
        for snap_idx in 0..args.snaps {
            let snap = extract_snap(&img, snap_idx, args.snap_width, args.snap_height);
            let snap = if args.upscale > 1 {
                upscale_image(&snap, args.upscale)
            } else {
                snap
            };
            let (report, diag) = run_one(
                target_idx,
                snap_idx,
                &snap,
                args.upscale,
                &chess_cfg,
                &detector,
                &spec,
                args.emit_diag,
            );
            agg.record(&report);

            let json = serde_json::to_string(&report).expect("serialize");
            let out_path = args.out.join(format!("t{target_idx}s{snap_idx}.json"));
            fs::write(&out_path, json).expect("write");

            if let Some(diag) = diag {
                let diag_path = args.out.join(format!("t{target_idx}s{snap_idx}_diag.json"));
                fs::write(
                    &diag_path,
                    serde_json::to_string(&diag).expect("serialize diag"),
                )
                .expect("write diag");
            }

            if args.save_snaps {
                let snap_path = args.out.join(format!("t{target_idx}s{snap_idx}.png"));
                snap.save(&snap_path).expect("write snap");
            }
        }
    }

    let summary = agg.finish(&args, &spec);
    let summary_path = args.out.join("summary.json");
    fs::write(
        &summary_path,
        serde_json::to_string_pretty(&summary).expect("serialize summary"),
    )
    .expect("write summary");

    println!(
        "frames={} detected={} rate={:.1}% markers_mean={:.1} corners_mean={:.1} wrong_id_total={} runtime_mean_ms={:.1}",
        summary.frames,
        summary.detected,
        summary.detection_rate_pct,
        summary.markers_decoded_mean,
        summary.charuco_corners_mean,
        summary.raw_wrong_id_total,
        summary.runtime_mean_ms,
    );
    println!("summary: {}", summary_path.display());
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

fn extract_snap(
    image: &image::GrayImage,
    snap_idx: u32,
    snap_width: u32,
    snap_height: u32,
) -> image::GrayImage {
    let x0 = snap_idx * snap_width;
    image.view(x0, 0, snap_width, snap_height).to_image()
}

fn upscale_image(src: &image::GrayImage, factor: u32) -> image::GrayImage {
    image::imageops::resize(
        src,
        src.width() * factor,
        src.height() * factor,
        image::imageops::FilterType::Lanczos3,
    )
}

#[allow(clippy::too_many_arguments)]
fn run_one(
    target_index: u32,
    snap_index: u32,
    snap: &image::GrayImage,
    upscale: u32,
    chess_cfg: &calib_targets_core::ChessConfig,
    detector: &CharucoDetector,
    board: &CharucoBoardSpec,
    emit_diag: bool,
) -> (CharucoFrameReport, Option<FrameDiag>) {
    let width = snap.width();
    let height = snap.height();

    let t_total = Instant::now();
    let t_chess = Instant::now();
    let corners: Vec<Corner> = detect_corners(snap, chess_cfg);
    let chess_ms = t_chess.elapsed().as_secs_f32() * 1000.0;

    let view = GrayImageView {
        width: width as usize,
        height: height as usize,
        data: snap.as_raw(),
    };

    let t_detect = Instant::now();
    let (outcome, detect_diag) = detector.detect_with_diagnostics(&view, &corners);
    let detect_ms = t_detect.elapsed().as_secs_f32() * 1000.0;
    let total_ms = t_total.elapsed().as_secs_f32() * 1000.0;

    // Pull the best chosen hypothesis's margin out of the diagnostics so
    // the aggregate frame JSON carries it without requiring the caller to
    // also read the diag JSON.
    let alignment_margin = detect_diag
        .components
        .iter()
        .filter_map(|c| c.board.as_ref())
        .map(|b| b.margin)
        .fold(0.0f32, f32::max);

    let metrics = match &outcome {
        Ok(res) => FrameMetrics {
            chessboard_corners: corners.len(),
            markers_decoded: res.raw_marker_count,
            markers_inlier: res.markers.len(),
            markers_wrong_id: res.raw_marker_wrong_id_count,
            charuco_corners: res.detection.corners.len(),
            alignment_margin,
        },
        Err(_) => FrameMetrics {
            chessboard_corners: corners.len(),
            alignment_margin,
            ..FrameMetrics::default()
        },
    };

    let timings = StageTimings {
        chess_ms,
        detect_ms,
        total_ms,
    };

    let detection_report = outcome
        .as_ref()
        .ok()
        .map(|res| detection_report_from_result(*board, res));
    let error = outcome.as_ref().err().map(error_to_string);

    let report = CharucoFrameReport {
        target_index,
        snap_index,
        width,
        height,
        upscale,
        metrics,
        timings_ms: timings,
        detection: detection_report,
        error,
    };

    let diag = if emit_diag {
        Some(FrameDiag {
            target_index,
            snap_index,
            width,
            height,
            upscale,
            detect: detect_diag,
            input_corners: corners
                .iter()
                .map(|c| [c.position.x, c.position.y])
                .collect(),
            result: outcome.as_ref().ok().map(DetectionSummary::from_result),
        })
    } else {
        None
    };

    (report, diag)
}

/// Full diagnostic JSON for a single snap (matches `FrameDiag` in
/// `overlay_charuco.py`). Emitted when `--emit-diag` is set; otherwise
/// suppressed to keep sweep output small.
#[derive(Serialize)]
struct FrameDiag {
    target_index: u32,
    snap_index: u32,
    width: u32,
    height: u32,
    upscale: u32,
    detect: CharucoDetectDiagnostics,
    /// Raw ChESS corners fed into the detector — useful to overlay the full
    /// input cloud alongside the labelled subset.
    input_corners: Vec<[f32; 2]>,
    /// Final detection result (ChArUco corners with IDs, decoded markers,
    /// alignment). Present only when the detector returned `Ok`.
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<DetectionSummary>,
}

/// Compact per-detection summary suitable for overlay rendering.
#[derive(Serialize)]
struct DetectionSummary {
    corners: Vec<CornerSummary>,
    markers: Vec<MarkerSummary>,
    alignment_transform: [i32; 4],
    alignment_translation: [i32; 2],
}

#[derive(Serialize)]
struct CornerSummary {
    id: Option<u32>,
    grid: Option<[i32; 2]>,
    position: [f32; 2],
    #[serde(skip_serializing_if = "Option::is_none")]
    target_position: Option<[f32; 2]>,
    score: f32,
}

#[derive(Serialize)]
struct MarkerSummary {
    id: u32,
    gc: [i32; 2],
    rotation: u8,
    score: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    corners_img: Option<[[f32; 2]; 4]>,
}

impl DetectionSummary {
    fn from_result(res: &CharucoDetectionResult) -> Self {
        let corners = res
            .detection
            .corners
            .iter()
            .map(|c| CornerSummary {
                id: c.id,
                grid: c.grid.map(|g| [g.i, g.j]),
                position: [c.position.x, c.position.y],
                target_position: c.target_position.map(|p| [p.x, p.y]),
                score: c.score,
            })
            .collect();
        let markers = res
            .markers
            .iter()
            .map(|m| MarkerSummary {
                id: m.id,
                gc: [m.gc.i, m.gc.j],
                rotation: m.rotation,
                score: m.score,
                corners_img: m.corners_img.map(|arr| {
                    [
                        [arr[0].x, arr[0].y],
                        [arr[1].x, arr[1].y],
                        [arr[2].x, arr[2].y],
                        [arr[3].x, arr[3].y],
                    ]
                }),
            })
            .collect();
        Self {
            corners,
            markers,
            alignment_transform: [
                res.alignment.transform.a,
                res.alignment.transform.b,
                res.alignment.transform.c,
                res.alignment.transform.d,
            ],
            alignment_translation: res.alignment.translation,
        }
    }
}

fn detection_report_from_result(
    board: CharucoBoardSpec,
    res: &CharucoDetectionResult,
) -> CompactDetection {
    CompactDetection {
        board,
        corners: res.detection.corners.len(),
        markers: res.markers.len(),
        raw_marker_count: res.raw_marker_count,
        raw_marker_wrong_id_count: res.raw_marker_wrong_id_count,
        alignment_transform: [
            res.alignment.transform.a,
            res.alignment.transform.b,
            res.alignment.transform.c,
            res.alignment.transform.d,
        ],
        alignment_translation: res.alignment.translation,
    }
}

fn error_to_string(err: &CharucoDetectError) -> String {
    err.to_string()
}

#[derive(Serialize)]
struct CharucoFrameReport {
    target_index: u32,
    snap_index: u32,
    width: u32,
    height: u32,
    upscale: u32,
    metrics: FrameMetrics,
    timings_ms: StageTimings,
    #[serde(skip_serializing_if = "Option::is_none")]
    detection: Option<CompactDetection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Default, Serialize, Clone, Copy)]
struct FrameMetrics {
    chessboard_corners: usize,
    markers_decoded: usize,
    markers_inlier: usize,
    markers_wrong_id: usize,
    charuco_corners: usize,
    alignment_margin: f32,
}

#[derive(Serialize, Clone, Copy)]
struct StageTimings {
    chess_ms: f32,
    detect_ms: f32,
    total_ms: f32,
}

/// Compact per-frame detection summary. Deliberately excludes the full
/// corner list to keep sweep output small; the full report is available
/// via the single-image `charuco_detect` example if needed.
#[derive(Serialize)]
struct CompactDetection {
    board: CharucoBoardSpec,
    corners: usize,
    markers: usize,
    raw_marker_count: usize,
    raw_marker_wrong_id_count: usize,
    alignment_transform: [i32; 4],
    alignment_translation: [i32; 2],
}

#[derive(Default)]
struct Aggregate {
    frames: usize,
    detected: usize,
    markers_decoded_sum: usize,
    corners_sum: usize,
    raw_wrong_id_total: usize,
    total_ms_sum: f32,
}

impl Aggregate {
    fn record(&mut self, r: &CharucoFrameReport) {
        self.frames += 1;
        self.total_ms_sum += r.timings_ms.total_ms;
        if r.detection.is_some() {
            self.detected += 1;
            self.markers_decoded_sum += r.metrics.markers_decoded;
            self.corners_sum += r.metrics.charuco_corners;
            self.raw_wrong_id_total += r.metrics.markers_wrong_id;
        }
    }

    fn finish(self, args: &Args, spec: &CharucoBoardSpec) -> SummaryReport {
        let frames = self.frames.max(1) as f32;
        let detected = self.detected.max(1) as f32;
        SummaryReport {
            frames: self.frames,
            detected: self.detected,
            detection_rate_pct: 100.0 * self.detected as f32 / frames,
            markers_decoded_mean: self.markers_decoded_sum as f32 / detected,
            charuco_corners_mean: self.corners_sum as f32 / detected,
            raw_wrong_id_total: self.raw_wrong_id_total,
            runtime_mean_ms: self.total_ms_sum / frames,
            upscale: args.upscale,
            use_board_matcher: args.use_board_matcher,
            board: *spec,
        }
    }
}

#[derive(Serialize)]
struct SummaryReport {
    frames: usize,
    detected: usize,
    detection_rate_pct: f32,
    markers_decoded_mean: f32,
    charuco_corners_mean: f32,
    raw_wrong_id_total: usize,
    runtime_mean_ms: f32,
    upscale: u32,
    use_board_matcher: bool,
    board: CharucoBoardSpec,
}
