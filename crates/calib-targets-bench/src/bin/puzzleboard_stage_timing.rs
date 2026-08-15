//! Per-stage timing harness for `PuzzleBoardDetector::detect`.
//!
//! Companion to `topo_stage_timing` / `charuco_stage_timing`: installs the
//! shared [`TimingLayer`](calib_targets_bench::span_timing::TimingLayer),
//! runs the production `detect` path over warmup + timed repeats on a public
//! synthetic fixture, and attributes the wall time across the pipeline's
//! stages.
//!
//! The stage tree, outermost first:
//!
//! ```text
//! detect_inner                     whole detect call
//!   chess_detect_all               chessboard graph build over the ChESS corners
//!   decode_component               once per connected component
//!     sample_all_edges               read one dot per interior edge from the image
//!     decode_edges                   recover (D4, origin) from the edge bits
//!       build                          cyclic class precompute, per D4 transform
//!       origin_scan / fold             rank origins against the tables
//! ```
//!
//! Spans nest, so a parent's busy time includes its children; the report gives
//! each stage as a percentage of `detect_inner` and also reports the residual
//! not attributed to any child, which is where unlabelled work hides.
//!
//! ChESS corner detection is deliberately *outside* the timed region — it is
//! detector input, shared with every other target family, and including it
//! would swamp the stages this harness exists to separate. `benches/
//! synthetic_decode.rs` times it separately.
//!
//! This is instrumentation only — it runs the unmodified `detect` path and does
//! not change detection output. The JSON report is a local-only artifact.
//!
//! ```text
//! cargo run --release -p calib-targets-bench --bin puzzleboard_stage_timing
//! ```

use std::error::Error;
use std::fs;
use std::path::PathBuf;

use calib_targets::detect::{default_chess_config, detect_corners, gray_view};
use calib_targets::puzzleboard::{
    PuzzleBoardDetector, PuzzleBoardParams, PuzzleBoardScoringMode, PuzzleBoardSearchMode,
    PuzzleBoardSpec,
};
use calib_targets_bench::span_timing::{
    command_output, cpu_name, span_ms, summarize, SummaryStats, TimingLayer,
};
use clap::{Parser, ValueEnum};
use image::ImageReader;
use serde::Serialize;
use std::collections::HashMap;
use std::time::Instant;

/// Stage spans, outermost first. `detect_inner` is the total; the rest are
/// reported as a percentage of it.
const STAGE_SPANS: &[&str] = &[
    "detect_inner",
    "chess_detect_all",
    "decode_component",
    "sample_all_edges",
    "decode_edges",
    "scan",
    "build",
    "origin_scan",
    "fold",
];

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum SearchArg {
    /// Sweep the whole 501 × 501 master.
    Full,
    /// Restrict to the declared board's rectangle.
    Fixed,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum ScoringArg {
    /// Hard bit-match count with a confidence-weighted tie-break.
    Hard,
    /// Per-bit log-likelihood with a margin gate (the default).
    Soft,
}

#[derive(Parser, Debug)]
#[command(
    name = "puzzleboard_stage_timing",
    about = "Measure PuzzleBoard detect stage timings from tracing spans"
)]
struct Args {
    /// Input image (public synthetic fixture).
    #[arg(
        long,
        default_value = "testdata/puzzleboard_synthetic_author_like/author_like_oblique.png"
    )]
    image: PathBuf,
    /// Board rows in squares.
    #[arg(long, default_value_t = 20)]
    rows: u32,
    /// Board columns in squares.
    #[arg(long, default_value_t = 20)]
    cols: u32,
    /// Board row origin on the master pattern.
    #[arg(long, default_value_t = 18)]
    origin_row: u32,
    /// Board column origin on the master pattern.
    #[arg(long, default_value_t = 219)]
    origin_col: u32,
    /// Origin search strategy.
    #[arg(long, value_enum, default_value_t = SearchArg::Full)]
    search: SearchArg,
    /// Hypothesis scoring function.
    #[arg(long, value_enum, default_value_t = ScoringArg::Soft)]
    scoring: ScoringArg,
    /// Timed repeats.
    #[arg(long, default_value_t = 20)]
    repeats: usize,
    /// Warmup repeats (discarded).
    #[arg(long, default_value_t = 3)]
    warmup: usize,
    /// Output JSON report path. Defaults to a gitignored directory.
    #[arg(long)]
    out: Option<PathBuf>,
}

#[derive(Clone, Debug, Default, Serialize)]
struct TimingSample {
    spans: HashMap<String, f64>,
    detect_ms: f64,
}

#[derive(Debug, Serialize)]
struct StageReport {
    name: String,
    summary: SummaryStats,
    /// p50 as a percentage of the `detect_inner` total.
    pct_of_detect_p50: f64,
}

#[derive(Debug, Serialize)]
struct Workload {
    raw_corners: usize,
    labelled_corners: usize,
    observed_edges: usize,
    board_rows: u32,
    board_cols: u32,
    search_mode: String,
    scoring_mode: String,
}

/// Provenance for the report, so a number can be traced to the build and
/// machine that produced it.
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
    width: u32,
    height: u32,
    workload: Workload,
    detect: SummaryStats,
    stages: Vec<StageReport>,
    /// `detect_inner` minus the stages it directly contains — unlabelled work.
    unattributed_p50_ms: f64,
    samples: Vec<TimingSample>,
}

fn measure_once(
    detector: &PuzzleBoardDetector,
    img: &image::GrayImage,
    corners: &[calib_targets::chessboard::ChessCorner],
    totals: &calib_targets_bench::span_timing::SpanTotals,
) -> TimingSample {
    totals.clear();
    let view = gray_view(img);
    let start = Instant::now();
    let _ = detector.detect(&view, corners);
    let detect_ms = start.elapsed().as_secs_f64() * 1000.0;
    let raw = totals.snapshot_ms();
    TimingSample {
        spans: STAGE_SPANS
            .iter()
            .map(|&name| (name.to_owned(), span_ms(&raw, name)))
            .collect(),
        detect_ms,
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let totals = TimingLayer::install()?;

    let spec =
        PuzzleBoardSpec::with_origin(args.rows, args.cols, 5.0, args.origin_row, args.origin_col)?;
    let mut params = PuzzleBoardParams::for_board(spec);
    params.decode.search_mode = match args.search {
        SearchArg::Full => PuzzleBoardSearchMode::Full,
        SearchArg::Fixed => PuzzleBoardSearchMode::FixedBoard,
    };
    params.decode.scoring_mode = match args.scoring {
        ScoringArg::Hard => PuzzleBoardScoringMode::HardWeighted,
        ScoringArg::Soft => PuzzleBoardScoringMode::SoftLogLikelihood,
    };
    let detector = PuzzleBoardDetector::new(params)?;

    let img = ImageReader::open(&args.image)?.decode()?.to_luma8();
    let (width, height) = (img.width(), img.height());

    // Corner detection is detector *input*: run it once, outside the timed
    // region, so the repeats measure only `detect`.
    let corners = detect_corners(&img, &default_chess_config());

    let view = gray_view(&img);
    let (result, diagnostics) = detector.detect_with_diagnostics(&view, &corners);
    let labelled_corners = match &result {
        Ok(res) => res.corners.len(),
        Err(e) => {
            eprintln!("warning: detect failed on {}: {e}", args.image.display());
            0
        }
    };

    for _ in 0..args.warmup {
        let _ = measure_once(&detector, &img, &corners, &totals);
    }
    let samples: Vec<TimingSample> = (0..args.repeats)
        .map(|_| measure_once(&detector, &img, &corners, &totals))
        .collect();

    let detect = summarize(samples.iter().map(|s| s.detect_ms).collect());
    let stage_stats = |name: &str| {
        summarize(
            samples
                .iter()
                .map(|s| s.spans.get(name).copied().unwrap_or(0.0))
                .collect(),
        )
    };
    let total_p50 = stage_stats("detect_inner").p50_ms.max(f64::MIN_POSITIVE);
    let stages: Vec<StageReport> = STAGE_SPANS
        .iter()
        .map(|&name| {
            let summary = stage_stats(name);
            StageReport {
                name: name.to_owned(),
                pct_of_detect_p50: 100.0 * summary.p50_ms / total_p50,
                summary,
            }
        })
        .collect();
    // `chess_detect_all` and `decode_component` are the direct children of
    // `detect_inner`; whatever is left is unlabelled work in the parent.
    let unattributed_p50_ms =
        total_p50 - stage_stats("chess_detect_all").p50_ms - stage_stats("decode_component").p50_ms;

    println!(
        "\n{} ({}×{})  search={:?} scoring={:?}  corners={} labelled={} edges={}",
        args.image.display(),
        width,
        height,
        args.search,
        args.scoring,
        corners.len(),
        labelled_corners,
        diagnostics.observed_edges.len(),
    );
    println!("\n| stage | p50 ms | p95 ms | % of detect |");
    println!("|---|---|---|---|");
    for stage in &stages {
        println!(
            "| {} | {:.3} | {:.3} | {:.1}% |",
            stage.name, stage.summary.p50_ms, stage.summary.p95_ms, stage.pct_of_detect_p50
        );
    }
    println!(
        "| _unattributed_ | {unattributed_p50_ms:.3} | | {:.1}% |",
        100.0 * unattributed_p50_ms / total_p50
    );

    let report = Report {
        metadata: Metadata {
            git_sha: command_output("git", &["rev-parse", "HEAD"]),
            rustc: command_output("rustc", &["--version"]),
            cpu: cpu_name(),
            profile: if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
            repeats: args.repeats,
            warmup: args.warmup,
            timing_source: "tracing span busy time",
        },
        image: args.image.display().to_string(),
        width,
        height,
        workload: Workload {
            raw_corners: corners.len(),
            labelled_corners,
            observed_edges: diagnostics.observed_edges.len(),
            board_rows: args.rows,
            board_cols: args.cols,
            search_mode: format!("{:?}", args.search),
            scoring_mode: format!("{:?}", args.scoring),
        },
        detect,
        stages,
        unattributed_p50_ms,
        samples,
    };
    let out = args
        .out
        .unwrap_or_else(|| PathBuf::from("bench_results/puzzleboard_stage_timing.json"));
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&out, serde_json::to_string_pretty(&report)?)?;
    println!("\nreport → {}", out.display());
    Ok(())
}
