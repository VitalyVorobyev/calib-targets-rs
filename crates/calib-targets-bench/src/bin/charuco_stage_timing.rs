//! Per-stage timing harness for `CharucoDetector::detect`.
//!
//! Mirrors `topo_stage_timing.rs`: it installs a `tracing` layer that
//! accumulates per-span busy time, runs the production `detect` path over
//! warmup + timed repeats on a single public fixture, and attributes the
//! board-level matcher's wall time across its internal stages
//! (`match_board`, `sample_cells`, `build_score_matrix`,
//! `enumerate_hypotheses`, `emit_markers`). The diagnostics-only fills are
//! not measured here — the production `detect` path computes no diagnostics.
//!
//! This is instrumentation only — it runs the unmodified `detect` path and
//! does not change detection output. The JSON report is a local-only
//! artifact (written under a gitignored directory).

use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use calib_targets::charuco::{CharucoDetectConfig, CharucoDetector};
use calib_targets::detect::{default_chess_config, detect_corners, gray_view};
use clap::Parser;
use image::ImageReader;
use serde::Serialize;
use tracing::{Id, Subscriber};
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::registry::{LookupSpan, Registry};
use tracing_subscriber::Layer;

/// Board-level matcher spans, in the order the matcher invokes them. The
/// first entry (`match_board`) is the production decode total; the rest are
/// reported as a percentage of it.
const STAGE_SPANS: &[&str] = &[
    "match_board",
    "sample_cells",
    "build_score_matrix",
    "enumerate_hypotheses",
    "emit_markers",
];

#[derive(Parser, Debug)]
#[command(
    name = "charuco_stage_timing",
    about = "Measure ChArUco board-level matcher stage timings from tracing spans"
)]
struct Args {
    /// Input image (public testdata fixture).
    #[arg(long, default_value = "testdata/small2.png")]
    image: PathBuf,
    /// ChArUco detect config JSON (board spec + params).
    #[arg(long, default_value = "testdata/charuco_detect_config_small.json")]
    config: PathBuf,
    /// Timed repeats.
    #[arg(long, default_value_t = 50)]
    repeats: usize,
    /// Warmup repeats (discarded).
    #[arg(long, default_value_t = 5)]
    warmup: usize,
    /// Output JSON report path. Defaults to a gitignored directory.
    #[arg(long)]
    out: Option<PathBuf>,
}

#[derive(Clone, Debug, Default, Serialize)]
struct TimingSample {
    /// Per-span busy time, keyed by span name (ms).
    spans: HashMap<String, f64>,
    /// Wall time of the full `detect` call (ms).
    detect_ms: f64,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
struct SummaryStats {
    mean_ms: f64,
    p50_ms: f64,
    p95_ms: f64,
    max_ms: f64,
}

#[derive(Debug, Serialize)]
struct StageReport {
    name: String,
    summary: SummaryStats,
    /// p50 percentage of the `match_board` (decode) total.
    pct_of_decode_p50: f64,
}

#[derive(Debug, Serialize)]
struct Workload {
    raw_corners: usize,
    candidate_cells: usize,
    labelled_corners: usize,
    markers: usize,
    components: usize,
}

#[derive(Debug, Serialize)]
struct Metadata {
    git_sha: Option<String>,
    rustc: Option<String>,
    cpu: Option<String>,
    profile: &'static str,
    repeats: usize,
    warmup: usize,
    timing_source: &'static str,
}

#[derive(Debug, Serialize)]
struct Report {
    metadata: Metadata,
    image: String,
    config: String,
    width: u32,
    height: u32,
    workload: Workload,
    detect: SummaryStats,
    stages: Vec<StageReport>,
    samples: Vec<TimingSample>,
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

fn span_ms(spans: &HashMap<&'static str, f64>, name: &str) -> f64 {
    spans.get(name).copied().unwrap_or(0.0)
}

/// Run one timed `detect` and capture per-span busy time plus the `detect`
/// wall time. Returns the marker / labelled-corner counts so the caller can
/// confirm output stability across repeats.
fn measure_once(
    detector: &CharucoDetector,
    img: &image::GrayImage,
    corners: &[calib_targets::chessboard::ChessCorner],
    totals: &SpanTotals,
) -> (TimingSample, usize, usize) {
    totals.clear();
    let view = gray_view(img);
    let detect_start = Instant::now();
    let result = detector.detect(&view, corners);
    let detect_ms = detect_start.elapsed().as_secs_f64() * 1000.0;
    let spans_raw = totals.snapshot_ms();

    let (labelled, markers) = match &result {
        Ok(res) => (res.corners.len(), res.markers.len()),
        Err(_) => (0, 0),
    };

    let spans = STAGE_SPANS
        .iter()
        .map(|&name| (name.to_owned(), span_ms(&spans_raw, name)))
        .collect();

    (TimingSample { spans, detect_ms }, labelled, markers)
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let totals = Arc::new(SpanTotals::default());
    let subscriber = Registry::default().with(TimingLayer {
        totals: Arc::clone(&totals),
    });
    tracing::subscriber::set_global_default(subscriber)?;

    // Build the detector from the checked-in config (board spec + params).
    let cfg = CharucoDetectConfig::load_json(&args.config)?;
    let detector = cfg.build_detector()?;

    let img = ImageReader::open(&args.image)?.decode()?.to_luma8();
    let (width, height) = (img.width(), img.height());

    // Corners are stable detector input; detect them once and reuse them for
    // every repeat so the timed loop measures only `detect`.
    let corners = detect_corners(&img, &default_chess_config());

    // One diagnostics pass (production work, identical to `detect`) to recover
    // the candidate-cell count for the workload report. Not timed.
    let view = gray_view(&img);
    let (diag_result, diagnostics) = detector.detect_with_diagnostics(&view, &corners);
    let candidate_cells: usize = diagnostics
        .components
        .iter()
        .map(|c| c.candidate_cell_count)
        .sum();
    let components = diagnostics.components.len();
    let (labelled_corners, markers) = match &diag_result {
        Ok(res) => (res.corners.len(), res.markers.len()),
        Err(e) => {
            eprintln!("warning: detect failed on {}: {e}", args.image.display());
            (0, 0)
        }
    };

    for _ in 0..args.warmup {
        let _ = measure_once(&detector, &img, &corners, &totals);
    }

    let mut samples = Vec::with_capacity(args.repeats);
    for _ in 0..args.repeats {
        let (sample, _labelled, _markers) = measure_once(&detector, &img, &corners, &totals);
        samples.push(sample);
    }

    let detect = summarize(samples.iter().map(|s| s.detect_ms).collect());
    let decode_p50 = summarize(
        samples
            .iter()
            .map(|s| s.spans.get("match_board").copied().unwrap_or(0.0))
            .collect(),
    )
    .p50_ms;

    let stages: Vec<StageReport> = STAGE_SPANS
        .iter()
        .map(|&name| {
            let summary = summarize(
                samples
                    .iter()
                    .map(|s| s.spans.get(name).copied().unwrap_or(0.0))
                    .collect(),
            );
            let pct = if decode_p50 > 0.0 {
                100.0 * summary.p50_ms / decode_p50
            } else {
                0.0
            };
            StageReport {
                name: name.to_owned(),
                summary,
                pct_of_decode_p50: pct,
            }
        })
        .collect();

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
            timing_source: "tracing_spans",
        },
        image: args.image.display().to_string(),
        config: args.config.display().to_string(),
        width,
        height,
        workload: Workload {
            raw_corners: corners.len(),
            candidate_cells,
            labelled_corners,
            markers,
            components,
        },
        detect,
        stages,
        samples,
    };

    print_table(&report, decode_p50);

    let out_path = args
        .out
        .unwrap_or_else(|| PathBuf::from("tools/out/charuco-stage-timing/stage-breakdown.json"));
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&out_path, serde_json::to_string_pretty(&report)?)?;
    println!("\nwrote {}", out_path.display());
    Ok(())
}

fn print_table(report: &Report, decode_p50: f64) {
    let w = &report.workload;
    println!(
        "charuco_stage_timing: {} ({}x{})",
        Path::new(&report.image).display(),
        report.width,
        report.height,
    );
    println!(
        "  workload: raw_corners={}  candidate_cells={}  labelled_corners={}  \
         markers={}  components={}",
        w.raw_corners, w.candidate_cells, w.labelled_corners, w.markers, w.components,
    );
    println!(
        "  repeats={}  warmup={}  profile={}",
        report.metadata.repeats, report.metadata.warmup, report.metadata.profile,
    );
    println!();
    println!(
        "  {:<26} {:>10} {:>10} {:>12}",
        "stage", "p50 (ms)", "p95 (ms)", "% decode"
    );
    println!("  {:-<26} {:->10} {:->10} {:->12}", "", "", "", "");
    for stage in &report.stages {
        let marker = if stage.name == "match_board" {
            "*"
        } else {
            " "
        };
        println!(
            "{marker} {:<26} {:>10.4} {:>10.4} {:>11.1}%",
            stage.name, stage.summary.p50_ms, stage.summary.p95_ms, stage.pct_of_decode_p50,
        );
    }
    println!("  {:-<26} {:->10} {:->10} {:->12}", "", "", "", "");
    println!(
        "  {:<26} {:>10.4} {:>10.4} {:>12}",
        "detect (wall)", report.detect.p50_ms, report.detect.p95_ms, "—"
    );
    println!(
        "  (* = match_board total decode = {decode_p50:.4} ms p50; stage %% are of this total)"
    );
}
