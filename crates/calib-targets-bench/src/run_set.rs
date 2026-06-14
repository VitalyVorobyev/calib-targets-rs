//! Run a single [`DetectorParams`] config over a set of dataset entries and
//! fold the per-image outcomes into a [`RunReport`].
//!
//! This is the shared core of `bench run` and `bench ablate`: both must
//! measure the *same thing* (identical detection loop, identical per-image
//! reporting), so the loop lives here once rather than being duplicated in
//! each subcommand. The CLI plumbing (dataset loading, entry filtering,
//! report saving) stays in the binary; this function takes the already-built
//! inputs and returns the report.

use std::collections::BTreeMap;

use calib_targets::chessboard::DetectorParams;
use calib_targets::detect::DetectorConfig;

use crate::baseline::Baseline;
use crate::dataset::{DatasetEntry, ImageKind};
use crate::report::{compute_report, make_summary, PerImageReport, RunReport};
use crate::runner::{run_entry, Engine};
use crate::SCHEMA_VERSION;

/// Inputs shared across every entry in one run, grouped to keep
/// [`run_report_for_params`]'s argument count low (the workspace denies
/// `too_many_arguments`).
pub struct RunContext<'a> {
    /// Low-level ChESS corner detector config (e.g. `orientation_method`).
    pub chess_cfg: &'a DetectorConfig,
    /// Which detection engine to drive (pipeline vs raw grid).
    pub engine: Engine,
    /// Baselines per image kind, used only to populate the per-image diff;
    /// the run itself does not depend on a baseline being present.
    pub baselines: &'a BTreeMap<ImageKind, Baseline>,
}

/// Run `params` over every existing `entry` and assemble a [`RunReport`].
///
/// Missing on-disk files (e.g. an unprovisioned private dataset) are skipped
/// with a stderr note, matching `bench run`'s behaviour. `config_id` is
/// stamped onto the returned report verbatim.
pub fn run_report_for_params(
    entries: &[&DatasetEntry],
    params: &DetectorParams,
    ctx: &RunContext<'_>,
    config_id: String,
) -> RunReport {
    let mut per_image: Vec<PerImageReport> = Vec::with_capacity(entries.len());
    let mut elapsed: Vec<f64> = Vec::with_capacity(entries.len());

    for &entry in entries {
        let abs = entry.absolute();
        if !abs.exists() {
            eprintln!(
                "skipping {} — file missing (private dataset not provisioned?)",
                entry.path
            );
            continue;
        }
        let outcomes = match run_entry(&abs, entry, params, ctx.chess_cfg, ctx.engine) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("run_entry {}: {e}", entry.path);
                continue;
            }
        };
        for outcome in outcomes {
            let report = compute_report(&outcome, ctx.baselines.get(&entry.kind));
            elapsed.push(report.elapsed_ms);
            per_image.push(report);
        }
    }

    let summary = make_summary(&per_image, &elapsed);
    RunReport {
        schema: SCHEMA_VERSION,
        detector: "chessboard".to_string(),
        config_id,
        summary,
        per_image,
    }
}
