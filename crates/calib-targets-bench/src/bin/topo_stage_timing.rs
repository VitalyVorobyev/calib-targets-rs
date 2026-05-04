use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use calib_targets::chessboard::{Detector, DetectorParams, GraphBuildAlgorithm};
use calib_targets::core::ChessConfig;
use calib_targets::detect::{default_chess_config, detect_corners};
use clap::Parser;
use image::ImageReader;
use serde::Serialize;
use tracing::{Id, Subscriber};
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::registry::{LookupSpan, Registry};
use tracing_subscriber::Layer;

#[derive(Parser, Debug)]
#[command(
    name = "topo_stage_timing",
    about = "Measure ChESS + topological chessboard stage timings from tracing spans"
)]
struct Args {
    /// Directory containing PNG images.
    #[arg(long, default_value = "testdata/02-topo-grid")]
    image_dir: PathBuf,
    /// Output JSON report path.
    #[arg(
        long,
        default_value = "tools/out/topo-grid-performance/stage-breakdown.json"
    )]
    out: PathBuf,
    /// Timed repeats per image.
    #[arg(long, default_value_t = 30)]
    repeats: usize,
    /// Warmup repeats per image.
    #[arg(long, default_value_t = 5)]
    warmup: usize,
    /// Optional explicit ChESS pre-blur sigma in pixels.
    #[arg(long, default_value_t = 0.0)]
    blur_sigma: f32,
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
    walk_ms: f64,
    component_merge_ms: f64,
    clustering_ms: f64,
    recovery_ms: f64,
    ordering_ms: f64,
    grid_total_ms: f64,
    full_total_ms: f64,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
struct SummaryStats {
    mean_ms: f64,
    p50_ms: f64,
    p95_ms: f64,
    max_ms: f64,
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
    walk: SummaryStats,
    component_merge: SummaryStats,
    clustering: SummaryStats,
    recovery: SummaryStats,
    ordering: SummaryStats,
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
    summary: StageSummary,
    samples: Vec<TimingSample>,
}

#[derive(Debug, Serialize)]
struct Metadata {
    git_sha: Option<String>,
    rustc: Option<String>,
    cpu: Option<String>,
    profile: &'static str,
    repeats: usize,
    warmup: usize,
    blur_sigma: f32,
    timing_source: &'static str,
}

#[derive(Debug, Serialize)]
struct Report {
    metadata: Metadata,
    images: Vec<ImageReport>,
}

#[derive(Default)]
struct SpanTotals {
    totals: Mutex<HashMap<&'static str, Duration>>,
}

impl SpanTotals {
    fn clear(&self) {
        self.totals
            .lock()
            .expect("span totals mutex poisoned")
            .clear();
    }

    fn snapshot_ms(&self) -> HashMap<&'static str, f64> {
        self.totals
            .lock()
            .expect("span totals mutex poisoned")
            .iter()
            .map(|(&name, &duration)| (name, duration.as_secs_f64() * 1000.0))
            .collect()
    }

    fn add(&self, name: &'static str, duration: Duration) {
        *self
            .totals
            .lock()
            .expect("span totals mutex poisoned")
            .entry(name)
            .or_default() += duration;
    }
}

struct SpanTiming {
    name: &'static str,
    entered_at: Option<Instant>,
    elapsed: Duration,
}

struct TimingLayer {
    totals: Arc<SpanTotals>,
}

impl<S> Layer<S> for TimingLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &tracing::span::Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(SpanTiming {
                name: attrs.metadata().name(),
                entered_at: None,
                elapsed: Duration::ZERO,
            });
        }
    }

    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id) {
            if let Some(timing) = span.extensions_mut().get_mut::<SpanTiming>() {
                timing.entered_at = Some(Instant::now());
            }
        }
    }

    fn on_exit(&self, id: &Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id) {
            if let Some(timing) = span.extensions_mut().get_mut::<SpanTiming>() {
                if let Some(start) = timing.entered_at.take() {
                    timing.elapsed += start.elapsed();
                }
            }
        }
    }

    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(&id) {
            if let Some(timing) = span.extensions_mut().remove::<SpanTiming>() {
                self.totals.add(timing.name, timing.elapsed);
            }
        }
    }
}

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?;
    let value = value.trim().to_owned();
    (!value.is_empty()).then_some(value)
}

fn cpu_name() -> Option<String> {
    command_output("sysctl", &["-n", "machdep.cpu.brand_string"]).or_else(|| {
        command_output(
            "sh",
            &["-c", "lscpu | sed -n 's/^Model name:[[:space:]]*//p'"],
        )
    })
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

fn summarize(mut values: Vec<f64>) -> SummaryStats {
    if values.is_empty() {
        return SummaryStats::default();
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mean_ms = values.iter().sum::<f64>() / values.len() as f64;
    let percentile = |q: f64| {
        let idx = ((values.len() - 1) as f64 * q).round() as usize;
        values[idx.min(values.len() - 1)]
    };
    SummaryStats {
        mean_ms,
        p50_ms: percentile(0.50),
        p95_ms: percentile(0.95),
        max_ms: *values.last().unwrap_or(&0.0),
    }
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
        walk: summarize(values(|s| s.walk_ms)),
        component_merge: summarize(values(|s| s.component_merge_ms)),
        clustering: summarize(values(|s| s.clustering_ms)),
        recovery: summarize(values(|s| s.recovery_ms)),
        ordering: summarize(values(|s| s.ordering_ms)),
        grid_total: summarize(values(|s| s.grid_total_ms)),
        full_total: summarize(values(|s| s.full_total_ms)),
    }
}

fn span_ms(spans: &HashMap<&'static str, f64>, name: &'static str) -> f64 {
    spans.get(name).copied().unwrap_or(0.0)
}

fn measure_once(
    img: &image::GrayImage,
    chess_cfg: &ChessConfig,
    params: &DetectorParams,
    totals: &SpanTotals,
) -> (TimingSample, usize, usize, usize) {
    totals.clear();
    let full_start = Instant::now();
    let corner_start = Instant::now();
    let corners = detect_corners(img, chess_cfg);
    let corner_wall_ms = corner_start.elapsed().as_secs_f64() * 1000.0;

    let grid_start = Instant::now();
    let detections = Detector::new(params.clone()).detect_all(&corners);
    let grid_wall_ms = grid_start.elapsed().as_secs_f64() * 1000.0;
    let full_wall_ms = full_start.elapsed().as_secs_f64() * 1000.0;
    let spans = totals.snapshot_ms();

    let labelled_count = detections
        .iter()
        .map(|d| d.target.corners.len())
        .max()
        .unwrap_or(0);
    let sample = TimingSample {
        corner_detection_ms: span_ms(&spans, "detect_corners").max(corner_wall_ms),
        input_adaptation_ms: span_ms(&spans, "topological_inputs"),
        axis_filter_ms: span_ms(&spans, "usable_mask"),
        triangulation_ms: span_ms(&spans, "delaunay_triangulate"),
        edge_classification_ms: span_ms(&spans, "classify_all_edges"),
        triangle_merge_ms: span_ms(&spans, "merge_triangle_pairs"),
        topological_filter_ms: span_ms(&spans, "topological_quad_filter"),
        geometry_filter_ms: span_ms(&spans, "geometry_quad_filter"),
        walk_ms: span_ms(&spans, "label_components"),
        component_merge_ms: span_ms(&spans, "topological_initial_component_merge"),
        clustering_ms: span_ms(&spans, "topological_clustered_augs"),
        recovery_ms: span_ms(&spans, "recover_topological_components"),
        ordering_ms: span_ms(&spans, "build_topological_detections"),
        grid_total_ms: span_ms(&spans, "detect_all_topological").max(grid_wall_ms),
        full_total_ms: full_wall_ms,
    };
    (sample, corners.len(), labelled_count, detections.len())
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let totals = Arc::new(SpanTotals::default());
    let subscriber = Registry::default().with(TimingLayer {
        totals: Arc::clone(&totals),
    });
    tracing::subscriber::set_global_default(subscriber)?;

    let mut chess_cfg = default_chess_config();
    chess_cfg.pre_blur_sigma_px = args.blur_sigma;

    let mut params = DetectorParams::default();
    params.graph_build_algorithm = GraphBuildAlgorithm::Topological;

    let mut images = Vec::new();
    for path in image_paths(&args.image_dir)? {
        let img = ImageReader::open(&path)?.decode()?.to_luma8();
        for _ in 0..args.warmup {
            let _ = measure_once(&img, &chess_cfg, &params, &totals);
        }

        let mut samples = Vec::with_capacity(args.repeats);
        let mut raw_corners = 0;
        let mut labelled_count = 0;
        let mut component_count = 0;
        for _ in 0..args.repeats {
            let (sample, corners, labelled, components) =
                measure_once(&img, &chess_cfg, &params, &totals);
            raw_corners = corners;
            labelled_count = labelled;
            component_count = components;
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
            summary: summarize_samples(&samples),
            samples,
        });
    }

    let report = Report {
        metadata: Metadata {
            git_sha: command_output("git", &["rev-parse", "--short", "HEAD"]),
            rustc: command_output("rustc", &["--version"]),
            cpu: cpu_name(),
            profile: if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
            repeats: args.repeats,
            warmup: args.warmup,
            blur_sigma: args.blur_sigma,
            timing_source: "tracing_spans",
        },
        images,
    };

    if let Some(parent) = args.out.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&args.out, serde_json::to_string_pretty(&report)?)?;
    println!("wrote {}", args.out.display());
    Ok(())
}
