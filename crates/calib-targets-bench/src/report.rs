//! Per-image / summary report types plus the run-vs-baseline diff and
//! serialization helpers shared by the bench CLI (`run`/`check`) and any
//! programmatic consumer (e.g. the studio server).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::baseline::Baseline;
use crate::diff::BaselineDiff;
use crate::runner::RunOutcome;
use crate::workspace_root;

/// One row of a run report: a single (logical) image's pass/fail status,
/// timing, and diff against its baseline.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PerImageReport {
    /// Snap label (baseline JSON key), e.g. `testdata/mid.png` or
    /// `privatedata/<dataset>/target_15.png#3`.
    pub image: String,
    /// `true` when a baseline exists and the diff is clean.
    pub passed: bool,
    /// Whether a baseline entry exists for this label.
    pub has_baseline: bool,
    /// Wall-clock detection time (corner detect + grid build), milliseconds.
    pub elapsed_ms: f64,
    /// Number of labelled corners in the detection (0 when none).
    pub labelled_count: usize,
    /// Structured diff against the baseline (empty when no baseline).
    pub diff_vs_baseline: BaselineDiff,
}

/// Aggregate counters + latency percentiles over a whole run.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Summary {
    /// Total images (logical snaps) processed.
    pub images_total: usize,
    /// Images that have a baseline and matched it.
    pub images_passed: usize,
    /// Images that have a baseline and diverged from it.
    pub images_failed: usize,
    /// Median per-image detection latency, milliseconds.
    pub p50_ms: f64,
    /// 95th-percentile per-image detection latency, milliseconds.
    pub p95_ms: f64,
    /// Worst per-image detection latency, milliseconds.
    pub max_ms: f64,
}

/// Top-level run report written to `bench_results/`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunReport {
    /// Report schema version ([`crate::SCHEMA_VERSION`]).
    pub schema: u32,
    /// Detector family, currently always `"chessboard"`.
    pub detector: String,
    /// Config slug, e.g. `pipeline.topological.ring_fit.chess_axes`.
    pub config_id: String,
    /// Aggregate counters + latency percentiles.
    pub summary: Summary,
    /// One entry per logical image (stitched snaps count individually).
    pub per_image: Vec<PerImageReport>,
}

/// Diff one [`RunOutcome`] against the (optional) baseline set and fold the
/// result into a [`PerImageReport`].
pub fn compute_report(outcome: &RunOutcome, baseline: Option<&Baseline>) -> PerImageReport {
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

/// Fold per-image rows + their latencies into a [`Summary`].
pub fn make_summary(per_image: &[PerImageReport], elapsed: &[f64]) -> Summary {
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

/// Serialize a [`RunReport`] as pretty JSON to `path`, creating parent
/// directories as needed.
pub fn save_report(report: &RunReport, path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text =
        serde_json::to_string_pretty(report).map_err(|e| std::io::Error::other(e.to_string()))?;
    std::fs::write(path, text)
}

/// `<workspace_root>/bench_results` — where run reports land (gitignored).
pub fn bench_results_dir() -> PathBuf {
    workspace_root().join("bench_results")
}
