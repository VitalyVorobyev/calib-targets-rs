use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use calib_targets::chessboard::{ChessboardAdvancedTuning, ChessboardDetector, ChessboardParams};
use calib_targets::core::DetectorConfig;
use calib_targets::detect::{default_chess_config, detect_corners, OrientationMethod};
use calib_targets_bench::span_timing::{
    command_output, cpu_name, span_ms, summarize, SpanTotals, SummaryStats, TimingLayer,
};
use clap::Parser;
use image::ImageReader;
use serde::Serialize;

#[derive(Clone, Copy, Debug, clap::ValueEnum, PartialEq, Eq)]
enum OrientationMethodArg {
    #[value(name = "ring_fit", alias = "ring-fit")]
    RingFit,
    #[value(name = "disk_fit", alias = "disk-fit")]
    DiskFit,
}

impl OrientationMethodArg {
    fn slug(self) -> &'static str {
        match self {
            OrientationMethodArg::RingFit => "ring_fit",
            OrientationMethodArg::DiskFit => "disk_fit",
        }
    }
}

impl From<OrientationMethodArg> for OrientationMethod {
    fn from(v: OrientationMethodArg) -> Self {
        match v {
            OrientationMethodArg::RingFit => OrientationMethod::RingFit,
            OrientationMethodArg::DiskFit => OrientationMethod::DiskFit,
        }
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "topo_stage_timing",
    about = "Measure ChESS + topological chessboard stage timings from tracing spans"
)]
struct Args {
    /// Directory containing PNG images.
    #[arg(long, default_value = "testdata/02-topo-grid")]
    image_dir: PathBuf,
    /// Explicit image paths. When supplied, these replace `--image-dir`.
    #[arg(long, num_args = 1..)]
    images: Vec<PathBuf>,
    /// Output JSON report path. Defaults to a slug-suffixed name so that
    /// `ring-fit` and `disk-fit` runs do not clobber each other.
    #[arg(long)]
    out: Option<PathBuf>,
    /// Timed repeats per image.
    #[arg(long, default_value_t = 30)]
    repeats: usize,
    /// Warmup repeats per image.
    #[arg(long, default_value_t = 5)]
    warmup: usize,
    /// Override chess-corners' axis-fit method. Default `ring-fit` matches
    /// upstream behaviour; `disk-fit` opts into the more accurate (slower)
    /// disk-sector fit added in chess-corners 0.9.
    #[arg(long, value_enum, default_value_t = OrientationMethodArg::RingFit)]
    orientation_method: OrientationMethodArg,
    /// Absolute ChESS response floor.
    #[arg(long, default_value_t = 100.0)]
    chess_threshold: f32,
    /// Stable chessboard admission floor.
    #[arg(long, default_value_t = 0.0)]
    min_corner_strength: f32,
    /// Minimum corners required for a public detection.
    #[arg(long, default_value_t = 8)]
    min_labeled_corners: usize,
    /// Maximum public chessboard components.
    #[arg(long, default_value_t = 3)]
    max_components: u32,
    /// Optional image upscale applied before the measured detector flow.
    #[arg(long, default_value_t = 1.0)]
    upscale: f32,
    /// Optional Gaussian blur applied after upscaling and before measurement.
    #[arg(long, default_value_t = 0.0)]
    pre_blur_sigma: f32,
    /// Projective-grid axis alignment tolerance in degrees.
    #[arg(long, default_value_t = 15.0)]
    axis_align_tol_deg: f32,
    /// Maximum accepted local-axis sigma in degrees.
    #[arg(long, default_value_t = 34.377_47)]
    max_axis_sigma_deg: f32,
    /// Global modulo-pi axis-cluster admission tolerance in degrees.
    #[arg(long, default_value_t = 16.0)]
    cluster_axis_tol_deg: f32,
    /// Maximum ratio between opposing quad edges.
    #[arg(long, default_value_t = 10.0)]
    opposing_edge_ratio_max: f32,
    /// Lower component-relative quad edge-length bound.
    #[arg(long, default_value_t = 0.0)]
    edge_length_min_rel: f32,
    /// Upper component-relative quad edge-length bound.
    #[arg(long, default_value_t = 1.8)]
    edge_length_max_rel: f32,
    /// Minimum quads retained per connected component.
    #[arg(long, default_value_t = 1)]
    min_quads_per_component: usize,
    /// Enable the conservative geometry-only recovery pass.
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    enable_geometry_only_recovery: bool,
    /// Geometry-only candidate tolerance relative to local cell size.
    #[arg(long, default_value_t = 0.15)]
    geometry_recovery_tol_rel: f32,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
struct TimingSample {
    corner_detection_ms: f64,
    input_adaptation_ms: f64,
    axis_filter_ms: f64,
    triangulation_ms: f64,
    edge_classification_ms: f64,
    triangle_merge_ms: f64,
    topological_filter_ms: f64,
    geometry_filter_ms: f64,
    cell_size_filter_ms: f64,
    walk_ms: f64,
    component_merge_ms: f64,
    validation_ms: f64,
    projective_fit_ms: f64,
    assembly_ms: f64,
    clustering_ms: f64,
    recovery_ms: f64,
    geometry_only_recovery_ms: f64,
    final_geometry_gate_ms: f64,
    output_assembly_ms: f64,
    chessboard_postprocessing_ms: f64,
    grid_total_ms: f64,
    full_total_ms: f64,
}

#[derive(Debug, Serialize)]
struct StageSummary {
    corner_detection: SummaryStats,
    input_adaptation: SummaryStats,
    axis_filter: SummaryStats,
    triangulation: SummaryStats,
    edge_classification: SummaryStats,
    triangle_merge: SummaryStats,
    topological_filter: SummaryStats,
    geometry_filter: SummaryStats,
    cell_size_filter: SummaryStats,
    walk: SummaryStats,
    component_merge: SummaryStats,
    validation: SummaryStats,
    projective_fit: SummaryStats,
    assembly: SummaryStats,
    clustering: SummaryStats,
    recovery: SummaryStats,
    geometry_only_recovery: SummaryStats,
    final_geometry_gate: SummaryStats,
    output_assembly: SummaryStats,
    chessboard_postprocessing: SummaryStats,
    grid_total: SummaryStats,
    full_total: SummaryStats,
}

#[derive(Debug, Serialize)]
struct ImageReport {
    image: String,
    width: u32,
    height: u32,
    raw_corners: usize,
    labelled_count: usize,
    component_count: usize,
    /// Number of measured repeats in which each tracing span was observed.
    /// An absent stage is therefore distinguishable from a measured zero.
    stage_observations: BTreeMap<String, usize>,
    summary: StageSummary,
    samples: Vec<TimingSample>,
}

#[derive(Debug, Serialize)]
struct Metadata {
    git_sha: Option<String>,
    dirty_state_sha256: Option<String>,
    rustc: Option<String>,
    cpu: Option<String>,
    profile: &'static str,
    repeats: usize,
    warmup: usize,
    timing_source: &'static str,
    orientation_method: &'static str,
    chess_threshold: f32,
    min_corner_strength: f32,
    min_labeled_corners: usize,
    max_components: u32,
    upscale: f32,
    pre_blur_sigma: f32,
    axis_align_tol_deg: f32,
    max_axis_sigma_deg: f32,
    cluster_axis_tol_deg: f32,
    opposing_edge_ratio_max: f32,
    edge_length_min_rel: f32,
    edge_length_max_rel: f32,
    min_quads_per_component: usize,
    enable_geometry_only_recovery: bool,
    geometry_recovery_tol_rel: f32,
}

#[derive(Debug, Serialize)]
struct Report {
    metadata: Metadata,
    images: Vec<ImageReport>,
}

fn image_paths(image_dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut paths = fs::read_dir(image_dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("png"))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}

fn summarize_samples(samples: &[TimingSample]) -> StageSummary {
    let values = |f: fn(&TimingSample) -> f64| samples.iter().map(f).collect::<Vec<_>>();
    StageSummary {
        corner_detection: summarize(values(|s| s.corner_detection_ms)),
        input_adaptation: summarize(values(|s| s.input_adaptation_ms)),
        axis_filter: summarize(values(|s| s.axis_filter_ms)),
        triangulation: summarize(values(|s| s.triangulation_ms)),
        edge_classification: summarize(values(|s| s.edge_classification_ms)),
        triangle_merge: summarize(values(|s| s.triangle_merge_ms)),
        topological_filter: summarize(values(|s| s.topological_filter_ms)),
        geometry_filter: summarize(values(|s| s.geometry_filter_ms)),
        cell_size_filter: summarize(values(|s| s.cell_size_filter_ms)),
        walk: summarize(values(|s| s.walk_ms)),
        component_merge: summarize(values(|s| s.component_merge_ms)),
        validation: summarize(values(|s| s.validation_ms)),
        projective_fit: summarize(values(|s| s.projective_fit_ms)),
        assembly: summarize(values(|s| s.assembly_ms)),
        clustering: summarize(values(|s| s.clustering_ms)),
        recovery: summarize(values(|s| s.recovery_ms)),
        geometry_only_recovery: summarize(values(|s| s.geometry_only_recovery_ms)),
        final_geometry_gate: summarize(values(|s| s.final_geometry_gate_ms)),
        output_assembly: summarize(values(|s| s.output_assembly_ms)),
        chessboard_postprocessing: summarize(values(|s| s.chessboard_postprocessing_ms)),
        grid_total: summarize(values(|s| s.grid_total_ms)),
        full_total: summarize(values(|s| s.full_total_ms)),
    }
}

fn measure_once(
    img: &image::GrayImage,
    chess_cfg: &DetectorConfig,
    params: &ChessboardParams,
    totals: &SpanTotals,
) -> (TimingSample, usize, usize, usize, Vec<&'static str>) {
    totals.clear();
    let full_start = Instant::now();
    let corner_start = Instant::now();
    let corners = detect_corners(img, chess_cfg);
    let corner_wall_ms = corner_start.elapsed().as_secs_f64() * 1000.0;

    let grid_start = Instant::now();
    let detections = ChessboardDetector::new(params.clone())
        .expect("valid detector params")
        .detect_all(&corners);
    let grid_wall_ms = grid_start.elapsed().as_secs_f64() * 1000.0;
    let full_wall_ms = full_start.elapsed().as_secs_f64() * 1000.0;
    let spans = totals.snapshot_ms();

    let labelled_count = detections
        .iter()
        .map(|d| d.corners.len())
        .max()
        .unwrap_or(0);
    let recovery_ms = span_ms(&spans, "recover_topological_components");
    let geometry_only_recovery_ms = span_ms(&spans, "chessboard_geometry_only_recovery");
    let output_assembly_ms = span_ms(&spans, "build_topological_detections");
    let sample = TimingSample {
        corner_detection_ms: span_ms(&spans, "detect_corners").max(corner_wall_ms),
        input_adaptation_ms: span_ms(&spans, "topological_inputs"),
        axis_filter_ms: span_ms(&spans, "usable_mask"),
        triangulation_ms: span_ms(&spans, "delaunay_triangulate"),
        edge_classification_ms: span_ms(&spans, "classify_all_edges"),
        triangle_merge_ms: span_ms(&spans, "merge_triangle_pairs"),
        topological_filter_ms: span_ms(&spans, "topological_quad_filter"),
        geometry_filter_ms: span_ms(&spans, "geometry_quad_filter"),
        cell_size_filter_ms: span_ms(&spans, "cell_size_quad_filter"),
        walk_ms: span_ms(&spans, "label_components"),
        component_merge_ms: span_ms(&spans, "topological_component_merge"),
        validation_ms: span_ms(&spans, "topological_validation"),
        projective_fit_ms: span_ms(&spans, "topological_projective_fit"),
        assembly_ms: span_ms(&spans, "topological_assembly"),
        clustering_ms: span_ms(&spans, "topological_clustered_augs"),
        recovery_ms,
        geometry_only_recovery_ms,
        final_geometry_gate_ms: span_ms(&spans, "chessboard_final_geometry_gate"),
        output_assembly_ms,
        chessboard_postprocessing_ms: recovery_ms + output_assembly_ms,
        grid_total_ms: span_ms(&spans, "detect_all_topological").max(grid_wall_ms),
        full_total_ms: full_wall_ms,
    };
    let mut observed_spans = spans.keys().copied().collect::<Vec<_>>();
    observed_spans.sort_unstable();
    (
        sample,
        corners.len(),
        labelled_count,
        detections.len(),
        observed_spans,
    )
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    if !args.upscale.is_finite() || args.upscale <= 0.0 {
        return Err("--upscale must be finite and positive".into());
    }
    if !args.pre_blur_sigma.is_finite() || args.pre_blur_sigma < 0.0 {
        return Err("--pre-blur-sigma must be finite and non-negative".into());
    }
    if !args.geometry_recovery_tol_rel.is_finite() || args.geometry_recovery_tol_rel <= 0.0 {
        return Err("--geometry-recovery-tol-rel must be finite and positive".into());
    }

    let totals = TimingLayer::install()?;

    let mut chess_cfg = default_chess_config();
    // `DetectorConfig::orientation_method` is `Option<_>` since
    // chess-corners 1.0 (`None` skips the axis fit). The bench always
    // fits orientation; `--orientation-method` still selects only which
    // fit to run.
    chess_cfg.orientation_method = Some(args.orientation_method.into());
    chess_cfg.threshold = args.chess_threshold;

    let mut topological = projective_grid::expert::TopologicalParams::default();
    topological.axis_align_tol_rad = args.axis_align_tol_deg.to_radians();
    topological.max_axis_sigma_rad = args.max_axis_sigma_deg.to_radians();
    topological.cluster_axis_tol_rad = args.cluster_axis_tol_deg.to_radians();
    topological.opposing_edge_ratio_max = args.opposing_edge_ratio_max;
    topological.edge_length_min_rel = args.edge_length_min_rel;
    topological.edge_length_max_rel = args.edge_length_max_rel;
    topological.min_quads_per_component = args.min_quads_per_component;
    let mut advanced = ChessboardAdvancedTuning::default();
    advanced.topological = topological;
    advanced.enable_geometry_only_recovery = args.enable_geometry_only_recovery;
    advanced.geometry_recovery_tol_rel = args.geometry_recovery_tol_rel;
    let mut params = ChessboardParams::default();
    params.min_corner_strength = args.min_corner_strength;
    params.min_labeled_corners = args.min_labeled_corners;
    params.max_components = args.max_components;
    let params = params.with_advanced(advanced);

    let mut images = Vec::new();
    let image_paths = if args.images.is_empty() {
        image_paths(&args.image_dir)?
    } else {
        args.images.clone()
    };
    for path in image_paths {
        let mut img = ImageReader::open(&path)?.decode()?.to_luma8();
        if args.upscale != 1.0 {
            let width = ((img.width() as f32 * args.upscale).round() as u32).max(1);
            let height = ((img.height() as f32 * args.upscale).round() as u32).max(1);
            img = image::imageops::resize(
                &img,
                width,
                height,
                image::imageops::FilterType::CatmullRom,
            );
        }
        if args.pre_blur_sigma > 0.0 {
            img = image::imageops::blur(&img, args.pre_blur_sigma);
        }
        for _ in 0..args.warmup {
            let _ = measure_once(&img, &chess_cfg, &params, &totals);
        }

        let mut samples = Vec::with_capacity(args.repeats);
        let mut raw_corners = 0;
        let mut labelled_count = 0;
        let mut component_count = 0;
        let mut stage_observations = BTreeMap::new();
        for _ in 0..args.repeats {
            let (sample, corners, labelled, components, observed_spans) =
                measure_once(&img, &chess_cfg, &params, &totals);
            raw_corners = corners;
            labelled_count = labelled;
            component_count = components;
            for name in observed_spans {
                *stage_observations.entry(name.to_owned()).or_insert(0) += 1;
            }
            samples.push(sample);
        }

        images.push(ImageReport {
            image: path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_owned(),
            width: img.width(),
            height: img.height(),
            raw_corners,
            labelled_count,
            component_count,
            stage_observations,
            summary: summarize_samples(&samples),
            samples,
        });
    }

    let report = Report {
        metadata: Metadata {
            git_sha: command_output("git", &["rev-parse", "HEAD"]),
            dirty_state_sha256: command_output(
                "sh",
                &[
                    "-c",
                    "git diff --binary HEAD | shasum -a 256 | cut -d' ' -f1",
                ],
            ),
            rustc: command_output("rustc", &["--version"]),
            cpu: cpu_name(),
            profile: if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
            repeats: args.repeats,
            warmup: args.warmup,
            timing_source: "tracing_spans",
            orientation_method: args.orientation_method.slug(),
            chess_threshold: args.chess_threshold,
            min_corner_strength: args.min_corner_strength,
            min_labeled_corners: args.min_labeled_corners,
            max_components: args.max_components,
            upscale: args.upscale,
            pre_blur_sigma: args.pre_blur_sigma,
            axis_align_tol_deg: args.axis_align_tol_deg,
            max_axis_sigma_deg: args.max_axis_sigma_deg,
            cluster_axis_tol_deg: args.cluster_axis_tol_deg,
            opposing_edge_ratio_max: args.opposing_edge_ratio_max,
            edge_length_min_rel: args.edge_length_min_rel,
            edge_length_max_rel: args.edge_length_max_rel,
            min_quads_per_component: args.min_quads_per_component,
            enable_geometry_only_recovery: args.enable_geometry_only_recovery,
            geometry_recovery_tol_rel: args.geometry_recovery_tol_rel,
        },
        images,
    };

    let out_path = args.out.unwrap_or_else(|| {
        PathBuf::from(format!(
            "tools/out/topo-grid-performance/stage-breakdown-{}.json",
            args.orientation_method.slug()
        ))
    });
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&out_path, serde_json::to_string_pretty(&report)?)?;
    println!("wrote {}", out_path.display());
    Ok(())
}
