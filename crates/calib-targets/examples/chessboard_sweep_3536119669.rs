//! Chessboard-only visible-grid sweep over the 3536119669 test set.
//!
//! Each `testdata/3536119669/target_N.png` is 4320x540: six 720x540 snaps of
//! the same 22x22 ChArUco board, concatenated horizontally. This binary splits
//! each image into its six sub-frames, runs only ChESS + chessboard grid
//! reconstruction, and reports a visible-subset quality gate. It intentionally
//! ignores ArUco marker decoding.
//!
//! Usage:
//! ```text
//! cargo run --release -p calib-targets --example chessboard_sweep_3536119669 -- \
//!     --output bench_results/chessboard_3536119669/<tag>.json
//! ```

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use calib_targets::chessboard::{
    score_frame, ChessboardDetector, ChessboardParams, GridFrameMetrics, GridGraphParams,
    VISIBLE_SUBSET_GATE_3536119669,
};
use calib_targets::detect::{
    detect_corners, ChessConfig, DescriptorMode, DetectorMode, ThresholdMode, UpscaleConfig,
};
use image::{GenericImageView, GrayImage};
use serde::Serialize;

const SCHEMA_VERSION: u32 = 2;
const EXPECTED_ROWS: u32 = 21;
const EXPECTED_COLS: u32 = 21;
const EXPECTED_INTERIOR: u32 = EXPECTED_ROWS * EXPECTED_COLS;
const SNAP_WIDTH: u32 = 720;
const SNAP_HEIGHT: u32 = 540;
const SNAPS_PER_IMAGE: u32 = 6;

#[derive(Clone)]
struct SweepConfig {
    name: &'static str,
    chess: ChessConfig,
}

#[derive(Clone, Serialize)]
struct MetricsReport {
    corner_count: usize,
    extent_i: u32,
    extent_j: u32,
    residual_median_px: Option<f32>,
    residual_p95_px: Option<f32>,
}

impl From<GridFrameMetrics> for MetricsReport {
    fn from(metrics: GridFrameMetrics) -> Self {
        Self {
            corner_count: metrics.corner_count,
            extent_i: metrics.extent_i,
            extent_j: metrics.extent_j,
            residual_median_px: metrics.residual_median_px,
            residual_p95_px: metrics.residual_p95_px,
        }
    }
}

#[derive(Clone, Serialize)]
struct CandidateReport {
    config_name: String,
    orientation_clustering: bool,
    raw_chess_corners: usize,
    valid_visible_subset: bool,
    metrics: MetricsReport,
}

#[derive(Serialize)]
struct StrictFullBoardReport {
    detected: bool,
    raw_chess_corners: usize,
    metrics: MetricsReport,
}

#[derive(Serialize)]
struct FrameReport {
    target_index: u32,
    snap_index: u32,
    width: u32,
    height: u32,
    strict_full_board: StrictFullBoardReport,
    selected_config_name: Option<String>,
    selected_orientation_clustering: Option<bool>,
    valid_visible_subset: bool,
    selected: Option<CandidateReport>,
    timing_total_us: u128,
}

#[derive(Serialize)]
struct RawConfigStats {
    config_name: String,
    min_corners: usize,
    median_corners: usize,
    p95_corners: usize,
    max_corners: usize,
    mean_corners: f32,
}

#[derive(Serialize)]
struct Aggregate {
    n_frames: usize,
    valid_visible_subset_frames: usize,
    visible_subset_rate_pct: f32,
    strict_full_board_detected_frames: usize,
    median_selected_corner_count: Option<usize>,
    p95_selected_corner_count: Option<usize>,
    median_valid_corner_count: Option<usize>,
    p95_valid_corner_count: Option<usize>,
    median_selected_residual_median_px: Option<f32>,
    median_selected_residual_p95_px: Option<f32>,
    median_timing_total_us: u128,
    p95_timing_total_us: u128,
    raw_chess_candidate_stats_by_config: Vec<RawConfigStats>,
}

#[derive(Serialize)]
struct VisibleSubsetGateReport {
    min_corners: usize,
    min_extent_i: u32,
    min_extent_j: u32,
    max_residual_median_px: f32,
    max_residual_p95_px: f32,
}

#[derive(Serialize)]
struct Report {
    schema_version: u32,
    git_sha: String,
    expected_interior_corners: u32,
    expected_rows: u32,
    expected_cols: u32,
    visible_subset_gate: VisibleSubsetGateReport,
    frames: Vec<FrameReport>,
    aggregate: Aggregate,
}

fn git_sha() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".to_string())
}

fn testdata_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/3536119669")
}

fn list_target_images(dir: &Path) -> Vec<(u32, PathBuf)> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        eprintln!("cannot read {}", dir.display());
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if !name.starts_with("target_") || !name.ends_with(".png") {
            continue;
        }
        if let Ok(idx) = name
            .trim_start_matches("target_")
            .trim_end_matches(".png")
            .parse::<u32>()
        {
            out.push((idx, path));
        }
    }
    out.sort_by_key(|(i, _)| *i);
    out
}

fn slice_snap(img: &GrayImage, snap: u32) -> Option<GrayImage> {
    let x0 = snap * SNAP_WIDTH;
    let x1 = x0 + SNAP_WIDTH;
    if x1 > img.width() || SNAP_HEIGHT > img.height() {
        return None;
    }
    Some(img.view(x0, 0, SNAP_WIDTH, SNAP_HEIGHT).to_image())
}

fn relative_chess_config(threshold: f32) -> ChessConfig {
    ChessConfig {
        threshold_mode: ThresholdMode::Relative,
        threshold_value: threshold,
        nms_radius: 2,
        ..ChessConfig::single_scale()
    }
}

fn sweep_configs() -> Vec<SweepConfig> {
    let mut configs = Vec::new();
    for (name, threshold) in [
        ("native_rel020", 0.20),
        ("native_rel015", 0.15),
        ("native_rel008", 0.08),
    ] {
        configs.push(SweepConfig {
            name,
            chess: relative_chess_config(threshold),
        });
    }

    for (name, threshold) in [
        ("up2_rel020", 0.20),
        ("up2_rel015", 0.15),
        ("up2_rel008", 0.08),
    ] {
        let mut chess = relative_chess_config(threshold);
        chess.upscale = UpscaleConfig::fixed(2);
        configs.push(SweepConfig { name, chess });
    }

    let mut broad = relative_chess_config(0.20);
    broad.upscale = UpscaleConfig::fixed(2);
    broad.detector_mode = DetectorMode::Broad;
    broad.descriptor_mode = DescriptorMode::FollowDetector;
    configs.push(SweepConfig {
        name: "up2_broad_rel020",
        chess: broad,
    });

    let mut up3 = relative_chess_config(0.20);
    up3.upscale = UpscaleConfig::fixed(3);
    configs.push(SweepConfig {
        name: "up3_rel020",
        chess: up3,
    });

    configs
}

fn graph_params() -> GridGraphParams {
    GridGraphParams {
        min_spacing_pix: 8.0,
        max_spacing_pix: 60.0,
        ..GridGraphParams::default()
    }
}

fn strict_full_board_params() -> ChessboardParams {
    ChessboardParams {
        expected_rows: Some(EXPECTED_ROWS),
        expected_cols: Some(EXPECTED_COLS),
        graph: graph_params(),
        ..ChessboardParams::default()
    }
}

fn partial_board_params(use_orientation_clustering: bool) -> ChessboardParams {
    ChessboardParams {
        expected_rows: None,
        expected_cols: None,
        use_orientation_clustering,
        graph: graph_params(),
        ..ChessboardParams::default()
    }
}

/// Phase 3 two-axis + step-consistency validator. Keeps the k-NN window wide
/// enough to sample board cardinals through any marker-internal clutter and
/// uses an absolute step fallback tuned for 720×540 ChArUco snaps.
fn partial_board_params_two_axis() -> ChessboardParams {
    use calib_targets::chessboard::ChessboardGraphMode;
    ChessboardParams {
        expected_rows: None,
        expected_cols: None,
        use_orientation_clustering: false,
        graph: GridGraphParams {
            mode: ChessboardGraphMode::TwoAxis,
            k_neighbors: 24,
            min_step_rel: 0.7,
            max_step_rel: 1.3,
            angular_tol_deg: 12.0,
            step_fallback_pix: 24.0,
            ..graph_params()
        },
        ..ChessboardParams::default()
    }
}

fn run_strict_full_board(img: &GrayImage) -> StrictFullBoardReport {
    let params = strict_full_board_params();
    let corners = detect_corners(img, &params.chess);
    let raw_chess_corners = corners.len();
    let detector = ChessboardDetector::new(params);
    let result = detector.detect_from_corners(&corners);
    let metrics = result
        .as_ref()
        .map(|r| score_frame(&r.detection, EXPECTED_ROWS, EXPECTED_COLS))
        .unwrap_or_default();
    StrictFullBoardReport {
        detected: result.is_some(),
        raw_chess_corners,
        metrics: metrics.into(),
    }
}

fn candidate_from_metrics(
    config_name: &str,
    orientation_clustering: bool,
    raw_chess_corners: usize,
    metrics: GridFrameMetrics,
) -> CandidateReport {
    CandidateReport {
        config_name: config_name.to_string(),
        orientation_clustering,
        raw_chess_corners,
        valid_visible_subset: metrics.passes_visible_subset(VISIBLE_SUBSET_GATE_3536119669),
        metrics: metrics.into(),
    }
}

fn empty_metrics() -> GridFrameMetrics {
    GridFrameMetrics::default()
}

fn residual_p95_for_sort(candidate: &CandidateReport) -> f32 {
    candidate.metrics.residual_p95_px.unwrap_or(f32::INFINITY)
}

fn is_better_valid(candidate: &CandidateReport, current: &CandidateReport) -> bool {
    candidate.metrics.corner_count > current.metrics.corner_count
        || (candidate.metrics.corner_count == current.metrics.corner_count
            && residual_p95_for_sort(candidate) < residual_p95_for_sort(current))
}

fn is_better_diagnostic(candidate: &CandidateReport, current: &CandidateReport) -> bool {
    candidate.metrics.corner_count > current.metrics.corner_count
        || (candidate.metrics.corner_count == current.metrics.corner_count
            && residual_p95_for_sort(candidate) < residual_p95_for_sort(current))
}

fn run_visible_subset_sweep(
    img: &GrayImage,
    configs: &[SweepConfig],
    raw_counts_by_config: &mut [Vec<usize>],
) -> Option<CandidateReport> {
    let mut best_valid: Option<CandidateReport> = None;
    let mut best_diagnostic: Option<CandidateReport> = None;
    let mut best_raw_diagnostic: Option<CandidateReport> = None;

    for (config_idx, config) in configs.iter().enumerate() {
        let corners = detect_corners(img, &config.chess);
        let raw_chess_corners = corners.len();
        raw_counts_by_config[config_idx].push(raw_chess_corners);

        let raw_candidate =
            candidate_from_metrics(config.name, false, raw_chess_corners, empty_metrics());
        if best_raw_diagnostic
            .as_ref()
            .map(|current| raw_candidate.raw_chess_corners > current.raw_chess_corners)
            .unwrap_or(true)
        {
            best_raw_diagnostic = Some(raw_candidate);
        }

        // Phase 3 two-axis validator + legacy Simple/Cluster validators.
        let mut param_variants: Vec<(bool, ChessboardParams, &'static str)> = Vec::new();
        param_variants.push((false, partial_board_params_two_axis(), "two_axis"));
        for use_orientation_clustering in [true, false] {
            param_variants.push((
                use_orientation_clustering,
                partial_board_params(use_orientation_clustering),
                if use_orientation_clustering {
                    "legacy_cluster"
                } else {
                    "legacy_simple"
                },
            ));
        }

        for (clustering, params, variant_tag) in param_variants {
            let config_tag = format!("{}::{}", config.name, variant_tag);
            let detector = ChessboardDetector::new(params);
            for result in detector.detect_all_from_corners(&corners) {
                let metrics = score_frame(&result.detection, EXPECTED_ROWS, EXPECTED_COLS);
                let candidate = candidate_from_metrics(
                    // Store the variant tag with the config name so the
                    // report shows which validator won each frame.
                    Box::leak(config_tag.clone().into_boxed_str()),
                    clustering,
                    raw_chess_corners,
                    metrics,
                );

                if candidate.valid_visible_subset
                    && best_valid
                        .as_ref()
                        .map(|current| is_better_valid(&candidate, current))
                        .unwrap_or(true)
                {
                    best_valid = Some(candidate.clone());
                }

                if best_diagnostic
                    .as_ref()
                    .map(|current| is_better_diagnostic(&candidate, current))
                    .unwrap_or(true)
                {
                    best_diagnostic = Some(candidate);
                }
            }
        }
    }

    best_valid.or(best_diagnostic).or(best_raw_diagnostic)
}

fn parse_output_path(args: &[String]) -> Option<PathBuf> {
    let mut iter = args.iter().skip(1);
    while let Some(a) = iter.next() {
        if a == "--output" || a == "-o" {
            return iter.next().map(PathBuf::from);
        }
    }
    None
}

fn percentile_usize(sorted: &[usize], q: f32) -> Option<usize> {
    if sorted.is_empty() {
        return None;
    }
    let idx = ((sorted.len() as f32 - 1.0) * q).round() as usize;
    Some(sorted[idx.min(sorted.len() - 1)])
}

fn percentile_u128(sorted: &[u128], q: f32) -> u128 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((sorted.len() as f32 - 1.0) * q).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn percentile_f32(sorted: &[f32], q: f32) -> Option<f32> {
    if sorted.is_empty() {
        return None;
    }
    let idx = ((sorted.len() as f32 - 1.0) * q).round() as usize;
    Some(sorted[idx.min(sorted.len() - 1)])
}

fn raw_config_stats(configs: &[SweepConfig], raw_counts: &[Vec<usize>]) -> Vec<RawConfigStats> {
    configs
        .iter()
        .zip(raw_counts.iter())
        .map(|(config, counts)| {
            let mut sorted = counts.clone();
            sorted.sort_unstable();
            let mean = if sorted.is_empty() {
                0.0
            } else {
                sorted.iter().sum::<usize>() as f32 / sorted.len() as f32
            };
            RawConfigStats {
                config_name: config.name.to_string(),
                min_corners: sorted.first().copied().unwrap_or(0),
                median_corners: percentile_usize(&sorted, 0.5).unwrap_or(0),
                p95_corners: percentile_usize(&sorted, 0.95).unwrap_or(0),
                max_corners: sorted.last().copied().unwrap_or(0),
                mean_corners: mean,
            }
        })
        .collect()
}

fn aggregate_frames(
    frames: &[FrameReport],
    configs: &[SweepConfig],
    raw_counts_by_config: &[Vec<usize>],
) -> Aggregate {
    let n_frames = frames.len();
    let valid_visible_subset_frames = frames.iter().filter(|f| f.valid_visible_subset).count();
    let strict_full_board_detected_frames = frames
        .iter()
        .filter(|f| f.strict_full_board.detected)
        .count();

    let mut selected_counts: Vec<usize> = frames
        .iter()
        .filter_map(|f| f.selected.as_ref().map(|s| s.metrics.corner_count))
        .collect();
    selected_counts.sort_unstable();

    let mut valid_counts: Vec<usize> = frames
        .iter()
        .filter_map(|f| {
            f.selected.as_ref().and_then(|s| {
                if s.valid_visible_subset {
                    Some(s.metrics.corner_count)
                } else {
                    None
                }
            })
        })
        .collect();
    valid_counts.sort_unstable();

    let mut residual_medians: Vec<f32> = frames
        .iter()
        .filter_map(|f| {
            f.selected.as_ref().and_then(|s| {
                if s.valid_visible_subset {
                    s.metrics.residual_median_px
                } else {
                    None
                }
            })
        })
        .collect();
    residual_medians.sort_by(|a, b| a.total_cmp(b));

    let mut residual_p95s: Vec<f32> = frames
        .iter()
        .filter_map(|f| {
            f.selected.as_ref().and_then(|s| {
                if s.valid_visible_subset {
                    s.metrics.residual_p95_px
                } else {
                    None
                }
            })
        })
        .collect();
    residual_p95s.sort_by(|a, b| a.total_cmp(b));

    let mut timings: Vec<u128> = frames.iter().map(|f| f.timing_total_us).collect();
    timings.sort_unstable();

    Aggregate {
        n_frames,
        valid_visible_subset_frames,
        visible_subset_rate_pct: if n_frames == 0 {
            0.0
        } else {
            100.0 * valid_visible_subset_frames as f32 / n_frames as f32
        },
        strict_full_board_detected_frames,
        median_selected_corner_count: percentile_usize(&selected_counts, 0.5),
        p95_selected_corner_count: percentile_usize(&selected_counts, 0.95),
        median_valid_corner_count: percentile_usize(&valid_counts, 0.5),
        p95_valid_corner_count: percentile_usize(&valid_counts, 0.95),
        median_selected_residual_median_px: percentile_f32(&residual_medians, 0.5),
        median_selected_residual_p95_px: percentile_f32(&residual_p95s, 0.5),
        median_timing_total_us: percentile_u128(&timings, 0.5),
        p95_timing_total_us: percentile_u128(&timings, 0.95),
        raw_chess_candidate_stats_by_config: raw_config_stats(configs, raw_counts_by_config),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let output_path = parse_output_path(&args);

    let dir = testdata_dir();
    let images = list_target_images(&dir);
    if images.is_empty() {
        eprintln!("no target_*.png files found in {}", dir.display());
        std::process::exit(1);
    }

    let configs = sweep_configs();
    let mut raw_counts_by_config: Vec<Vec<usize>> = configs.iter().map(|_| Vec::new()).collect();
    let mut frames = Vec::with_capacity(images.len() * SNAPS_PER_IMAGE as usize);

    println!(
        "| tgt | snap | strict | valid | selected config | raw | corners | extent | med res | p95 res | total ms |"
    );
    println!(
        "|-----|------|--------|-------|-----------------|-----|---------|--------|---------|---------|----------|"
    );

    for (tgt_idx, path) in &images {
        let img = match image::open(path) {
            Ok(i) => i.to_luma8(),
            Err(e) => {
                eprintln!("skip {}: {e}", path.display());
                continue;
            }
        };
        if img.height() != SNAP_HEIGHT {
            eprintln!(
                "{}: height {} != expected {}",
                path.display(),
                img.height(),
                SNAP_HEIGHT,
            );
            continue;
        }

        for snap in 0..SNAPS_PER_IMAGE {
            let Some(sub) = slice_snap(&img, snap) else {
                continue;
            };
            let t_frame = Instant::now();
            let strict_full_board = run_strict_full_board(&sub);
            let selected = run_visible_subset_sweep(&sub, &configs, &mut raw_counts_by_config);
            let timing_total_us = t_frame.elapsed().as_micros();

            let valid_visible_subset = selected
                .as_ref()
                .map(|s| s.valid_visible_subset)
                .unwrap_or(false);
            let selected_config_name = selected.as_ref().map(|s| s.config_name.clone());
            let selected_orientation_clustering =
                selected.as_ref().map(|s| s.orientation_clustering);

            let (raw, corners, extent, med, p95) = selected
                .as_ref()
                .map(|s| {
                    (
                        s.raw_chess_corners,
                        s.metrics.corner_count,
                        format!("{}x{}", s.metrics.extent_i, s.metrics.extent_j),
                        s.metrics
                            .residual_median_px
                            .map(|v| format!("{v:.3}"))
                            .unwrap_or_else(|| "-".to_string()),
                        s.metrics
                            .residual_p95_px
                            .map(|v| format!("{v:.3}"))
                            .unwrap_or_else(|| "-".to_string()),
                    )
                })
                .unwrap_or((0, 0, "0x0".to_string(), "-".to_string(), "-".to_string()));

            println!(
                "| {:>3} | {:>4} | {:>6} | {:>5} | {:>15} | {:>3} | {:>7} | {:>6} | {:>7} | {:>7} | {:>8.1} |",
                tgt_idx,
                snap,
                if strict_full_board.detected { "yes" } else { "no" },
                if valid_visible_subset { "yes" } else { "no" },
                selected_config_name.as_deref().unwrap_or("-"),
                raw,
                corners,
                extent,
                med,
                p95,
                timing_total_us as f64 / 1000.0,
            );

            frames.push(FrameReport {
                target_index: *tgt_idx,
                snap_index: snap,
                width: SNAP_WIDTH,
                height: SNAP_HEIGHT,
                strict_full_board,
                selected_config_name,
                selected_orientation_clustering,
                valid_visible_subset,
                selected,
                timing_total_us,
            });
        }
    }

    let aggregate = aggregate_frames(&frames, &configs, &raw_counts_by_config);

    println!();
    println!("=== Aggregate ===");
    println!(
        "frames: {} visible-subset valid: {} ({:.1}%) strict-full-board: {}",
        aggregate.n_frames,
        aggregate.valid_visible_subset_frames,
        aggregate.visible_subset_rate_pct,
        aggregate.strict_full_board_detected_frames
    );
    if let Some(v) = aggregate.median_selected_corner_count {
        println!("selected corner count median: {v}");
    }
    if let Some(v) = aggregate.p95_selected_corner_count {
        println!("selected corner count p95:    {v}");
    }
    if let Some(v) = aggregate.median_selected_residual_median_px {
        println!("valid median residual median: {v:.3} px");
    }
    if let Some(v) = aggregate.median_selected_residual_p95_px {
        println!("valid p95 residual median:    {v:.3} px");
    }
    println!(
        "per-frame total: median {:.1} ms, p95 {:.1} ms",
        aggregate.median_timing_total_us as f64 / 1000.0,
        aggregate.p95_timing_total_us as f64 / 1000.0,
    );

    if let Some(out) = output_path {
        let gate = VISIBLE_SUBSET_GATE_3536119669;
        let report = Report {
            schema_version: SCHEMA_VERSION,
            git_sha: git_sha(),
            expected_interior_corners: EXPECTED_INTERIOR,
            expected_rows: EXPECTED_ROWS,
            expected_cols: EXPECTED_COLS,
            visible_subset_gate: VisibleSubsetGateReport {
                min_corners: gate.min_corners,
                min_extent_i: gate.min_extent_i,
                min_extent_j: gate.min_extent_j,
                max_residual_median_px: gate.max_residual_median_px,
                max_residual_p95_px: gate.max_residual_p95_px,
            },
            frames,
            aggregate,
        };
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&out, serde_json::to_string_pretty(&report)?)?;
        println!("wrote report to {}", out.display());
    }

    Ok(())
}
