//! Run the ChArUco detector over a directory of stacked target images
//! (one PNG per target, N × WxH snaps per image — the layout used by
//! the large real-world calibration datasets).
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

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use calib_targets::detect::{default_chess_config, detect_corners};
use calib_targets_charuco::{
    diagnostics::CharucoDetectDiagnostics, load_board_spec_any, CharucoBoardSpec,
    CharucoDetectError, CharucoDetection, CharucoDetector, CharucoParams,
};
use calib_targets_chessboard::{ChessCorner as Corner, ChessboardDetector};
use calib_targets_core::GrayImageView;
use image::GenericImageView;
use nalgebra::Point2;
use projective_grid::{
    check_consistency, ConsistencyParams, ConsistencyRequest, Coord, CoordinateHypothesis,
    LatticeKind, PointFeature,
};
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
    emit_diag: bool,
    save_snaps: bool,
    bit_slope: Option<f32>,
    min_margin: Option<f32>,
}

/// Stable slug for the grid-build algorithm used in summary JSON + report filenames.
fn algorithm_slug() -> &'static str {
    "topological"
}

fn usage_and_exit() -> ! {
    eprintln!(
        "usage: run_dataset --dataset <dir> --board <path> --out <dir> \
         [--upscale N] [--snaps N] [--snap-width N] [--snap-height N] \
         [--emit-diag] [--save-snaps]"
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
            "--algorithm" => {
                // Accepted for back-compat; the only builder is topological.
                let _ = it.next().unwrap_or_default();
            }
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
        spec.cols,
        spec.rows,
        spec.cell_size,
        spec.dictionary.name(),
        spec.marker_size_rel
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
    let mut params = CharucoParams::for_board(spec);
    // The board-level matcher is its own inlier gate — `for_board` already
    // sets the low (1 / 1) floors, so the matcher's margin gate is what
    // decides accept/reject.
    if args.bit_slope.is_some() || args.min_margin.is_some() {
        let mut advanced = params.effective_tuning().into_owned();
        if let Some(slope) = args.bit_slope {
            advanced.bit_likelihood_slope = slope;
        }
        if let Some(min_margin) = args.min_margin {
            advanced.alignment_min_margin = min_margin;
        }
        params = params.with_advanced(advanced);
    }
    eprintln!(
        "matcher: board-level (soft-bit log-likelihood)  algorithm: {}",
        algorithm_slug(),
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
            let (report, diag) = run_one(&RunCtx {
                target_index: target_idx,
                snap_index: snap_idx,
                snap: &snap,
                upscale: args.upscale,
                chess_cfg: &chess_cfg,
                detector: &detector,
                params: &params,
                board: &spec,
                emit_diag: args.emit_diag,
            });
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
        "algorithm={} frames={} detected={} rate={:.1}% markers_mean={:.1} corners_mean={:.1} wrong_id_total={} runtime_mean_ms={:.1}",
        summary.algorithm,
        summary.frames,
        summary.detected,
        summary.detection_rate_pct,
        summary.markers_decoded_mean,
        summary.charuco_corners_mean,
        summary.raw_wrong_id_total,
        summary.runtime_mean_ms,
    );
    println!(
        "self-consistency: residual/cell p50={:.4} p90={:.4} max={:.4} | \
         emitted dup-position frames={} | chess-stage reused-source frames={} dup-position frames={}",
        summary.residual_over_cell_p50,
        summary.residual_over_cell_p90,
        summary.residual_over_cell_max,
        summary.final_duplicate_position_frames,
        summary.chess_reused_source_frames,
        summary.chess_duplicate_position_frames,
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

struct RunCtx<'a> {
    target_index: u32,
    snap_index: u32,
    snap: &'a image::GrayImage,
    upscale: u32,
    chess_cfg: &'a calib_targets_core::DetectorConfig,
    detector: &'a CharucoDetector,
    /// The same params the detector was built from — the self-consistency audit
    /// re-runs the chessboard stage with them to see the labels *before*
    /// ChArUco maps and refits them.
    params: &'a CharucoParams,
    board: &'a CharucoBoardSpec,
    emit_diag: bool,
}

fn run_one(ctx: &RunCtx<'_>) -> (CharucoFrameReport, Option<FrameDiag>) {
    let target_index = ctx.target_index;
    let snap_index = ctx.snap_index;
    let snap = ctx.snap;
    let upscale = ctx.upscale;
    let chess_cfg = ctx.chess_cfg;
    let detector = ctx.detector;
    let board = ctx.board;
    let emit_diag = ctx.emit_diag;
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

    // Projective self-consistency, measured at two points in the pipeline: the
    // chessboard stage that produced the lattice, and the ChArUco corners
    // actually emitted. Comparing the two localises which stage breaks the
    // `(u, v) -> position` homography.
    let (chess_components, chess_consistency, chess_stage) =
        chessboard_stage_consistency(ctx.params, &corners);
    let final_consistency = outcome
        .as_ref()
        .ok()
        .map(|res| {
            let labels: Vec<(Coord, Point2<f32>)> =
                res.corners.iter().map(|c| (c.grid, c.position)).collect();
            self_consistency(&labels, None)
        })
        .unwrap_or_default();

    let metrics = match &outcome {
        Ok(res) => FrameMetrics {
            chessboard_corners: corners.len(),
            markers_decoded: detect_diag.raw_marker_count,
            markers_inlier: res.markers.len(),
            markers_wrong_id: detect_diag.raw_marker_wrong_id_count,
            charuco_corners: res.corners.len(),
            alignment_margin,
            chess_components,
            chess_consistency,
            final_consistency,
        },
        Err(_) => FrameMetrics {
            chessboard_corners: corners.len(),
            alignment_margin,
            chess_components,
            chess_consistency,
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
        .map(|res| detection_report_from_result(*board, res, &detect_diag));
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
            chess_stage,
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
    /// The chessboard stage's labelled components, captured before ChArUco
    /// maps them to board coordinates and refits their positions. Lets an
    /// audit attribute a broken labelling to the grid builder or to ChArUco.
    chess_stage: Vec<ChessComponentSummary>,
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
    fn from_result(res: &CharucoDetection) -> Self {
        let corners = res
            .corners
            .iter()
            .map(|c| CornerSummary {
                id: Some(c.id),
                grid: Some([c.grid.u, c.grid.v]),
                position: [c.position.x, c.position.y],
                target_position: Some([c.target_position.x, c.target_position.y]),
                score: c.score,
            })
            .collect();
        let markers = res
            .markers
            .iter()
            .map(|m| MarkerSummary {
                id: m.id,
                gc: [m.gc.u, m.gc.v],
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
        let matrix = res.alignment.matrix();
        Self {
            corners,
            markers,
            alignment_transform: [matrix[0][0], matrix[0][1], matrix[1][0], matrix[1][1]],
            alignment_translation: res.alignment.translation(),
        }
    }
}

fn detection_report_from_result(
    board: CharucoBoardSpec,
    res: &CharucoDetection,
    diagnostics: &CharucoDetectDiagnostics,
) -> CompactDetection {
    let matrix = res.alignment.matrix();
    CompactDetection {
        board,
        corners: res.corners.len(),
        markers: res.markers.len(),
        raw_marker_count: diagnostics.raw_marker_count,
        raw_marker_wrong_id_count: diagnostics.raw_marker_wrong_id_count,
        alignment_transform: [matrix[0][0], matrix[0][1], matrix[1][0], matrix[1][1]],
        alignment_translation: res.alignment.translation(),
    }
}

fn error_to_string(err: &CharucoDetectError) -> String {
    err.to_string()
}

// ---------------------------------------------------------------------------
// Projective self-consistency audit
// ---------------------------------------------------------------------------

/// Coincidence radius for the collapsed-label check, as a fraction of the
/// labelled set's own cell pitch. Mirrors `TOPO_DUP_PIXEL_FRAC` in
/// `projective-grid`'s wrong-label filters so both surfaces call the same
/// thing a duplicate.
const DUP_PIXEL_FRAC: f32 = 0.2;

/// Self-consistency audit of one labelled corner set.
///
/// For a planar target the map `(u, v) -> image position` **is a homography by
/// construction**, so a labelled set can be checked against itself: fit the
/// lattice-to-image projective map to the set's own pairs and look at the
/// residual. No ground truth, no camera model, no calibration involved — which
/// makes this usable as a per-frame product signal and not just an offline
/// metric.
#[derive(Default, Serialize, Clone, Copy)]
struct SelfConsistency {
    /// Number of labelled corners the audit saw.
    labels: usize,
    /// `true` when a projective fit was possible (>= 4 labels, non-degenerate).
    fitted: bool,
    /// Median per-corner reprojection residual, pixels.
    median_residual_px: f32,
    /// Largest per-corner reprojection residual, pixels.
    max_residual_px: f32,
    /// Median cardinal-edge length of the labelled set, pixels — the local cell
    /// pitch that makes the residual dimensionless.
    cell_px: f32,
    /// `median_residual_px / cell_px`. Scale-free, so it is comparable across
    /// snaps, board sizes and viewing distances; this is the discriminating
    /// number, not the raw pixel residual.
    median_over_cell: f32,
    /// Pairs of distinct labels whose image positions coincide within
    /// `DUP_PIXEL_FRAC * cell_px` — the collapsed-label signature.
    duplicate_position_pairs: usize,
    /// Source corners bound to more than one lattice coordinate. Non-zero means
    /// the labelled set is not injective, which no homography admits.
    /// Always `0` for sets whose provenance is unknown (the final ChArUco
    /// corners carry no input index).
    reused_source_corners: usize,
}

/// Median cardinal-edge length of a labelled set, in pixels.
fn cell_pitch_px(by_grid: &HashMap<(i32, i32), Point2<f32>>) -> f32 {
    let mut lens: Vec<f32> = Vec::new();
    for (&(u, v), &p) in by_grid {
        for (du, dv) in [(1, 0), (0, 1)] {
            if let Some(&q) = by_grid.get(&(u + du, v + dv)) {
                lens.push((q - p).norm());
            }
        }
    }
    if lens.is_empty() {
        return 0.0;
    }
    lens.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    lens[lens.len() / 2]
}

/// Audit a labelled set. `provenance` carries the source-corner index per label
/// when the caller knows it (the chessboard stage does; the emitted ChArUco
/// corners do not).
fn self_consistency(
    labels: &[(Coord, Point2<f32>)],
    provenance: Option<&[usize]>,
) -> SelfConsistency {
    let mut out = SelfConsistency {
        labels: labels.len(),
        ..SelfConsistency::default()
    };

    if let Some(indices) = provenance {
        let mut coords_per_index: HashMap<usize, usize> = HashMap::new();
        for &idx in indices {
            *coords_per_index.entry(idx).or_insert(0) += 1;
        }
        out.reused_source_corners = coords_per_index.values().filter(|&&n| n > 1).count();
    }

    let by_grid: HashMap<(i32, i32), Point2<f32>> =
        labels.iter().map(|&(c, p)| ((c.u, c.v), p)).collect();
    out.cell_px = cell_pitch_px(&by_grid);

    if out.cell_px > 0.0 {
        let eps2 = (DUP_PIXEL_FRAC * out.cell_px).powi(2);
        for (a, &(_, pa)) in labels.iter().enumerate() {
            for &(_, pb) in &labels[a + 1..] {
                if (pa - pb).norm_squared() < eps2 {
                    out.duplicate_position_pairs += 1;
                }
            }
        }
    }

    if labels.len() < 4 {
        return out;
    }

    let features: Vec<PointFeature> = labels
        .iter()
        .enumerate()
        .map(|(i, &(_, p))| PointFeature::new(i, p))
        .collect();
    let hypotheses: Vec<CoordinateHypothesis> = labels
        .iter()
        .enumerate()
        .map(|(i, &(c, _))| CoordinateHypothesis::new(i, c, None))
        .collect();

    // Infinite tolerance: the audit measures the residual, it does not judge it.
    let request = ConsistencyRequest::new(
        LatticeKind::Square,
        &features,
        &hypotheses,
        None,
        ConsistencyParams::new(f32::INFINITY),
    );
    let Ok(report) = check_consistency(request) else {
        return out;
    };

    let mut residuals: Vec<f32> = report
        .grid()
        .entries()
        .iter()
        .filter_map(|e| e.residual_px)
        .collect();
    if residuals.is_empty() {
        return out;
    }
    residuals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    out.fitted = true;
    out.median_residual_px = residuals[residuals.len() / 2];
    out.max_residual_px = report.fit().residuals.max_px;
    if out.cell_px > 0.0 {
        out.median_over_cell = out.median_residual_px / out.cell_px;
    }
    out
}

/// Audit every component the chessboard stage produced, and return the worst
/// one (highest scale-free residual) alongside the component count.
///
/// This is the *pre-ChArUco* view of the same labelled corners: comparing it
/// against the audit of the emitted ChArUco corners localises which stage
/// breaks projective consistency.
fn chessboard_stage_consistency(
    params: &CharucoParams,
    corners: &[Corner],
) -> (usize, SelfConsistency, Vec<ChessComponentSummary>) {
    let Ok(detector) = ChessboardDetector::new(params.chessboard.clone()) else {
        return (0, SelfConsistency::default(), Vec::new());
    };
    let components = detector.detect_all(corners);
    let mut worst = SelfConsistency::default();
    let mut summaries = Vec::with_capacity(components.len());
    for component in &components {
        let labels: Vec<(Coord, Point2<f32>)> = component
            .corners
            .iter()
            .map(|c| (c.grid, c.position))
            .collect();
        let provenance: Vec<usize> = component.corners.iter().map(|c| c.input_index).collect();
        let audit = self_consistency(&labels, Some(&provenance));
        let worse = audit.reused_source_corners > worst.reused_source_corners
            || (audit.reused_source_corners == worst.reused_source_corners
                && audit.median_over_cell > worst.median_over_cell);
        if worse {
            worst = audit;
        }
        summaries.push(ChessComponentSummary {
            consistency: audit,
            corners: component
                .corners
                .iter()
                .map(|c| ChessCornerSummary {
                    grid: [c.grid.u, c.grid.v],
                    position: [c.position.x, c.position.y],
                    input_index: c.input_index,
                    score: c.score,
                })
                .collect(),
        });
    }
    (components.len(), worst, summaries)
}

/// One chessboard-stage component, as seen *before* ChArUco maps and refits it.
#[derive(Serialize)]
struct ChessComponentSummary {
    consistency: SelfConsistency,
    corners: Vec<ChessCornerSummary>,
}

#[derive(Serialize)]
struct ChessCornerSummary {
    grid: [i32; 2],
    position: [f32; 2],
    /// Index into the detector's input corner slice — the provenance that makes
    /// "one source corner labelled at two coordinates" observable.
    input_index: usize,
    score: f32,
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
    /// Components the chessboard stage produced for this snap.
    chess_components: usize,
    /// Worst component's self-consistency at the chessboard stage.
    chess_consistency: SelfConsistency,
    /// Self-consistency of the ChArUco corners actually emitted.
    final_consistency: SelfConsistency,
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
    /// Scale-free self-consistency of every emitted detection, for the
    /// distribution reported in the summary.
    final_residual_over_cell: Vec<f32>,
    /// Detections that emitted two labels at one image position.
    final_duplicate_position_frames: usize,
    /// Snaps whose chessboard stage bound one source corner to several lattice
    /// coordinates — a labelled set no homography admits.
    chess_reused_source_frames: usize,
    /// Snaps whose chessboard stage already collapsed two labels onto one pixel.
    chess_duplicate_position_frames: usize,
}

/// Percentile of an already-sorted slice, nearest-rank.
fn percentile(sorted: &[f32], q: f32) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((q * (sorted.len() - 1) as f32).round() as usize).min(sorted.len() - 1);
    sorted[idx]
}

impl Aggregate {
    fn record(&mut self, r: &CharucoFrameReport) {
        self.frames += 1;
        self.total_ms_sum += r.timings_ms.total_ms;
        if r.metrics.chess_consistency.reused_source_corners > 0 {
            self.chess_reused_source_frames += 1;
        }
        if r.metrics.chess_consistency.duplicate_position_pairs > 0 {
            self.chess_duplicate_position_frames += 1;
        }
        if r.detection.is_some() {
            self.detected += 1;
            self.markers_decoded_sum += r.metrics.markers_decoded;
            self.corners_sum += r.metrics.charuco_corners;
            self.raw_wrong_id_total += r.metrics.markers_wrong_id;
            if r.metrics.final_consistency.fitted {
                self.final_residual_over_cell
                    .push(r.metrics.final_consistency.median_over_cell);
            }
            if r.metrics.final_consistency.duplicate_position_pairs > 0 {
                self.final_duplicate_position_frames += 1;
            }
        }
    }

    fn finish(mut self, args: &Args, spec: &CharucoBoardSpec) -> SummaryReport {
        let frames = self.frames.max(1) as f32;
        let detected = self.detected.max(1) as f32;
        self.final_residual_over_cell
            .sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        SummaryReport {
            frames: self.frames,
            detected: self.detected,
            detection_rate_pct: 100.0 * self.detected as f32 / frames,
            markers_decoded_mean: self.markers_decoded_sum as f32 / detected,
            charuco_corners_mean: self.corners_sum as f32 / detected,
            raw_wrong_id_total: self.raw_wrong_id_total,
            runtime_mean_ms: self.total_ms_sum / frames,
            residual_over_cell_p50: percentile(&self.final_residual_over_cell, 0.50),
            residual_over_cell_p90: percentile(&self.final_residual_over_cell, 0.90),
            residual_over_cell_max: percentile(&self.final_residual_over_cell, 1.0),
            final_duplicate_position_frames: self.final_duplicate_position_frames,
            chess_reused_source_frames: self.chess_reused_source_frames,
            chess_duplicate_position_frames: self.chess_duplicate_position_frames,
            upscale: args.upscale,
            algorithm: algorithm_slug(),
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
    /// Distribution of the emitted detections' scale-free self-homography
    /// residual (`median residual / cell pitch`). A healthy population sits at
    /// corner-localisation scale — a couple of percent of the cell pitch.
    residual_over_cell_p50: f32,
    residual_over_cell_p90: f32,
    residual_over_cell_max: f32,
    /// Detections that emitted two labels at one image position.
    final_duplicate_position_frames: usize,
    /// Snaps whose chessboard stage bound one source corner to several coords.
    chess_reused_source_frames: usize,
    /// Snaps whose chessboard stage already collapsed two labels onto one pixel.
    chess_duplicate_position_frames: usize,
    upscale: u32,
    /// Grid-build algorithm slug (always `topological` — the sole builder).
    algorithm: &'static str,
    board: CharucoBoardSpec,
}
