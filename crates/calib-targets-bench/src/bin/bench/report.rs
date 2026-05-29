//! Per-image / summary report types and the run-vs-baseline diff, summary,
//! and serialization helpers shared by the `run`/`check` subcommands.

use std::path::{Path, PathBuf};

use calib_targets_bench::baseline::Baseline;
use calib_targets_bench::diff::BaselineDiff;
use calib_targets_bench::runner::RunOutcome;
use calib_targets_bench::workspace_root;
use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct PerImageReport {
    pub(crate) image: String,
    pub(crate) passed: bool,
    pub(crate) has_baseline: bool,
    pub(crate) elapsed_ms: f64,
    pub(crate) labelled_count: usize,
    pub(crate) diff_vs_baseline: BaselineDiff,
}

#[derive(Serialize)]
pub(crate) struct Summary {
    pub(crate) images_total: usize,
    pub(crate) images_passed: usize,
    pub(crate) images_failed: usize,
    pub(crate) p50_ms: f64,
    pub(crate) p95_ms: f64,
    pub(crate) max_ms: f64,
}

#[derive(Serialize)]
pub(crate) struct RunReport {
    pub(crate) schema: u32,
    pub(crate) detector: &'static str,
    pub(crate) config_id: String,
    pub(crate) summary: Summary,
    pub(crate) per_image: Vec<PerImageReport>,
}

pub(crate) fn compute_report(outcome: &RunOutcome, baseline: Option<&Baseline>) -> PerImageReport {
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

pub(crate) fn make_summary(per_image: &[PerImageReport], elapsed: &[f64]) -> Summary {
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

pub(crate) fn print_summary(report: &RunReport) -> std::io::Result<()> {
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

pub(crate) fn save_report(report: &RunReport, path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text =
        serde_json::to_string_pretty(report).map_err(|e| std::io::Error::other(e.to_string()))?;
    std::fs::write(path, text)
}

pub(crate) fn bench_results_dir() -> PathBuf {
    workspace_root().join("bench_results")
}
