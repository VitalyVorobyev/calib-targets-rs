//! Phase A sweep harness: runs the instrumented chessboard detector over
//! every 720×540 sub-snap of `testdata/3536119669/target_*.png` and dumps
//! per-frame JSON + an aggregate summary CSV.
//!
//! Orthogonal to `chessboard_sweep_3536119669.rs`, which is a multi-config
//! grid search with a pass/fail gate. This harness is the Phase A
//! replacement used for algorithmic A/B comparisons: continuous metrics,
//! per-stage counts, per-reason rejection tallies.
//!
//! Usage:
//! ```text
//! cargo run --release -p calib-targets --example chessboard_sweep_v2 -- \
//!     --dataset testdata/3536119669 \
//!     --out bench_results/chessboard_3536119669/phaseA_<tag>
//! ```
//!
//! Optional `--config <json-path>` reads a [`ChessboardParams`] to use
//! instead of the compile-time default. Each ablation lives in its own
//! JSON config so the harness stays agnostic.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use calib_targets::chessboard::{
    ChessboardDebugFrame, ChessboardDetector, ChessboardGraphMode, ChessboardParams,
};
use calib_targets::detect::detect_corners;
use image::{GenericImageView, GrayImage};
use serde::Serialize;

const SNAP_WIDTH: u32 = 720;
const SNAP_HEIGHT: u32 = 540;
const SNAPS_PER_IMAGE: u32 = 6;

#[derive(Default, Clone)]
struct CliArgs {
    dataset: Option<PathBuf>,
    out_dir: Option<PathBuf>,
    config_path: Option<PathBuf>,
    config_name: Option<String>,
    mode: Option<String>,
    min_corners: Option<usize>,
    expected_rows: Option<u32>,
    expected_cols: Option<u32>,
    completeness: Option<f32>,
    chess_threshold: Option<f32>,
    global_prune: Option<bool>,
    local_prune: Option<bool>,
    local_threshold_rel: Option<f32>,
    local_threshold_px_floor: Option<f32>,
    local_window_half: Option<i32>,
}

#[derive(Serialize)]
struct FrameSummary {
    target_index: u32,
    snap_index: u32,
    width: u32,
    height: u32,
    // scalar Phase A signals
    corner_count: usize,
    extent_i: u32,
    extent_j: u32,
    horizontal_coverage_frac: Option<f32>,
    residual_median_px: Option<f32>,
    residual_p95_px: Option<f32>,
    edge_axis_residual_median_deg: Option<f32>,
    edge_axis_residual_p95_deg: Option<f32>,
    local_step_cv: Option<f32>,
    local_homography_residual_median_px: Option<f32>,
    local_homography_residual_p95_px: Option<f32>,
    graph_degree_0: u32,
    graph_degree_1: u32,
    graph_degree_2: u32,
    graph_degree_3: u32,
    graph_degree_4: u32,
    // stage counts
    raw_corners: usize,
    after_strength_filter: usize,
    after_orientation_cluster_filter: Option<usize>,
    graph_nodes: usize,
    graph_edges: usize,
    largest_component: usize,
    num_components: usize,
    assigned_grid_corners: usize,
    after_global_homography_prune: Option<usize>,
    final_labeled_corners: usize,
    // top reject reasons
    reject_out_of_distance_window: u64,
    reject_out_of_step_window: u64,
    reject_axis_line_disagree: u64,
    reject_no_axis_match_source: u64,
    reject_no_axis_match_candidate: u64,
    reject_not_orthogonal: u64,
    reject_edge_axis_angle_mismatch: u64,
    reject_missing_cluster: u64,
    reject_same_cluster_legacy: u64,
    reject_low_alignment: u64,
    reject_cluster_polarity_flip: u64,
    reject_local_homography_residual: u64,
    // timing
    timing_total_us: u128,
    // success indicator
    detection_ok: bool,
}

#[derive(Serialize)]
struct AggregateSummary {
    config_name: String,
    n_frames: usize,
    n_frames_detected: usize,
    detection_rate_pct: f32,
    // median and p95 of the main continuous signals across frames
    median_corner_count: Option<usize>,
    median_horizontal_coverage_frac: Option<f32>,
    p95_horizontal_coverage_frac: Option<f32>,
    median_residual_median_px: Option<f32>,
    median_residual_p95_px: Option<f32>,
    median_edge_axis_residual_median_deg: Option<f32>,
    median_local_step_cv: Option<f32>,
    median_local_homography_residual_median_px: Option<f32>,
    median_graph_degree_4_frac: Option<f32>,
    median_timing_total_us: u128,
}

fn parse_args() -> CliArgs {
    let mut args = CliArgs::default();
    let mut argv = env::args().skip(1);
    while let Some(a) = argv.next() {
        match a.as_str() {
            "--dataset" => args.dataset = argv.next().map(PathBuf::from),
            "--out" => args.out_dir = argv.next().map(PathBuf::from),
            "--config" => args.config_path = argv.next().map(PathBuf::from),
            "--name" => args.config_name = argv.next(),
            "--mode" => args.mode = argv.next(),
            "--min-corners" => args.min_corners = argv.next().and_then(|s| s.parse().ok()),
            "--rows" => args.expected_rows = argv.next().and_then(|s| s.parse().ok()),
            "--cols" => args.expected_cols = argv.next().and_then(|s| s.parse().ok()),
            "--completeness" => args.completeness = argv.next().and_then(|s| s.parse().ok()),
            "--chess-threshold" => args.chess_threshold = argv.next().and_then(|s| s.parse().ok()),
            "--no-global-prune" => args.global_prune = Some(false),
            "--global-prune" => args.global_prune = Some(true),
            "--local-prune" => args.local_prune = Some(true),
            "--no-local-prune" => args.local_prune = Some(false),
            "--local-threshold-rel" => {
                args.local_threshold_rel = argv.next().and_then(|s| s.parse().ok())
            }
            "--local-threshold-px" => {
                args.local_threshold_px_floor = argv.next().and_then(|s| s.parse().ok())
            }
            "--local-window-half" => {
                args.local_window_half = argv.next().and_then(|s| s.parse().ok())
            }
            "--help" | "-h" => {
                eprintln!(
                    "usage: chessboard_sweep_v2 --dataset <path> --out <dir> \\\n\
                     \t[--config <json>] [--name <tag>] [--mode legacy|two_axis] \\\n\
                     \t[--min-corners N] [--rows R] [--cols C] [--completeness 0..1] \\\n\
                     \t[--chess-threshold F]"
                );
                std::process::exit(0);
            }
            other => {
                eprintln!("unknown argument: {other}");
                std::process::exit(2);
            }
        }
    }
    args
}

fn main() {
    let args = parse_args();
    let dataset = args.dataset.expect("--dataset <path> is required");
    let out_dir = args.out_dir.expect("--out <dir> is required");
    fs::create_dir_all(&out_dir).expect("failed to create output directory");

    let mut params = match args.config_path.as_deref() {
        Some(path) => load_params(path),
        None => ChessboardParams::default(),
    };
    if let Some(mode) = args.mode.as_deref() {
        params.graph.mode = match mode {
            "legacy" => ChessboardGraphMode::Legacy,
            "two_axis" | "twoaxis" => ChessboardGraphMode::TwoAxis,
            other => {
                eprintln!("unknown --mode {other:?} (expected legacy|two_axis)");
                std::process::exit(2);
            }
        };
    }
    if let Some(n) = args.min_corners {
        params.min_corners = n;
    }
    if args.expected_rows.is_some() {
        params.expected_rows = args.expected_rows;
    }
    if args.expected_cols.is_some() {
        params.expected_cols = args.expected_cols;
    }
    if let Some(t) = args.completeness {
        params.completeness_threshold = t;
    }
    if let Some(t) = args.chess_threshold {
        params.chess.threshold_value = t;
    }
    if let Some(flag) = args.global_prune {
        params.enable_global_homography_prune = flag;
    }
    if let Some(flag) = args.local_prune {
        params.local_homography.enable = flag;
    }
    if let Some(t) = args.local_threshold_rel {
        params.local_homography.threshold_rel = t;
    }
    if let Some(t) = args.local_threshold_px_floor {
        params.local_homography.threshold_px_floor = t;
    }
    if let Some(w) = args.local_window_half {
        params.local_homography.window_half = w;
    }
    let config_name = args
        .config_name
        .clone()
        .or_else(|| {
            args.config_path
                .as_ref()
                .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
        })
        .unwrap_or_else(|| "baseline".to_string());
    eprintln!(
        "chessboard_sweep_v2: config={config_name} dataset={:?}",
        dataset
    );

    let target_paths = collect_target_images(&dataset);
    if target_paths.is_empty() {
        eprintln!("no target_*.png files found in {:?}", dataset);
        std::process::exit(1);
    }
    eprintln!("found {} target files", target_paths.len());

    let mut per_frame_json = Vec::new();
    let mut summary_rows: Vec<FrameSummary> = Vec::new();

    for path in &target_paths {
        let target_idx = parse_target_index(path).unwrap_or(0);
        let image = image::open(path)
            .expect("failed to open target image")
            .to_luma8();
        for snap_idx in 0..SNAPS_PER_IMAGE {
            let snap = extract_snap(&image, snap_idx);
            let started = Instant::now();
            let corners = detect_corners(&snap, &params.chess);
            let detector = ChessboardDetector::new(params.clone());
            let frame = detector.detect_debug_from_corners(&corners, snap.width(), snap.height());
            let elapsed = started.elapsed().as_micros();

            let row = build_summary_row(target_idx, snap_idx, &snap, &frame, elapsed);
            summary_rows.push(row);

            // Persist the compact debug frame (without the per-corner arrays
            // for the baseline sweep; enable full dumps via env when debugging).
            let per_frame = PerFrame {
                target_index: target_idx,
                snap_index: snap_idx,
                width: snap.width(),
                height: snap.height(),
                frame: if env::var("SWEEP_V2_FULL_FRAMES").is_ok() {
                    Some(&frame)
                } else {
                    None
                },
                metrics: serde_json::to_value(frame.metrics).unwrap(),
                stage_counts: serde_json::to_value(frame.stage_counts.clone()).unwrap(),
            };
            per_frame_json.push(serde_json::to_value(&per_frame).unwrap());
        }
    }

    let aggregate = aggregate_summary(&config_name, &summary_rows);

    // --- Write outputs ------------------------------------------------------
    let summary_path = out_dir.join(format!("{}_summary.csv", config_name));
    write_csv(&summary_path, &summary_rows);
    eprintln!("wrote {:?}", summary_path);

    let agg_path = out_dir.join(format!("{}_aggregate.json", config_name));
    fs::write(&agg_path, serde_json::to_string_pretty(&aggregate).unwrap())
        .expect("failed to write aggregate");
    eprintln!("wrote {:?}", agg_path);

    let frames_path = out_dir.join(format!("{}_per_frame.jsonl", config_name));
    let mut frame_lines = String::new();
    for entry in per_frame_json {
        frame_lines.push_str(&serde_json::to_string(&entry).unwrap());
        frame_lines.push('\n');
    }
    fs::write(&frames_path, frame_lines).expect("failed to write per-frame jsonl");
    eprintln!("wrote {:?}", frames_path);

    // Console summary: one line per aggregate metric so ablation A/B is
    // obvious in the terminal.
    println!("{}", serde_json::to_string_pretty(&aggregate).unwrap());
}

#[derive(Serialize)]
struct PerFrame<'a> {
    target_index: u32,
    snap_index: u32,
    width: u32,
    height: u32,
    metrics: serde_json::Value,
    stage_counts: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    frame: Option<&'a ChessboardDebugFrame>,
}

fn collect_target_images(dataset: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(dataset) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.starts_with("target_") && !s.contains(' '))
                    .unwrap_or(false)
                && path.extension().map(|e| e == "png").unwrap_or(false)
            {
                out.push(path);
            }
        }
    }
    out.sort_by_key(|a| parse_target_index(a));
    out
}

fn parse_target_index(path: &Path) -> Option<u32> {
    path.file_stem()
        .and_then(|s| s.to_str())
        .and_then(|s| s.strip_prefix("target_"))
        .and_then(|s| s.parse::<u32>().ok())
}

fn extract_snap(image: &GrayImage, snap_idx: u32) -> GrayImage {
    let x0 = snap_idx * SNAP_WIDTH;
    let view = image.view(x0, 0, SNAP_WIDTH, SNAP_HEIGHT);
    view.to_image()
}

fn load_params(path: &Path) -> ChessboardParams {
    let text = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("failed to read config {:?}: {e}", path));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("failed to parse config {:?}: {e}", path))
}

fn build_summary_row(
    target_idx: u32,
    snap_idx: u32,
    snap: &GrayImage,
    frame: &ChessboardDebugFrame,
    timing_total_us: u128,
) -> FrameSummary {
    let hist = frame.metrics.graph_degree_hist.unwrap_or([0; 5]);
    let rj = &frame.stage_counts.edges_by_reject_reason;
    let reason = |key: &str| *rj.get(key).unwrap_or(&0);
    FrameSummary {
        target_index: target_idx,
        snap_index: snap_idx,
        width: snap.width(),
        height: snap.height(),
        corner_count: frame.metrics.corner_count,
        extent_i: frame.metrics.extent_i,
        extent_j: frame.metrics.extent_j,
        horizontal_coverage_frac: frame.metrics.horizontal_coverage_frac,
        residual_median_px: frame.metrics.residual_median_px,
        residual_p95_px: frame.metrics.residual_p95_px,
        edge_axis_residual_median_deg: frame.metrics.edge_axis_residual_median_deg,
        edge_axis_residual_p95_deg: frame.metrics.edge_axis_residual_p95_deg,
        local_step_cv: frame.metrics.local_step_cv,
        local_homography_residual_median_px: frame.metrics.local_homography_residual_median_px,
        local_homography_residual_p95_px: frame.metrics.local_homography_residual_p95_px,
        graph_degree_0: hist[0],
        graph_degree_1: hist[1],
        graph_degree_2: hist[2],
        graph_degree_3: hist[3],
        graph_degree_4: hist[4],
        raw_corners: frame.stage_counts.raw_corners,
        after_strength_filter: frame.stage_counts.after_strength_filter,
        after_orientation_cluster_filter: frame.stage_counts.after_orientation_cluster_filter,
        graph_nodes: frame.stage_counts.graph_nodes,
        graph_edges: frame.stage_counts.graph_edges,
        largest_component: frame.stage_counts.largest_component_size,
        num_components: frame.stage_counts.num_components,
        assigned_grid_corners: frame.stage_counts.assigned_grid_corners,
        after_global_homography_prune: frame.stage_counts.after_global_homography_prune,
        final_labeled_corners: frame.stage_counts.final_labeled_corners,
        reject_out_of_distance_window: reason("out_of_distance_window"),
        reject_out_of_step_window: reason("out_of_step_window"),
        reject_axis_line_disagree: reason("axis_line_disagree"),
        reject_no_axis_match_source: reason("no_axis_match_source"),
        reject_no_axis_match_candidate: reason("no_axis_match_candidate"),
        reject_not_orthogonal: reason("not_orthogonal"),
        reject_edge_axis_angle_mismatch: reason("edge_axis_angle_mismatch"),
        reject_missing_cluster: reason("missing_cluster"),
        reject_same_cluster_legacy: reason("same_cluster_legacy"),
        reject_low_alignment: reason("low_alignment"),
        reject_cluster_polarity_flip: reason("cluster_polarity_flip"),
        reject_local_homography_residual: reason("local_homography_residual"),
        timing_total_us,
        detection_ok: frame.stage_counts.final_labeled_corners > 0,
    }
}

fn aggregate_summary(config_name: &str, rows: &[FrameSummary]) -> AggregateSummary {
    let n = rows.len();
    let detected: Vec<&FrameSummary> = rows.iter().filter(|r| r.detection_ok).collect();
    let n_detected = detected.len();
    let pct = if n == 0 {
        0.0
    } else {
        100.0 * n_detected as f32 / n as f32
    };

    let median_usize = |selector: fn(&FrameSummary) -> usize| -> Option<usize> {
        if detected.is_empty() {
            return None;
        }
        let mut v: Vec<usize> = detected.iter().map(|r| selector(r)).collect();
        v.sort_unstable();
        Some(v[v.len() / 2])
    };

    let median_of = |selector: fn(&FrameSummary) -> Option<f32>| -> Option<f32> {
        let mut v: Vec<f32> = rows
            .iter()
            .filter_map(selector)
            .filter(|x| x.is_finite())
            .collect();
        if v.is_empty() {
            return None;
        }
        v.sort_by(|a, b| a.total_cmp(b));
        Some(v[v.len() / 2])
    };

    let p95_of = |selector: fn(&FrameSummary) -> Option<f32>| -> Option<f32> {
        let mut v: Vec<f32> = rows
            .iter()
            .filter_map(selector)
            .filter(|x| x.is_finite())
            .collect();
        if v.is_empty() {
            return None;
        }
        v.sort_by(|a, b| a.total_cmp(b));
        let idx = ((v.len() as f32 - 1.0) * 0.95).round() as usize;
        Some(v[idx.min(v.len() - 1)])
    };

    let median_deg4_frac = if detected.is_empty() {
        None
    } else {
        let mut v: Vec<f32> = detected
            .iter()
            .map(|r| {
                let total = r.graph_degree_0
                    + r.graph_degree_1
                    + r.graph_degree_2
                    + r.graph_degree_3
                    + r.graph_degree_4;
                if total == 0 {
                    0.0
                } else {
                    r.graph_degree_4 as f32 / total as f32
                }
            })
            .collect();
        v.sort_by(|a, b| a.total_cmp(b));
        Some(v[v.len() / 2])
    };

    let median_timing = {
        let mut v: Vec<u128> = rows.iter().map(|r| r.timing_total_us).collect();
        v.sort_unstable();
        v.get(v.len() / 2).copied().unwrap_or(0)
    };

    AggregateSummary {
        config_name: config_name.to_string(),
        n_frames: n,
        n_frames_detected: n_detected,
        detection_rate_pct: pct,
        median_corner_count: median_usize(|r| r.corner_count),
        median_horizontal_coverage_frac: median_of(|r| r.horizontal_coverage_frac),
        p95_horizontal_coverage_frac: p95_of(|r| r.horizontal_coverage_frac),
        median_residual_median_px: median_of(|r| r.residual_median_px),
        median_residual_p95_px: median_of(|r| r.residual_p95_px),
        median_edge_axis_residual_median_deg: median_of(|r| r.edge_axis_residual_median_deg),
        median_local_step_cv: median_of(|r| r.local_step_cv),
        median_local_homography_residual_median_px: median_of(|r| {
            r.local_homography_residual_median_px
        }),
        median_graph_degree_4_frac: median_deg4_frac,
        median_timing_total_us: median_timing,
    }
}

fn write_csv(path: &Path, rows: &[FrameSummary]) {
    use std::io::Write;
    let mut buf = String::new();
    // header
    let header = [
        "target_index",
        "snap_index",
        "width",
        "height",
        "detection_ok",
        "corner_count",
        "extent_i",
        "extent_j",
        "horizontal_coverage_frac",
        "residual_median_px",
        "residual_p95_px",
        "edge_axis_residual_median_deg",
        "edge_axis_residual_p95_deg",
        "local_step_cv",
        "local_homography_residual_median_px",
        "local_homography_residual_p95_px",
        "graph_degree_0",
        "graph_degree_1",
        "graph_degree_2",
        "graph_degree_3",
        "graph_degree_4",
        "raw_corners",
        "after_strength_filter",
        "after_orientation_cluster_filter",
        "graph_nodes",
        "graph_edges",
        "largest_component",
        "num_components",
        "assigned_grid_corners",
        "after_global_homography_prune",
        "final_labeled_corners",
        "reject_out_of_distance_window",
        "reject_out_of_step_window",
        "reject_axis_line_disagree",
        "reject_no_axis_match_source",
        "reject_no_axis_match_candidate",
        "reject_not_orthogonal",
        "reject_edge_axis_angle_mismatch",
        "reject_missing_cluster",
        "reject_same_cluster_legacy",
        "reject_low_alignment",
        "reject_cluster_polarity_flip",
        "reject_local_homography_residual",
        "timing_total_us",
    ];
    buf.push_str(&header.join(","));
    buf.push('\n');

    for r in rows {
        use std::fmt::Write as _;
        let o = |v: Option<f32>| v.map(|x| format!("{x:.6}")).unwrap_or_default();
        let ou = |v: Option<usize>| v.map(|x| x.to_string()).unwrap_or_default();
        writeln!(
            &mut buf,
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            r.target_index,
            r.snap_index,
            r.width,
            r.height,
            r.detection_ok,
            r.corner_count,
            r.extent_i,
            r.extent_j,
            o(r.horizontal_coverage_frac),
            o(r.residual_median_px),
            o(r.residual_p95_px),
            o(r.edge_axis_residual_median_deg),
            o(r.edge_axis_residual_p95_deg),
            o(r.local_step_cv),
            o(r.local_homography_residual_median_px),
            o(r.local_homography_residual_p95_px),
            r.graph_degree_0,
            r.graph_degree_1,
            r.graph_degree_2,
            r.graph_degree_3,
            r.graph_degree_4,
            r.raw_corners,
            r.after_strength_filter,
            ou(r.after_orientation_cluster_filter),
            r.graph_nodes,
            r.graph_edges,
            r.largest_component,
            r.num_components,
            r.assigned_grid_corners,
            ou(r.after_global_homography_prune),
            r.final_labeled_corners,
            r.reject_out_of_distance_window,
            r.reject_out_of_step_window,
            r.reject_axis_line_disagree,
            r.reject_no_axis_match_source,
            r.reject_no_axis_match_candidate,
            r.reject_not_orthogonal,
            r.reject_edge_axis_angle_mismatch,
            r.reject_missing_cluster,
            r.reject_same_cluster_legacy,
            r.reject_low_alignment,
            r.reject_cluster_polarity_flip,
            r.reject_local_homography_residual,
            r.timing_total_us
        )
        .unwrap();
    }
    let mut f = fs::File::create(path).expect("failed to open output csv");
    f.write_all(buf.as_bytes()).expect("failed to write csv");
}
