//! The `accuracy` subcommand: run the ground-truth accuracy suite
//! ([`calib_targets_bench::accuracy`]) and write its report.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use calib_targets_bench::accuracy::families::Family;
use calib_targets_bench::accuracy::{
    full_conditions, markdown, quick_conditions, run, summarize, Refiner, Report, SUPERSAMPLE,
};
use calib_targets_bench::report::bench_results_dir;
use calib_targets_bench::workspace_root;

use crate::cli::AccuracyArgs;

pub(crate) fn cmd_accuracy(args: AccuracyArgs) -> ExitCode {
    let families: Vec<Family> = if args.family.is_empty() {
        Family::ALL.to_vec()
    } else {
        args.family.iter().map(|&f| f.into()).collect()
    };
    let refiners: Vec<Refiner> = if args.refiner.is_empty() {
        Refiner::ALL.to_vec()
    } else {
        args.refiner.iter().map(|&r| r.into()).collect()
    };
    let conditions = if args.quick {
        quick_conditions(&families)
    } else {
        full_conditions(&families)
    };
    let threads = args.threads.unwrap_or_else(|| {
        std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get)
    });
    println!(
        "accuracy: {} conditions × {} refiner(s) on {threads} thread(s)",
        conditions.len(),
        refiners.len()
    );
    let started = Instant::now();
    let rows = run(&conditions, &refiners, threads);
    let elapsed = started.elapsed().as_secs_f64();
    let report = Report {
        grid: if args.quick { "quick" } else { "full" }.to_string(),
        supersample: SUPERSAMPLE,
        summary: summarize(&rows),
        rows,
    };
    let md = markdown(&report);
    // The summary table is the first section.
    let head: String = md.split("\n## ").next().unwrap_or_default().to_string();
    println!("{head}\nran in {elapsed:.1} s");

    let dir = match args.out.as_deref() {
        Some(p) if PathBuf::from(p).is_absolute() => PathBuf::from(p),
        Some(p) => workspace_root().join(p),
        None => bench_results_dir().join("accuracy"),
    };
    let stem = if args.quick {
        "accuracy_quick"
    } else {
        "accuracy"
    };
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("create {}: {e}", dir.display());
        return ExitCode::from(2);
    }
    let json = match serde_json::to_string_pretty(&report) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("serialize report: {e}");
            return ExitCode::from(2);
        }
    };
    for (ext, body) in [("md", md.as_str()), ("json", json.as_str())] {
        let path = dir.join(format!("{stem}.{ext}"));
        if let Err(e) = std::fs::write(&path, body) {
            eprintln!("write {}: {e}", path.display());
            return ExitCode::from(2);
        }
        println!("wrote {}", path.display());
    }

    let wrong: usize = report.summary.iter().map(|s| s.wrong_labels).sum();
    if wrong > 0 {
        eprintln!(
            "PRECISION CONTRACT VIOLATED: {wrong} wrong label(s) — see the per-condition table"
        );
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
