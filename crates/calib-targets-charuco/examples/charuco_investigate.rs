use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Instant;

use calib_targets_charuco::{
    build_strip_acceptance, compute_strip_coverage, passes_spread_gate, split_composite_rects,
    spread_gate_limit, CharucoBoardSpec, CharucoDetectConfig, CharucoDetectError,
    CharucoDetectReport, CharucoDetector, CharucoDetectorParams, DatasetConfig, ImageCropRect,
    COMPOSITE_STRIP_COUNT, DEFAULT_MIN_CORNER_COUNT,
};
use calib_targets_core::{init_with_level, Corner, GrayImageView};
use chess_corners::{find_chess_corners_image, ChessConfig, CornerDescriptor};
use image::{imageops::crop_imm, GrayImage, ImageReader};
use log::{info, LevelFilter};
use nalgebra::Point2;
use serde::Serialize;

const MAIN_REPO_DATASET: &str = "/Users/vitalyvorobyev/vision/calib-targets-rs/testdata/3536119669";
const DOWNLOADS_DATASET: &str = "/Users/vitalyvorobyev/Downloads/3536119669";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let log_level = LevelFilter::from_str("info").unwrap_or(LevelFilter::Info);
    init_with_level(log_level)?;
    info!("Logger initialized");

    match parse_args(env::args().skip(1).collect())? {
        Command::Single(args) => run_single(args)?,
        Command::Perf(args) => run_perf(args)?,
        Command::Dataset(args) => run_dataset(args)?,
    }

    Ok(())
}

#[derive(Debug)]
enum Command {
    Single(SingleArgs),
    Perf(PerfArgs),
    Dataset(DatasetArgs),
}

#[derive(Debug, Clone)]
struct CommonArgs {
    input_dir: Option<PathBuf>,
    out_dir: Option<PathBuf>,
    min_marker_inliers: Option<usize>,
    multi_hypothesis_decode: bool,
    rectified_recovery: bool,
    global_corner_validation: bool,
    allow_low_inlier_unique_alignment: bool,
}

#[derive(Debug, Clone)]
struct SingleArgs {
    common: CommonArgs,
    image: String,
    strip: Option<usize>,
}

#[derive(Debug, Clone)]
struct PerfArgs {
    common: CommonArgs,
    image: String,
    strip: usize,
    repeat: usize,
}

#[derive(Debug, Clone)]
struct DatasetArgs {
    common: CommonArgs,
}

#[derive(Debug)]
struct InvestigationContext {
    dataset_dir: PathBuf,
    config_label: String,
    board: CharucoBoardSpec,
    detector: CharucoDetector,
    expected_strip_size: Option<(u32, u32)>,
}

#[derive(Clone, Debug, Serialize)]
struct StripSummary {
    image_name: String,
    image_path: String,
    strip_index: usize,
    crop_rect: ImageCropRect,
    report_path: String,
    input_image_path: String,
    raw_corner_count: usize,
    marker_count: usize,
    final_corner_count: usize,
    x_bin_counts: Vec<usize>,
    empty_bin_count: usize,
    min_bin_count: usize,
    y_min: Option<f32>,
    y_p10: Option<f32>,
    y_median: Option<f32>,
    y_p90: Option<f32>,
    y_max: Option<f32>,
    passes_corner_count: bool,
    passes_x_coverage: bool,
    passes_all: bool,
    raw_corner_ms: f64,
    chessboard_ms: f64,
    decode_ms: f64,
    alignment_ms: f64,
    map_validate_ms: f64,
    total_ms: f64,
    error: Option<String>,
    failure_stage: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct CompositeSummary {
    image_name: String,
    image_path: String,
    strip_count: usize,
    strip_corner_counts: Vec<usize>,
    spread_limit: Option<usize>,
    passes_spread_gate: Option<bool>,
    failing_strip_count: usize,
    failing_x_coverage_count: usize,
}

#[derive(Clone, Debug, Serialize)]
struct DatasetSummary {
    dataset_dir: String,
    total_composites: usize,
    total_strips: usize,
    successful_strips: usize,
    worst_strip_corner_count: usize,
    strips_failing_min_corner: usize,
    strips_failing_x_bin_coverage: usize,
    composites_failing_spread_gate: usize,
    composites: Vec<CompositeSummary>,
    strips: Vec<StripSummary>,
}

#[derive(Clone, Debug, Serialize)]
struct TimingStats {
    min_ms: f64,
    mean_ms: f64,
    p50_ms: f64,
    p95_ms: f64,
    max_ms: f64,
}

#[derive(Clone, Debug, Serialize)]
struct PerfSummary {
    image_name: String,
    strip_index: usize,
    repeat: usize,
    raw_corner_ms: TimingStats,
    chessboard_ms: TimingStats,
    decode_ms: TimingStats,
    alignment_ms: TimingStats,
    map_validate_ms: TimingStats,
    total_ms: TimingStats,
    final_corner_counts: Vec<usize>,
}

#[derive(Debug)]
struct StripRunArtifacts {
    summary: StripSummary,
}

#[derive(Debug)]
struct StripRunMeasure {
    raw_corner_ms: f64,
    chessboard_ms: f64,
    decode_ms: f64,
    alignment_ms: f64,
    map_validate_ms: f64,
    total_ms: f64,
    final_corner_count: usize,
}

fn parse_args(args: Vec<String>) -> Result<Command, Box<dyn std::error::Error>> {
    let Some(mode) = args.first().cloned() else {
        return Err("usage: charuco_investigate <single|perf|dataset> [options]".into());
    };

    let mut input_dir = None;
    let mut out_dir = None;
    let mut image = None;
    let mut strip = None;
    let mut repeat = 10usize;
    let mut min_marker_inliers = None;
    let mut multi_hypothesis_decode = false;
    let mut rectified_recovery = false;
    let mut global_corner_validation = false;
    let mut allow_low_inlier_unique_alignment = false;

    let mut idx = 1usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--input-dir" => {
                idx += 1;
                input_dir = Some(PathBuf::from(require_arg(&args, idx, "--input-dir")?));
            }
            "--out-dir" => {
                idx += 1;
                out_dir = Some(PathBuf::from(require_arg(&args, idx, "--out-dir")?));
            }
            "--image" => {
                idx += 1;
                image = Some(require_arg(&args, idx, "--image")?.to_string());
            }
            "--strip" => {
                idx += 1;
                strip = Some(
                    require_arg(&args, idx, "--strip")?
                        .parse::<usize>()
                        .map_err(|_| "invalid --strip")?,
                );
            }
            "--repeat" => {
                idx += 1;
                repeat = require_arg(&args, idx, "--repeat")?
                    .parse::<usize>()
                    .map_err(|_| "invalid --repeat")?;
            }
            "--min-marker-inliers" => {
                idx += 1;
                min_marker_inliers = Some(
                    require_arg(&args, idx, "--min-marker-inliers")?
                        .parse::<usize>()
                        .map_err(|_| "invalid --min-marker-inliers")?,
                );
            }
            "--multi-hypothesis-decode" => {
                multi_hypothesis_decode = true;
            }
            "--rectified-recovery" => {
                rectified_recovery = true;
            }
            "--global-corner-validation" => {
                global_corner_validation = true;
            }
            "--allow-low-inlier-unique-alignment" => {
                allow_low_inlier_unique_alignment = true;
            }
            unknown => return Err(format!("unknown argument {unknown}").into()),
        }
        idx += 1;
    }

    let common = CommonArgs {
        input_dir,
        out_dir,
        min_marker_inliers,
        multi_hypothesis_decode,
        rectified_recovery,
        global_corner_validation,
        allow_low_inlier_unique_alignment,
    };
    Ok(match mode.as_str() {
        "single" => Command::Single(SingleArgs {
            common,
            image: image.ok_or("--image is required for single mode")?,
            strip,
        }),
        "perf" => Command::Perf(PerfArgs {
            common,
            image: image.ok_or("--image is required for perf mode")?,
            strip: strip.ok_or("--strip is required for perf mode")?,
            repeat,
        }),
        "dataset" => Command::Dataset(DatasetArgs { common }),
        _ => return Err(format!("unknown mode {mode}").into()),
    })
}

fn require_arg<'a>(
    args: &'a [String],
    idx: usize,
    flag: &str,
) -> Result<&'a str, Box<dyn std::error::Error>> {
    args.get(idx)
        .map(String::as_str)
        .ok_or_else(|| format!("missing value for {flag}").into())
}

fn run_single(args: SingleArgs) -> Result<(), Box<dyn std::error::Error>> {
    let context = build_context(&args.common)?;
    let image_path = resolve_image_path(&context.dataset_dir, &args.image)?;
    let out_dir = args.common.out_dir.unwrap_or_else(|| {
        PathBuf::from("tmpdata/charuco_investigate").join(image_stem(&image_path))
    });
    let summary = process_image(
        &context,
        &image_path,
        &out_dir,
        args.strip
            .map(|idx| vec![idx])
            .unwrap_or_else(|| (0..COMPOSITE_STRIP_COUNT).collect()),
    )?;
    write_summary_files(&out_dir, &summary)?;
    println!("wrote summary to {}", out_dir.display());
    Ok(())
}

fn run_dataset(args: DatasetArgs) -> Result<(), Box<dyn std::error::Error>> {
    let context = build_context(&args.common)?;
    let out_dir = args
        .common
        .out_dir
        .unwrap_or_else(|| PathBuf::from("tmpdata/charuco_investigate").join("dataset"));
    fs::create_dir_all(&out_dir)?;

    let mut all_strips = Vec::new();
    let mut composites = Vec::new();
    for image_path in sorted_target_images(&context.dataset_dir)? {
        let image_out_dir = out_dir.join(image_stem(&image_path));
        let summary = process_image(
            &context,
            &image_path,
            &image_out_dir,
            (0..COMPOSITE_STRIP_COUNT).collect(),
        )?;
        composites.extend(summary.composites);
        all_strips.extend(summary.strips);
    }

    let summary = build_dataset_summary(&context.dataset_dir, composites, all_strips);
    write_summary_files(&out_dir, &summary)?;
    println!("wrote summary to {}", out_dir.display());
    Ok(())
}

fn run_perf(args: PerfArgs) -> Result<(), Box<dyn std::error::Error>> {
    let context = build_context(&args.common)?;
    let image_path = resolve_image_path(&context.dataset_dir, &args.image)?;
    let out_dir = args.common.out_dir.unwrap_or_else(|| {
        PathBuf::from("tmpdata/charuco_investigate")
            .join(format!("{}_perf", image_stem(&image_path)))
    });
    fs::create_dir_all(&out_dir)?;

    let composite = load_image(&image_path)?;
    let rects = split_composite_rects(
        composite.width(),
        composite.height(),
        context.expected_strip_size,
    )?;
    let crop_rect = *rects
        .get(args.strip)
        .ok_or_else(|| format!("strip {} out of range", args.strip))?;
    let strip = crop_imm(
        &composite,
        crop_rect.x,
        crop_rect.y,
        crop_rect.width,
        crop_rect.height,
    )
    .to_image();

    let _warmup = measure_strip(&context, &image_path, args.strip, crop_rect, &strip)?;
    let mut measures = Vec::with_capacity(args.repeat);
    for repeat_idx in 0..args.repeat {
        if repeat_idx == 0 {
            let artifacts = process_strip(
                &context,
                &image_path,
                args.strip,
                crop_rect,
                &strip,
                &out_dir.join(format!("strip_{}", args.strip)),
            )?;
            measures.push(StripRunMeasure {
                raw_corner_ms: artifacts.summary.raw_corner_ms,
                chessboard_ms: artifacts.summary.chessboard_ms,
                decode_ms: artifacts.summary.decode_ms,
                alignment_ms: artifacts.summary.alignment_ms,
                map_validate_ms: artifacts.summary.map_validate_ms,
                total_ms: artifacts.summary.total_ms,
                final_corner_count: artifacts.summary.final_corner_count,
            });
        } else {
            measures.push(measure_strip(
                &context,
                &image_path,
                args.strip,
                crop_rect,
                &strip,
            )?);
        }
    }

    let perf_summary = PerfSummary {
        image_name: image_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown")
            .to_string(),
        strip_index: args.strip,
        repeat: args.repeat,
        raw_corner_ms: summarize_timing(measures.iter().map(|m| m.raw_corner_ms).collect()),
        chessboard_ms: summarize_timing(measures.iter().map(|m| m.chessboard_ms).collect()),
        decode_ms: summarize_timing(measures.iter().map(|m| m.decode_ms).collect()),
        alignment_ms: summarize_timing(measures.iter().map(|m| m.alignment_ms).collect()),
        map_validate_ms: summarize_timing(measures.iter().map(|m| m.map_validate_ms).collect()),
        total_ms: summarize_timing(measures.iter().map(|m| m.total_ms).collect()),
        final_corner_counts: measures.iter().map(|m| m.final_corner_count).collect(),
    };

    let perf_path = out_dir.join("perf_summary.json");
    fs::write(&perf_path, serde_json::to_string_pretty(&perf_summary)?)?;
    println!("wrote perf summary to {}", perf_path.display());
    Ok(())
}

fn build_context(common: &CommonArgs) -> Result<InvestigationContext, Box<dyn std::error::Error>> {
    let dataset_dir = resolve_dataset_dir(common.input_dir.clone())?;
    let config_path = find_dataset_config(&dataset_dir);
    let (board, expected_strip_size, config_label) = if let Some(path) = &config_path {
        let cfg = DatasetConfig::load_json(path)?;
        (
            cfg.board_spec()?,
            Some(cfg.strip_size()?),
            path.display().to_string(),
        )
    } else {
        let cfg = CharucoDetectConfig::load_json("testdata/charuco_detect_config.json")?;
        (cfg.board, None, "builtin_defaults".to_string())
    };

    let mut params = CharucoDetectorParams::for_board(&board);
    params.px_per_square = 60.0;
    if let Some(min_marker_inliers) = common.min_marker_inliers {
        params.min_marker_inliers = min_marker_inliers;
    }
    params.augmentation.multi_hypothesis_decode = common.multi_hypothesis_decode;
    params.augmentation.rectified_recovery = common.rectified_recovery;
    params.use_global_corner_validation = common.global_corner_validation;
    params.allow_low_inlier_unique_alignment = common.allow_low_inlier_unique_alignment;
    let detector = CharucoDetector::new(params)?;

    Ok(InvestigationContext {
        dataset_dir,
        config_label,
        board,
        detector,
        expected_strip_size,
    })
}

fn resolve_dataset_dir(input_dir: Option<PathBuf>) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(path) = input_dir {
        return Ok(path);
    }

    let cwd = env::current_dir()?;
    let candidates = [
        cwd.join("testdata/3536119669"),
        PathBuf::from(MAIN_REPO_DATASET),
        PathBuf::from(DOWNLOADS_DATASET),
    ];
    for path in candidates {
        if path.exists() {
            return Ok(path);
        }
    }
    Err("could not resolve dataset directory".into())
}

fn find_dataset_config(dataset_dir: &Path) -> Option<PathBuf> {
    let path = dataset_dir.join("config.json");
    path.exists().then_some(path)
}

fn resolve_image_path(
    dataset_dir: &Path,
    image_arg: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let explicit = PathBuf::from(image_arg);
    if explicit.exists() {
        return Ok(explicit);
    }
    let candidate = dataset_dir.join(image_arg);
    if candidate.exists() {
        return Ok(candidate);
    }
    Err(format!("image not found: {image_arg}").into())
}

fn sorted_target_images(dataset_dir: &Path) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let mut images: Vec<PathBuf> = fs::read_dir(dataset_dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().and_then(|ext| ext.to_str()) == Some("png")
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name.starts_with("target_"))
                    .unwrap_or(false)
        })
        .collect();
    images.sort();
    Ok(images)
}

fn process_image(
    context: &InvestigationContext,
    image_path: &Path,
    out_dir: &Path,
    strips: Vec<usize>,
) -> Result<DatasetSummary, Box<dyn std::error::Error>> {
    fs::create_dir_all(out_dir)?;
    let composite = load_image(image_path)?;
    let rects = split_composite_rects(
        composite.width(),
        composite.height(),
        context.expected_strip_size,
    )?;

    let mut strip_summaries = Vec::new();
    for strip_index in strips {
        let crop_rect = *rects
            .get(strip_index)
            .ok_or_else(|| format!("strip {} out of range", strip_index))?;
        let crop = crop_imm(
            &composite,
            crop_rect.x,
            crop_rect.y,
            crop_rect.width,
            crop_rect.height,
        )
        .to_image();
        let strip_dir = out_dir.join(format!("strip_{}", strip_index));
        let artifacts = process_strip(
            context,
            image_path,
            strip_index,
            crop_rect,
            &crop,
            &strip_dir,
        )?;
        strip_summaries.push(artifacts.summary);
    }

    let composite_summary = build_composite_summary(image_path, &strip_summaries);
    Ok(build_dataset_summary(
        image_path.parent().unwrap_or_else(|| Path::new("")),
        vec![composite_summary],
        strip_summaries,
    ))
}

fn process_strip(
    context: &InvestigationContext,
    composite_path: &Path,
    strip_index: usize,
    crop_rect: ImageCropRect,
    crop: &GrayImage,
    out_dir: &Path,
) -> Result<StripRunArtifacts, Box<dyn std::error::Error>> {
    fs::create_dir_all(out_dir)?;
    let input_path = out_dir.join("input.png");
    crop.save(&input_path)?;

    let raw_start = Instant::now();
    let raw_corners = detect_raw_corners(crop);
    let raw_corner_ms = elapsed_ms(raw_start);
    let target_corners = adapt_corners(&raw_corners);
    let run = context
        .detector
        .detect_with_diagnostics(&make_view(crop), &target_corners);

    let (marker_count, final_corner_count, error, failure_stage) = match &run.result {
        Ok(res) => (res.markers.len(), res.detection.corners.len(), None, None),
        Err(err) => (
            0,
            0,
            Some(err.to_string()),
            Some(failure_stage(err).to_string()),
        ),
    };

    let detection_corners = match &run.result {
        Ok(res) => res.detection.corners.as_slice(),
        Err(_) => &[],
    };
    let coverage = compute_strip_coverage(detection_corners, crop.width());
    let acceptance =
        build_strip_acceptance(final_corner_count, &coverage, DEFAULT_MIN_CORNER_COUNT);

    let mut report = CharucoDetectReport::new_with_context(
        input_path.display().to_string(),
        context.config_label.clone(),
        context.board,
        target_corners,
    );
    report.source_image_path = Some(composite_path.display().to_string());
    report.strip_index = Some(strip_index);
    report.crop_rect = Some(crop_rect);
    report.set_detection_run(run);
    if let Some(diagnostics) = report.diagnostics.as_mut() {
        diagnostics.coverage = Some(coverage.clone());
        diagnostics.acceptance = Some(acceptance.clone());
    }

    let report_path = out_dir.join("report.json");
    report.write_json(&report_path)?;

    let det_diag = report
        .diagnostics
        .as_ref()
        .map(|d| &d.detection)
        .expect("detection diagnostics");

    Ok(StripRunArtifacts {
        summary: StripSummary {
            image_name: composite_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("unknown")
                .to_string(),
            image_path: composite_path.display().to_string(),
            strip_index,
            crop_rect,
            report_path: report_path.display().to_string(),
            input_image_path: input_path.display().to_string(),
            raw_corner_count: raw_corners.len(),
            marker_count,
            final_corner_count,
            x_bin_counts: coverage.x_bin_counts.clone(),
            empty_bin_count: coverage.empty_bin_count,
            min_bin_count: coverage.min_bin_count,
            y_min: coverage.y_min,
            y_p10: coverage.y_p10,
            y_median: coverage.y_median,
            y_p90: coverage.y_p90,
            y_max: coverage.y_max,
            passes_corner_count: acceptance.passes_corner_count,
            passes_x_coverage: acceptance.passes_x_coverage,
            passes_all: acceptance.passes_all,
            raw_corner_ms,
            chessboard_ms: det_diag.timings.chessboard_ms,
            decode_ms: det_diag.timings.decode_ms,
            alignment_ms: det_diag.timings.alignment_ms,
            map_validate_ms: det_diag.timings.map_validate_ms,
            total_ms: raw_corner_ms + det_diag.timings.total_ms,
            error,
            failure_stage,
        },
    })
}

fn measure_strip(
    context: &InvestigationContext,
    composite_path: &Path,
    strip_index: usize,
    crop_rect: ImageCropRect,
    crop: &GrayImage,
) -> Result<StripRunMeasure, Box<dyn std::error::Error>> {
    let raw_start = Instant::now();
    let raw_corners = detect_raw_corners(crop);
    let raw_corner_ms = elapsed_ms(raw_start);
    let target_corners = adapt_corners(&raw_corners);
    let run = context
        .detector
        .detect_with_diagnostics(&make_view(crop), &target_corners);
    let final_corner_count = match &run.result {
        Ok(res) => res.detection.corners.len(),
        Err(_) => 0,
    };

    let _ = (composite_path, strip_index, crop_rect);
    Ok(StripRunMeasure {
        raw_corner_ms,
        chessboard_ms: run.diagnostics.timings.chessboard_ms,
        decode_ms: run.diagnostics.timings.decode_ms,
        alignment_ms: run.diagnostics.timings.alignment_ms,
        map_validate_ms: run.diagnostics.timings.map_validate_ms,
        total_ms: raw_corner_ms + run.diagnostics.timings.total_ms,
        final_corner_count,
    })
}

fn build_dataset_summary(
    dataset_dir: &Path,
    composites: Vec<CompositeSummary>,
    strips: Vec<StripSummary>,
) -> DatasetSummary {
    let successful_strips = strips.iter().filter(|strip| strip.error.is_none()).count();
    let worst_strip_corner_count = strips
        .iter()
        .map(|strip| strip.final_corner_count)
        .min()
        .unwrap_or(0);
    let strips_failing_min_corner = strips
        .iter()
        .filter(|strip| !strip.passes_corner_count)
        .count();
    let strips_failing_x_bin_coverage = strips
        .iter()
        .filter(|strip| !strip.passes_x_coverage)
        .count();
    let composites_failing_spread_gate = composites
        .iter()
        .filter(|summary| summary.passes_spread_gate == Some(false))
        .count();

    DatasetSummary {
        dataset_dir: dataset_dir.display().to_string(),
        total_composites: composites.len(),
        total_strips: strips.len(),
        successful_strips,
        worst_strip_corner_count,
        strips_failing_min_corner,
        strips_failing_x_bin_coverage,
        composites_failing_spread_gate,
        composites,
        strips,
    }
}

fn build_composite_summary(image_path: &Path, strips: &[StripSummary]) -> CompositeSummary {
    let counts: Vec<usize> = strips
        .iter()
        .map(|strip| strip.final_corner_count)
        .collect();
    let spread_limit = if strips.len() == COMPOSITE_STRIP_COUNT {
        spread_gate_limit(&counts)
    } else {
        None
    };
    let passes = if strips.len() == COMPOSITE_STRIP_COUNT {
        Some(passes_spread_gate(&counts))
    } else {
        None
    };

    CompositeSummary {
        image_name: image_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown")
            .to_string(),
        image_path: image_path.display().to_string(),
        strip_count: strips.len(),
        strip_corner_counts: counts,
        spread_limit,
        passes_spread_gate: passes,
        failing_strip_count: strips.iter().filter(|strip| !strip.passes_all).count(),
        failing_x_coverage_count: strips
            .iter()
            .filter(|strip| !strip.passes_x_coverage)
            .count(),
    }
}

fn write_summary_files(
    out_dir: &Path,
    summary: &DatasetSummary,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(out_dir)?;
    let json_path = out_dir.join("summary.json");
    fs::write(&json_path, serde_json::to_string_pretty(summary)?)?;

    let csv_path = out_dir.join("summary.csv");
    let mut file = File::create(&csv_path)?;
    writeln!(
        file,
        "image_name,strip_index,raw_corner_count,marker_count,final_corner_count,x_bin_counts,empty_bin_count,min_bin_count,passes_corner_count,passes_x_coverage,passes_all,raw_corner_ms,chessboard_ms,decode_ms,alignment_ms,map_validate_ms,total_ms,error,failure_stage,report_path"
    )?;
    for strip in &summary.strips {
        writeln!(
            file,
            "{},{},{},{},{},\"{}\",{},{},{},{},{},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{},{},{}",
            csv_escape(&strip.image_name),
            strip.strip_index,
            strip.raw_corner_count,
            strip.marker_count,
            strip.final_corner_count,
            strip
                .x_bin_counts
                .iter()
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
                .join("|"),
            strip.empty_bin_count,
            strip.min_bin_count,
            strip.passes_corner_count,
            strip.passes_x_coverage,
            strip.passes_all,
            strip.raw_corner_ms,
            strip.chessboard_ms,
            strip.decode_ms,
            strip.alignment_ms,
            strip.map_validate_ms,
            strip.total_ms,
            csv_escape_opt(strip.error.as_deref()),
            csv_escape_opt(strip.failure_stage.as_deref()),
            csv_escape(&strip.report_path),
        )?;
    }
    Ok(())
}

fn summarize_timing(mut values: Vec<f64>) -> TimingStats {
    values.sort_by(|a, b| a.total_cmp(b));
    let min_ms = *values.first().unwrap_or(&0.0);
    let max_ms = *values.last().unwrap_or(&0.0);
    let mean_ms = if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    };
    TimingStats {
        min_ms,
        mean_ms,
        p50_ms: percentile(&values, 0.50),
        p95_ms: percentile(&values, 0.95),
        max_ms,
    }
}

fn percentile(values: &[f64], q: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let q = q.clamp(0.0, 1.0);
    let pos = q * (values.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    if lo == hi {
        return values[lo];
    }
    let w = pos - lo as f64;
    values[lo] * (1.0 - w) + values[hi] * w
}

fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn csv_escape_opt(value: Option<&str>) -> String {
    value.map(csv_escape).unwrap_or_default()
}

fn load_image(path: &Path) -> Result<GrayImage, Box<dyn std::error::Error>> {
    Ok(ImageReader::open(path)?.decode()?.to_luma8())
}

fn detect_raw_corners(img: &GrayImage) -> Vec<CornerDescriptor> {
    let mut chess_cfg = ChessConfig::single_scale();
    chess_cfg.params.threshold_rel = 0.2;
    chess_cfg.params.nms_radius = 2;
    find_chess_corners_image(img, &chess_cfg)
}

fn adapt_corners(raw: &[CornerDescriptor]) -> Vec<Corner> {
    raw.iter().map(adapt_chess_corner).collect()
}

fn make_view(img: &GrayImage) -> GrayImageView<'_> {
    GrayImageView {
        width: img.width() as usize,
        height: img.height() as usize,
        data: img.as_raw(),
    }
}

fn adapt_chess_corner(c: &CornerDescriptor) -> Corner {
    Corner {
        position: Point2::new(c.x, c.y),
        orientation: c.orientation,
        orientation_cluster: None,
        strength: c.response,
    }
}

fn failure_stage(err: &CharucoDetectError) -> &'static str {
    match err {
        CharucoDetectError::ChessboardNotDetected => "chessboard",
        CharucoDetectError::NoMarkers => "decode",
        CharucoDetectError::AlignmentFailed { .. } => "alignment",
        CharucoDetectError::MeshWarp(_) => "mesh_warp",
    }
}

fn image_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("image")
        .to_string()
}

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1_000.0
}
