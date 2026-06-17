//! `cargo bench-{run,check,bless,preview}` — see top-level docs in the
//! library crate for the schema and workflow.
//!
//! Module layout:
//!
//! - [`cli`] — the `clap` argument tree and value-enum knobs.
//! - [`report`] — per-image/summary report types + the baseline-diff,
//!   summary, and serialization helpers used by `run`/`check`.
//! - [`diagnose`] — the `diagnose` subcommand (per-stage breakdown +
//!   diagnostic overlays, including the topological path).
//! - this file — the entry point and the `run`/`preview`/`bless`/`compare`/
//!   `ablate` subcommands plus their small shared helpers. `run` and `ablate`
//!   share the per-config run loop via `calib_targets_bench::run_set`.

mod cli;
mod diagnose;
mod report;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use std::collections::BTreeMap;

use calib_targets::chessboard::DetectorParams;
use calib_targets::detect::default_chess_config;
use calib_targets_bench::ablate::{render_ablation_markdown, run_ablation, AblationOpts};
use calib_targets_bench::baseline::Baseline;
use calib_targets_bench::dataset::{Dataset, DatasetEntry, ImageKind};
use calib_targets_bench::overlay::render_overlay_on_gray;
use calib_targets_bench::run_set::{run_report_for_params, RunContext};
use calib_targets_bench::runner::run_entry;
use calib_targets_bench::{workspace_root, Engine};

use calib_targets_bench::compare::{build_comparison, load_report, render_markdown};
use calib_targets_bench::report::{bench_results_dir, save_report};
use cli::{
    load_chessboard_config, params_with, AblateArgs, AlgorithmArg, BlessArgs, Cli, Cmd,
    CompareArgs, EngineArg, OrientationSourceArg, PreviewArgs, RunArgs,
};
use diagnose::cmd_diagnose;
use report::print_summary;

use clap::Parser;

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Run(args) => cmd_run(args, false),
        Cmd::Check(args) => cmd_run(args, true),
        Cmd::Preview(args) => cmd_preview(args),
        Cmd::Bless(args) => cmd_bless(args),
        Cmd::Diagnose(args) => cmd_diagnose(args),
        Cmd::Compare(args) => cmd_compare(args),
        Cmd::Ablate(args) => cmd_ablate(args),
    }
}

fn cmd_run(args: RunArgs, fail_on_diff: bool) -> ExitCode {
    if unsupported_combo(args.engine, args.algorithm, args.orientation_source) {
        eprintln!(
            "pipeline + seed-and-grow + neighbour-edges is unsupported; use \
             --engine grid for neighbour-edge seed-and-grow, or --algorithm topological"
        );
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

    let mut params = match load_chessboard_config(args.chessboard_config.as_deref()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("load --chessboard-config: {e}");
            return ExitCode::from(2);
        }
    };
    params.graph_build_algorithm = args.algorithm.into();
    params.orientation_source = args.orientation_source.into();
    let engine = Engine::from(args.engine);
    let mut chess_cfg = default_chess_config();
    chess_cfg.orientation_method = args.orientation_method.into();

    let config_id = format!(
        "{}.{}.{}.{}",
        args.engine.slug(),
        args.algorithm.slug(),
        args.orientation_method.slug(),
        args.orientation_source.slug()
    );
    let ctx = RunContext {
        chess_cfg: &chess_cfg,
        engine,
        baselines: &baselines,
    };
    let report = run_report_for_params(&entries, &params, &ctx, config_id.clone());

    if let Err(e) = print_summary(&report) {
        eprintln!("print summary: {e}");
    }

    let report_path = bench_results_dir().join(format!("chessboard.{config_id}.json"));
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
    if unsupported_combo(args.engine, args.algorithm, args.orientation_source) {
        eprintln!(
            "pipeline + seed-and-grow + neighbour-edges is unsupported; use \
             --engine grid for neighbour-edge seed-and-grow, or --algorithm topological"
        );
        return ExitCode::from(2);
    }
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
    let params = params_with(args.algorithm, args.orientation_source);
    let engine = Engine::from(args.engine);
    let config_slug = format!(
        "{}.{}.{}.{}",
        args.engine.slug(),
        args.algorithm.slug(),
        args.orientation_method.slug(),
        args.orientation_source.slug()
    );
    let mut chess_cfg = default_chess_config();
    chess_cfg.orientation_method = args.orientation_method.into();
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
        let outcomes = match run_entry(&abs, entry, &params, &chess_cfg, engine) {
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
            let dst = preview_path(&out_root, &outcome.label, &config_slug);
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
    let chess_cfg = default_chess_config();
    let mut blessed = 0usize;
    for entry in &entries {
        let abs = entry.absolute();
        if !abs.exists() {
            eprintln!("skipping {} — file missing", entry.path);
            continue;
        }
        // Baselines are pinned from the production pipeline only.
        let outcomes = match run_entry(&abs, entry, &params, &chess_cfg, Engine::Pipeline) {
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

fn cmd_compare(args: CompareArgs) -> ExitCode {
    let a_path = resolve_report_path(&args.a);
    let b_path = resolve_report_path(&args.b);
    let a = match load_report(&a_path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("load {}: {e}", a_path.display());
            return ExitCode::from(2);
        }
    };
    let b = match load_report(&b_path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("load {}: {e}", b_path.display());
            return ExitCode::from(2);
        }
    };

    let comparison = build_comparison(&a, &b);
    let markdown = render_markdown(&comparison);
    print!("{markdown}");

    let stem = compare_out_stem(args.out.as_deref(), &a.config_id, &b.config_id);
    let md_path = append_ext(&stem, ".md");
    let json_path = append_ext(&stem, ".json");
    if let Some(parent) = md_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            eprintln!("create {}: {e}", parent.display());
            return ExitCode::from(2);
        }
    }
    let json = match serde_json::to_string_pretty(&comparison) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("serialize comparison: {e}");
            return ExitCode::from(2);
        }
    };
    if let Err(e) = std::fs::write(&md_path, &markdown) {
        eprintln!("write {}: {e}", md_path.display());
        return ExitCode::from(2);
    }
    if let Err(e) = std::fs::write(&json_path, json) {
        eprintln!("write {}: {e}", json_path.display());
        return ExitCode::from(2);
    }
    println!("\nwrote {}", md_path.display());
    println!("wrote {}", json_path.display());
    ExitCode::SUCCESS
}

fn cmd_ablate(args: AblateArgs) -> ExitCode {
    if unsupported_combo(args.engine, args.algorithm, args.orientation_source) {
        eprintln!(
            "pipeline + seed-and-grow + neighbour-edges is unsupported; use \
             --engine grid for neighbour-edge seed-and-grow, or --algorithm topological"
        );
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
    let mut entries = filter_entries(&dataset, kind, args.image.as_deref());
    if let Some(group) = args.group.as_deref() {
        entries.retain(|e| e.dataset == group);
    }
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

    let mut base = match load_chessboard_config(args.chessboard_config.as_deref()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("load --chessboard-config: {e}");
            return ExitCode::from(2);
        }
    };
    base.graph_build_algorithm = args.algorithm.into();
    base.orientation_source = args.orientation_source.into();
    let engine = Engine::from(args.engine);
    let mut chess_cfg = default_chess_config();
    chess_cfg.orientation_method = args.orientation_method.into();
    let base_config_id = format!(
        "{}.{}.{}.{}",
        args.engine.slug(),
        args.algorithm.slug(),
        args.orientation_method.slug(),
        args.orientation_source.slug()
    );
    let dataset_filter = args
        .group
        .clone()
        .or_else(|| args.image.clone())
        .or_else(|| kind.map(|k| format!("{k:?}").to_lowercase()))
        .unwrap_or_else(|| "all".to_string());

    let stem = match args.out.as_deref() {
        Some(o) => {
            let p = Path::new(o);
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                workspace_root().join(o)
            }
        }
        None => bench_results_dir().join("ablation"),
    };

    let opts = AblationOpts {
        rel: args.rel,
        bool_only: args.bool_only,
        scalars_only: args.scalars_only,
        only: args.only.clone(),
        dataset_filter,
        base_config_id,
        dump_runs: args.dump_runs.then(|| append_ext(&stem, "_runs")),
    };

    let ctx = RunContext {
        chess_cfg: &chess_cfg,
        engine,
        baselines: &baselines,
    };
    let run = |params: &DetectorParams, config_id: String| {
        run_report_for_params(&entries, params, &ctx, config_id)
    };

    let report = match run_ablation(&base, &opts, run) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("ablation run: {e}");
            return ExitCode::from(2);
        }
    };

    let markdown = render_ablation_markdown(&report);
    print!("{markdown}");

    let md_path = append_ext(&stem, ".md");
    let json_path = append_ext(&stem, ".json");
    if let Some(parent) = md_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            eprintln!("create {}: {e}", parent.display());
            return ExitCode::from(2);
        }
    }
    let json = match serde_json::to_string_pretty(&report) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("serialize ablation: {e}");
            return ExitCode::from(2);
        }
    };
    if let Err(e) = std::fs::write(&md_path, &markdown) {
        eprintln!("write {}: {e}", md_path.display());
        return ExitCode::from(2);
    }
    if let Err(e) = std::fs::write(&json_path, json) {
        eprintln!("write {}: {e}", json_path.display());
        return ExitCode::from(2);
    }
    println!("\nwrote {}", md_path.display());
    println!("wrote {}", json_path.display());
    ExitCode::SUCCESS
}

// --- helpers --------------------------------------------------------------

/// Resolve a report path: use it as given when it exists, else fall back to
/// `bench_results/<path>`.
fn resolve_report_path(p: &str) -> PathBuf {
    let direct = PathBuf::from(p);
    if direct.exists() {
        direct
    } else {
        bench_results_dir().join(p)
    }
}

/// The output stem for a comparison: the explicit `--out` (workspace-relative
/// unless absolute), else `bench_results/compare.<a_alg>_vs_<b_alg>`.
fn compare_out_stem(out: Option<&str>, a_config_id: &str, b_config_id: &str) -> PathBuf {
    match out {
        Some(o) => {
            let p = Path::new(o);
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                workspace_root().join(o)
            }
        }
        None => {
            let alg = |id: &str| id.split('.').nth(1).unwrap_or(id).to_string();
            bench_results_dir().join(format!(
                "compare.{}_vs_{}",
                alg(a_config_id),
                alg(b_config_id)
            ))
        }
    }
}

/// Append a literal extension (e.g. `.md`) to a stem path that may itself
/// contain dots — `Path::with_extension` would clobber the last segment.
fn append_ext(stem: &Path, ext: &str) -> PathBuf {
    let mut s = stem.to_path_buf().into_os_string();
    s.push(ext);
    PathBuf::from(s)
}

fn filter_entries<'a>(
    ds: &'a Dataset,
    kind: Option<ImageKind>,
    image: Option<&str>,
) -> Vec<&'a DatasetEntry> {
    ds.iter_kind(kind)
        .filter(|e| image.map(|i| e.path == i).unwrap_or(true))
        .collect()
}

/// The native seed-and-grow pipeline consumes ChESS axes directly, and a
/// measured head-to-head (2026-06-17) confirmed feeding it synthesized
/// neighbour-edge axes collapses recall (0 corners on most clutter-free
/// frames), so `pipeline + seed-and-grow + neighbour-edges` stays a typed error
/// in the detector. Reject it at the CLI with guidance instead. The grid engine
/// handles the seed-and-grow + neighbour-edge cell for measurement.
fn unsupported_combo(
    engine: EngineArg,
    algorithm: AlgorithmArg,
    orientation_source: OrientationSourceArg,
) -> bool {
    matches!(engine, EngineArg::Pipeline)
        && matches!(algorithm, AlgorithmArg::SeedAndGrow)
        && matches!(orientation_source, OrientationSourceArg::NeighbourEdges)
}

fn preview_path(out_root: &Path, label: &str, config_slug: &str) -> PathBuf {
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
        Some(s) => format!("{stem}.{s}.chessboard.{config_slug}.png"),
        None => format!("{stem}.chessboard.{config_slug}.png"),
    };
    mirror.join(filename)
}
