//! `cargo bench-{run,check,bless,preview}` — see top-level docs in the
//! library crate for the schema and workflow.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use calib_targets::chessboard::{CornerStage, DetectorParams};
use calib_targets::detect::{default_chess_config, detect_corners};
use calib_targets_bench::baseline::Baseline;
use calib_targets_bench::dataset::{Dataset, DatasetEntry, ImageKind};
use calib_targets_bench::diff::BaselineDiff;
use calib_targets_bench::overlay::{render_diagnose_overlay, render_overlay_on_gray};
use calib_targets_bench::runner::{run_entry, RunOutcome};
use calib_targets_bench::{workspace_root, SCHEMA_VERSION};
use image::imageops::FilterType;
use image::{GenericImageView, ImageReader};

use clap::{Args, Parser, Subcommand};
use serde::Serialize;

#[derive(Parser)]
#[command(
    name = "bench",
    about = "chessboard grid-builder regression / performance harness"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the detector on every image and write a JSON report to bench_results/.
    Run(RunArgs),
    /// Run + diff against baselines. Exit non-zero on any regression.
    Check(RunArgs),
    /// Render PNG overlays under preview/ for visual inspection.
    Preview(PreviewArgs),
    /// Pin the current detector output as the new baseline.
    Bless(BlessArgs),
    /// Print per-stage corner counts and render a per-stage diagnostic
    /// overlay. Use this to investigate "why is this corner missing?"
    /// before changing detector code.
    Diagnose(DiagnoseArgs),
}

#[derive(Args)]
struct DiagnoseArgs {
    /// Image path (relative to workspace root). Stitched composites accept
    /// a `#k` suffix to pick one sub-snap, e.g.
    /// `privatedata/130x130_puzzle/target_15.png#3`.
    image: String,
    /// Output overlay path (default: `preview/diagnose/<stem>.png`).
    #[arg(long)]
    out: Option<String>,
    /// Optional path to dump the full `DebugFrame` (cluster histogram +
    /// per-corner stages + iteration traces) as JSON for offline triage.
    /// Local-only output; do not commit.
    #[arg(long)]
    dump_frame: Option<String>,
}

#[derive(Args)]
struct RunArgs {
    /// Restrict to one kind of image set.
    #[arg(long, value_enum)]
    dataset: Option<DatasetKindArg>,
    /// Restrict to a single image (relative path under workspace root).
    #[arg(long)]
    image: Option<String>,
}

#[derive(Args)]
struct PreviewArgs {
    /// Output directory (relative to workspace root).
    #[arg(long, default_value = "preview")]
    out: String,
    /// Restrict to one kind of image set.
    #[arg(long, value_enum)]
    dataset: Option<DatasetKindArg>,
    /// Restrict to a single image.
    #[arg(long)]
    image: Option<String>,
    /// Render every image, even when the dataset filter / image filter would skip.
    #[arg(long)]
    all: bool,
}

#[derive(Args)]
struct BlessArgs {
    /// Image to bless (relative path, e.g. `testdata/mid.png`). Pass --all to bless every entry instead.
    #[arg(long)]
    image: Option<String>,
    /// Bless every entry of the chosen kind.
    #[arg(long)]
    all: bool,
    /// Restrict --all to one kind of image set.
    #[arg(long, value_enum)]
    dataset: Option<DatasetKindArg>,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum DatasetKindArg {
    Public,
    Private,
}

impl From<DatasetKindArg> for ImageKind {
    fn from(v: DatasetKindArg) -> Self {
        match v {
            DatasetKindArg::Public => ImageKind::Public,
            DatasetKindArg::Private => ImageKind::Private,
        }
    }
}

// --- report types ----------------------------------------------------------

#[derive(Serialize)]
struct PerImageReport {
    image: String,
    passed: bool,
    has_baseline: bool,
    elapsed_ms: f64,
    labelled_count: usize,
    diff_vs_baseline: BaselineDiff,
}

#[derive(Serialize)]
struct Summary {
    images_total: usize,
    images_passed: usize,
    images_failed: usize,
    p50_ms: f64,
    p95_ms: f64,
    max_ms: f64,
}

#[derive(Serialize)]
struct RunReport {
    schema: u32,
    detector: &'static str,
    config_id: &'static str,
    summary: Summary,
    per_image: Vec<PerImageReport>,
}

// --- command implementations ----------------------------------------------

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Run(args) => cmd_run(args, false),
        Cmd::Check(args) => cmd_run(args, true),
        Cmd::Preview(args) => cmd_preview(args),
        Cmd::Bless(args) => cmd_bless(args),
        Cmd::Diagnose(args) => cmd_diagnose(args),
    }
}

fn cmd_run(args: RunArgs, fail_on_diff: bool) -> ExitCode {
    let dataset = match Dataset::load_default() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("load datasets.toml: {e}");
            return ExitCode::from(2);
        }
    };
    let kind = args.dataset.map(ImageKind::from);
    let entries = filter_entries(&dataset, kind, args.image.as_deref());
    if entries.is_empty() {
        eprintln!("no images matched the filter");
        return ExitCode::from(2);
    }

    let baselines: BTreeMap<ImageKind, Baseline> = [
        (
            ImageKind::Public,
            Baseline::load_or_empty(ImageKind::Public),
        ),
        (
            ImageKind::Private,
            Baseline::load_or_empty(ImageKind::Private),
        ),
    ]
    .into_iter()
    .collect();

    let params = DetectorParams::default();
    let mut per_image = Vec::with_capacity(entries.len());
    let mut elapsed: Vec<f64> = Vec::with_capacity(entries.len());

    for entry in &entries {
        let abs = entry.absolute();
        if !abs.exists() {
            eprintln!(
                "skipping {} — file missing (private dataset not provisioned?)",
                entry.path
            );
            continue;
        }
        let outcomes = match run_entry(&abs, entry, &params) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("run_entry {}: {e}", entry.path);
                continue;
            }
        };
        for outcome in outcomes {
            let report = compute_report(&outcome, baselines.get(&entry.kind));
            elapsed.push(report.elapsed_ms);
            per_image.push(report);
        }
    }

    let summary = make_summary(&per_image, &elapsed);
    let report = RunReport {
        schema: SCHEMA_VERSION,
        detector: "chessboard",
        config_id: "default",
        summary,
        per_image,
    };

    if let Err(e) = print_summary(&report) {
        eprintln!("print summary: {e}");
    }

    let report_path = bench_results_dir().join("chessboard.json");
    if let Err(e) = save_report(&report, &report_path) {
        eprintln!("save report: {e}");
    } else {
        println!("wrote report → {}", report_path.display());
    }

    if fail_on_diff && report.summary.images_failed > 0 {
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn cmd_preview(args: PreviewArgs) -> ExitCode {
    let dataset = match Dataset::load_default() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("load datasets.toml: {e}");
            return ExitCode::from(2);
        }
    };
    let kind = if args.all {
        None
    } else {
        args.dataset.map(ImageKind::from)
    };
    let image_filter = if args.all {
        None
    } else {
        args.image.as_deref()
    };
    let entries = filter_entries(&dataset, kind, image_filter);
    if entries.is_empty() {
        eprintln!("no images matched the filter");
        return ExitCode::from(2);
    }

    let out_root = workspace_root().join(&args.out);
    let params = DetectorParams::default();
    let mut wrote = 0usize;
    for entry in &entries {
        let abs = entry.absolute();
        if !abs.exists() {
            eprintln!(
                "skipping {} — file missing (private dataset not provisioned?)",
                entry.path
            );
            continue;
        }
        let outcomes = match run_entry(&abs, entry, &params) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("run_entry {}: {e}", entry.path);
                continue;
            }
        };
        for outcome in outcomes {
            let labelled = outcome
                .detection
                .as_ref()
                .map(|d| d.labelled_count)
                .unwrap_or(0);
            let dst = preview_path(&out_root, &outcome.label);
            if let Err(e) =
                render_overlay_on_gray(&outcome.fed_image, outcome.detection.as_ref(), &dst)
            {
                eprintln!("overlay {}: {e}", outcome.label);
                continue;
            }
            wrote += 1;
            println!(
                "{:<60} {:>4} corners {:>7.1} ms  →  {}",
                outcome.label,
                labelled,
                outcome.elapsed_ms,
                dst.strip_prefix(workspace_root()).unwrap_or(&dst).display()
            );
        }
    }
    println!("\nwrote {wrote} overlays under {}", args.out);
    ExitCode::SUCCESS
}

fn cmd_bless(args: BlessArgs) -> ExitCode {
    if args.all == args.image.is_some() {
        eprintln!("pass exactly one of --image <path> or --all");
        return ExitCode::from(2);
    }
    let dataset = match Dataset::load_default() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("load datasets.toml: {e}");
            return ExitCode::from(2);
        }
    };
    let kind = args.dataset.map(ImageKind::from);
    let entries: Vec<&DatasetEntry> = if let Some(image) = &args.image {
        let Some(entry) = dataset.find(image) else {
            eprintln!("image {image} is not in datasets.toml");
            return ExitCode::from(2);
        };
        vec![entry]
    } else {
        filter_entries(&dataset, kind, None)
    };

    let mut public = Baseline::load_or_empty(ImageKind::Public);
    let mut private = Baseline::load_or_empty(ImageKind::Private);
    let params = DetectorParams::default();
    let mut blessed = 0usize;
    for entry in &entries {
        let abs = entry.absolute();
        if !abs.exists() {
            eprintln!("skipping {} — file missing", entry.path);
            continue;
        }
        let outcomes = match run_entry(&abs, entry, &params) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("run_entry {}: {e}", entry.path);
                continue;
            }
        };
        for outcome in outcomes {
            let Some(det) = outcome.detection else {
                eprintln!(
                    "refusing to bless {} — detector produced no detection",
                    outcome.label
                );
                continue;
            };
            let bucket = match entry.kind {
                ImageKind::Public => &mut public,
                ImageKind::Private => &mut private,
            };
            bucket.images.insert(outcome.label.clone(), det);
            blessed += 1;
            println!("blessed {}", outcome.label);
        }
    }

    let mut wrote: Vec<PathBuf> = Vec::new();
    for (kind, baseline) in [(ImageKind::Public, &public), (ImageKind::Private, &private)] {
        if baseline.images.is_empty() {
            continue;
        }
        match baseline.save(kind) {
            Ok(p) => wrote.push(p),
            Err(e) => eprintln!("save baseline {kind:?}: {e}"),
        }
    }

    println!("\nblessed {blessed} entries");
    for p in wrote {
        println!("  → {}", p.display());
    }
    ExitCode::SUCCESS
}

fn cmd_diagnose(args: DiagnoseArgs) -> ExitCode {
    let dataset = match Dataset::load_default() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("load datasets.toml: {e}");
            return ExitCode::from(2);
        }
    };
    // Parse `path#k` form.
    let (base_path, sub_idx): (&str, Option<u32>) = match args.image.rsplit_once('#') {
        Some((b, s)) => (b, s.parse().ok()),
        None => (args.image.as_str(), None),
    };

    // Find the matching dataset entry; if absent, build a default one for the path.
    let entry = match dataset.find(base_path) {
        Some(e) => e.clone(),
        None => DatasetEntry {
            path: base_path.to_string(),
            kind: ImageKind::Public,
            note: String::new(),
            upscale: 1,
            stitched: None,
        },
    };
    let abs = entry.absolute();
    if !abs.exists() {
        eprintln!("file not found: {}", abs.display());
        return ExitCode::from(2);
    }

    let img = match ImageReader::open(&abs).and_then(|r| r.decode().map_err(std::io::Error::other))
    {
        Ok(d) => d.to_luma8(),
        Err(e) => {
            eprintln!("decode {}: {e}", abs.display());
            return ExitCode::from(2);
        }
    };

    let snap = if let (Some(spec), Some(k)) = (entry.stitched.as_ref(), sub_idx) {
        let x0 = k * spec.snap_width;
        img.view(x0, 0, spec.snap_width, spec.snap_height)
            .to_image()
    } else {
        img
    };
    let upscaled = if entry.upscale > 1 {
        let (w, h) = snap.dimensions();
        image::imageops::resize(
            &snap,
            w * entry.upscale,
            h * entry.upscale,
            FilterType::Triangle,
        )
    } else {
        snap
    };

    let chess_cfg = default_chess_config();
    let corners = detect_corners(&upscaled, &chess_cfg);
    let detector_params = DetectorParams::default();
    let detector = calib_targets::chessboard::Detector::new(detector_params);
    let frame = detector.detect_debug(&corners);

    print_stage_summary(&args.image, &frame);

    // Also probe how many components `detect_all` recovers — useful when a
    // ChArUco split produces several disjoint chessboard subgraphs that the
    // single-best `detect()` call hides.
    let detector_for_all = calib_targets::chessboard::Detector::new(DetectorParams::default());
    let all_frames = detector_for_all.detect_all_debug(&corners);
    if all_frames.len() > 1 {
        println!("\n  --- detect_all_debug ---");
        for (k, f) in all_frames.iter().enumerate() {
            let labelled = f
                .detection
                .as_ref()
                .map(|d| d.target.corners.len())
                .unwrap_or(0);
            println!("  component {k}: labelled={labelled}");
        }
    }

    let label = if sub_idx.is_some() {
        args.image.clone()
    } else {
        base_path.to_string()
    };
    let dst = args.out.as_deref().map_or_else(
        || {
            workspace_root()
                .join("preview/diagnose")
                .join(diagnose_filename(&label))
        },
        |p| workspace_root().join(p),
    );
    if let Err(e) = render_diagnose_overlay(&upscaled, &frame, &dst) {
        eprintln!("render diagnose overlay: {e}");
        return ExitCode::from(2);
    }
    println!(
        "\nwrote diagnose overlay → {}",
        dst.strip_prefix(workspace_root()).unwrap_or(&dst).display()
    );

    if let Some(dump_path) = args.dump_frame.as_deref() {
        let dump_dst = workspace_root().join(dump_path);
        if let Some(parent) = dump_dst.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                eprintln!("create dump-frame parent dir: {e}");
                return ExitCode::from(2);
            }
        }
        let json = match serde_json::to_string_pretty(&frame) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("serialize debug frame: {e}");
                return ExitCode::from(2);
            }
        };
        if let Err(e) = std::fs::write(&dump_dst, json) {
            eprintln!("write debug frame to {}: {e}", dump_dst.display());
            return ExitCode::from(2);
        }
        println!(
            "wrote debug frame → {}",
            dump_dst
                .strip_prefix(workspace_root())
                .unwrap_or(&dump_dst)
                .display()
        );
    }

    ExitCode::SUCCESS
}

fn print_stage_summary(label: &str, frame: &calib_targets::chessboard::DebugFrame) {
    let mut counts: std::collections::BTreeMap<&'static str, usize> =
        std::collections::BTreeMap::new();
    for aug in &frame.corners {
        let key: &'static str = match &aug.stage {
            CornerStage::Raw => "Raw",
            CornerStage::Strong => "Strong",
            CornerStage::NoCluster { .. } => "NoCluster",
            CornerStage::Clustered { .. } => "Clustered",
            CornerStage::AttachmentAmbiguous { .. } => "AttachmentAmbiguous",
            CornerStage::AttachmentFailedInvariants { .. } => "AttachmentFailedInvariants",
            CornerStage::Labeled { .. } => "Labeled",
            CornerStage::LabeledThenBlacklisted { .. } => "LabeledThenBlacklisted",
            _ => "Other",
        };
        *counts.entry(key).or_insert(0) += 1;
    }
    println!("--- {label} ---");
    println!("  input corners: {}", frame.input_count);
    for (k, v) in &counts {
        println!("  {k:>30}: {v}");
    }
    if !frame.iterations.is_empty() {
        println!("  --- validation iterations ---");
        for it in &frame.iterations {
            println!(
                "  iter {}: labelled={} new_blacklist={} converged={}",
                it.iter,
                it.labelled_count,
                it.new_blacklist.len(),
                it.converged
            );
            if let Some(ext) = &it.extension {
                let med = ext
                    .h_residual_median_px
                    .map(|v| format!("{v:.2}"))
                    .unwrap_or_else(|| "—".to_string());
                let max = ext
                    .h_residual_max_px
                    .map(|v| format!("{v:.2}"))
                    .unwrap_or_else(|| "—".to_string());
                println!(
                    "    stage6: h_trusted={} median_res={} px max_res={} px iters={} attached={} \
                     rej(no_cand={} ambig={} label={} validator={} edge={})",
                    ext.h_trusted,
                    med,
                    max,
                    ext.iterations,
                    ext.attached,
                    ext.rejected_no_candidate,
                    ext.rejected_ambiguous,
                    ext.rejected_label,
                    ext.rejected_validator,
                    ext.rejected_edge,
                );
            }
        }
    }
    if let Some(b) = &frame.boosters {
        println!("  boosters: {b:?}");
    }
    if let Some(d) = &frame.detection {
        println!(
            "  detection: {} labelled corners, cell_size = {:.2} px",
            d.target.corners.len(),
            d.cell_size
        );
        // Print bbox of labelled set.
        let mut min_i = i32::MAX;
        let mut max_i = i32::MIN;
        let mut min_j = i32::MAX;
        let mut max_j = i32::MIN;
        for lc in &d.target.corners {
            if let Some(g) = lc.grid {
                min_i = min_i.min(g.i);
                max_i = max_i.max(g.i);
                min_j = min_j.min(g.j);
                max_j = max_j.max(g.j);
            }
        }
        if min_i != i32::MAX {
            println!(
                "  labelled bbox: i ∈ [{min_i}, {max_i}], j ∈ [{min_j}, {max_j}]  ({}×{})",
                max_i - min_i + 1,
                max_j - min_j + 1,
            );
        }
    } else {
        println!("  detection: NONE");
    }
}

fn diagnose_filename(label: &str) -> String {
    let safe = label.replace(['/', '#'], "_");
    format!("{safe}.diagnose.png")
}

// --- helpers --------------------------------------------------------------

fn filter_entries<'a>(
    ds: &'a Dataset,
    kind: Option<ImageKind>,
    image: Option<&str>,
) -> Vec<&'a DatasetEntry> {
    ds.iter_kind(kind)
        .filter(|e| image.map(|i| e.path == i).unwrap_or(true))
        .collect()
}

fn compute_report(outcome: &RunOutcome, baseline: Option<&Baseline>) -> PerImageReport {
    let baseline_image = baseline.and_then(|b| b.images.get(&outcome.label));
    let labelled_count = outcome
        .detection
        .as_ref()
        .map(|d| d.labelled_count)
        .unwrap_or(0);

    let diff = match (&outcome.detection, baseline_image) {
        (Some(run), Some(bi)) => BaselineDiff::compute(bi, &run.corners),
        (Some(run), None) => {
            // No baseline yet: every run-corner is "extra".
            let mut d = BaselineDiff::default();
            for c in &run.corners {
                d.extra_labels.push([c.i, c.j]);
            }
            d
        }
        (None, Some(bi)) => {
            // Lost detection that the baseline expected: every baseline corner is missing.
            let mut d = BaselineDiff::default();
            for c in &bi.corners {
                d.missing_labels.push([c.i, c.j]);
            }
            d
        }
        (None, None) => BaselineDiff::default(),
    };

    let has_baseline = baseline_image.is_some();
    let passed = has_baseline && diff.passed();
    PerImageReport {
        image: outcome.label.clone(),
        passed,
        has_baseline,
        elapsed_ms: outcome.elapsed_ms,
        labelled_count,
        diff_vs_baseline: diff,
    }
}

fn make_summary(per_image: &[PerImageReport], elapsed: &[f64]) -> Summary {
    let total = per_image.len();
    let passed = per_image.iter().filter(|r| r.passed).count();
    let failed = per_image
        .iter()
        .filter(|r| r.has_baseline && !r.passed)
        .count();
    let mut sorted = elapsed.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p50 = pct(&sorted, 0.50);
    let p95 = pct(&sorted, 0.95);
    let maxv = sorted.last().copied().unwrap_or(0.0);
    Summary {
        images_total: total,
        images_passed: passed,
        images_failed: failed,
        p50_ms: p50,
        p95_ms: p95,
        max_ms: maxv,
    }
}

fn pct(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() - 1) as f64 * q).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn print_summary(report: &RunReport) -> std::io::Result<()> {
    println!("\n--- per-image -----------------------------------------------------------");
    for r in &report.per_image {
        let d = &r.diff_vs_baseline;
        let status = if !r.has_baseline {
            "NO-BASELINE"
        } else if r.passed {
            if d.extra_labels.is_empty() {
                "PASS"
            } else {
                "PASS+"
            }
        } else {
            "FAIL"
        };
        let dup = d.duplicate_run_positions.len();
        println!(
            "{status:<11} {:<50} {:>4} corners {:>7.1} ms  miss={:>3} extra={:>3} pos={:>3} id={:>3} dup={:>3}{}",
            r.image,
            r.labelled_count,
            r.elapsed_ms,
            d.missing_labels.len(),
            d.extra_labels.len(),
            d.wrong_position.len(),
            d.wrong_id.len(),
            dup,
            if d.inconsistent_shift { "  SHIFT-INCONSISTENT" } else { "" },
        );
    }
    let improvements: usize = report
        .per_image
        .iter()
        .filter(|r| r.passed && !r.diff_vs_baseline.extra_labels.is_empty())
        .map(|r| r.diff_vs_baseline.extra_labels.len())
        .sum();
    println!("\n--- summary -------------------------------------------------------------");
    println!(
        "total={} passed={} failed={} improvements=+{}  p50={:.1} ms  p95={:.1} ms  max={:.1} ms",
        report.summary.images_total,
        report.summary.images_passed,
        report.summary.images_failed,
        improvements,
        report.summary.p50_ms,
        report.summary.p95_ms,
        report.summary.max_ms,
    );
    Ok(())
}

fn save_report(report: &RunReport, path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text =
        serde_json::to_string_pretty(report).map_err(|e| std::io::Error::other(e.to_string()))?;
    std::fs::write(path, text)
}

fn bench_results_dir() -> PathBuf {
    workspace_root().join("bench_results")
}

fn preview_path(out_root: &Path, label: &str) -> PathBuf {
    let (base, sub) = match label.rsplit_once('#') {
        Some((b, s)) => (b, Some(s)),
        None => (label, None),
    };
    let stem = Path::new(base)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("preview");
    let parent_dir = Path::new(base)
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or("");
    let mirror = if parent_dir.is_empty() {
        out_root.to_path_buf()
    } else {
        out_root.join(parent_dir)
    };
    let filename = match sub {
        Some(s) => format!("{stem}.{s}.chessboard.png"),
        None => format!("{stem}.chessboard.png"),
    };
    mirror.join(filename)
}
