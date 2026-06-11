//! Dataset-run job executor: one bench-style run at a time, mirroring the
//! bench CLI's `cmd_run` loop (`run_entry` → `compute_report` per snap),
//! with progress and partial results readable while the run is live.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use calib_targets::chessboard::DetectorParams;
use calib_targets::detect::DetectorConfig;
use calib_targets_bench::baseline::Baseline;
use calib_targets_bench::dataset::{Dataset, DatasetEntry, ImageKind};
use calib_targets_bench::report::{compute_report, make_summary, PerImageReport, Summary};
use calib_targets_bench::runner::run_entry;
use calib_targets_bench::Engine;
use serde::Serialize;

/// Lifecycle of one dataset run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    /// Worker is processing entries.
    Running,
    /// All entries processed; `summary` is final.
    Done,
    /// Worker aborted (I/O error); `error` carries the message.
    Failed,
}

/// Progress counter of a live run.
#[derive(Clone, Debug, Serialize)]
pub struct RunProgress {
    /// Logical snaps completed so far.
    pub done: usize,
    /// Total logical snaps scheduled.
    pub total: usize,
    /// Entry currently being processed (`None` once finished).
    pub current: Option<String>,
}

/// Full state of one dataset run (cloned out under lock for responses).
#[derive(Clone, Debug, Serialize)]
pub struct RunRecord {
    /// Run identifier (`run-N`).
    pub id: String,
    /// Lifecycle status.
    pub status: RunStatus,
    /// Unix seconds when the run was launched.
    pub started_at: u64,
    /// Human-readable config slug (engine.algorithm.method.source).
    pub config_id: String,
    /// Dataset filter the run was launched with.
    pub dataset: String,
    /// Progress counters.
    pub progress: RunProgress,
    /// Per-image rows, growing while the run is live.
    pub per_image: Vec<PerImageReport>,
    /// Aggregate summary (present once done).
    pub summary: Option<Summary>,
    /// Failure message when `status == Failed`.
    pub error: Option<String>,
}

/// Registry of all runs this server session. Holds at most one live run.
#[derive(Default)]
pub struct RunRegistry {
    next_id: u64,
    runs: BTreeMap<String, RunRecord>,
}

/// Shared handle to the registry.
pub type SharedRuns = Arc<Mutex<RunRegistry>>;

impl RunRegistry {
    /// `true` when a run is currently executing.
    pub fn has_active(&self) -> bool {
        self.runs.values().any(|r| r.status == RunStatus::Running)
    }

    /// All runs, newest first.
    pub fn list(&self) -> Vec<RunRecord> {
        let mut v: Vec<_> = self.runs.values().cloned().collect();
        v.sort_by(|a, b| b.started_at.cmp(&a.started_at).then(b.id.cmp(&a.id)));
        v
    }

    /// Snapshot of one run.
    pub fn get(&self, id: &str) -> Option<RunRecord> {
        self.runs.get(id).cloned()
    }
}

/// Everything a worker needs to execute one run.
pub struct RunSpec {
    /// Entries to process (already filtered + availability-checked).
    pub entries: Vec<DatasetEntry>,
    /// Effective detector params.
    pub params: DetectorParams,
    /// ChESS corner-detector config (orientation method applied).
    pub chess_cfg: DetectorConfig,
    /// Detection engine.
    pub engine: Engine,
    /// Config slug for display.
    pub config_id: String,
    /// Dataset filter slug for display.
    pub dataset: String,
}

/// Filter the manifest by kind, keeping only entries present on disk.
pub fn select_entries(dataset: &Dataset, kind: Option<ImageKind>) -> Vec<DatasetEntry> {
    dataset
        .iter_kind(kind)
        .filter(|e| e.absolute().exists())
        .cloned()
        .collect()
}

/// Register a new run and spawn its worker. Returns the run id, or `None`
/// when another run is already active.
pub fn launch(runs: &SharedRuns, spec: RunSpec) -> Option<String> {
    let total: usize = spec.entries.iter().map(|e| e.snap_count() as usize).sum();
    let id = {
        let mut reg = runs.lock().expect("runs lock");
        if reg.has_active() {
            return None;
        }
        reg.next_id += 1;
        let id = format!("run-{}", reg.next_id);
        let started_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        reg.runs.insert(
            id.clone(),
            RunRecord {
                id: id.clone(),
                status: RunStatus::Running,
                started_at,
                config_id: spec.config_id.clone(),
                dataset: spec.dataset.clone(),
                progress: RunProgress {
                    done: 0,
                    total,
                    current: None,
                },
                per_image: Vec::new(),
                summary: None,
                error: None,
            },
        );
        id
    };

    let runs = runs.clone();
    let worker_id = id.clone();
    tokio::task::spawn_blocking(move || execute(&runs, &worker_id, spec));
    Some(id)
}

fn execute(runs: &SharedRuns, id: &str, spec: RunSpec) {
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

    let mut elapsed: Vec<f64> = Vec::new();
    for entry in &spec.entries {
        update(runs, id, |r| {
            r.progress.current = Some(entry.path.clone());
        });
        let outcomes = match run_entry(
            &entry.absolute(),
            entry,
            &spec.params,
            &spec.chess_cfg,
            spec.engine,
        ) {
            Ok(v) => v,
            Err(e) => {
                update(runs, id, |r| {
                    r.status = RunStatus::Failed;
                    r.error = Some(format!("{}: {e}", entry.path));
                    r.progress.current = None;
                });
                return;
            }
        };
        for outcome in outcomes {
            let report = compute_report(&outcome, baselines.get(&entry.kind));
            elapsed.push(report.elapsed_ms);
            update(runs, id, |r| {
                r.per_image.push(report.clone());
                r.progress.done += 1;
            });
        }
    }

    update(runs, id, |r| {
        r.summary = Some(make_summary(&r.per_image, &elapsed));
        r.status = RunStatus::Done;
        r.progress.current = None;
    });
}

fn update(runs: &SharedRuns, id: &str, f: impl FnOnce(&mut RunRecord)) {
    let mut reg = runs.lock().expect("runs lock");
    if let Some(r) = reg.runs.get_mut(id) {
        f(r);
    }
}
