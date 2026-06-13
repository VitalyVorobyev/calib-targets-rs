//! Dataset-run endpoints: launch a bench-style run over the manifest,
//! poll progress + partial per-image results, list past runs. Read-only
//! with respect to baselines — there is deliberately no bless endpoint.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use calib_targets::detect::default_chess_config;
use calib_targets_bench::dataset::ImageKind;
use calib_targets_bench::Engine;
use serde::Deserialize;

use super::detect::{effective_params, EngineReq, OrientationMethodReq};
use super::AppState;
use crate::error::ApiError;
use crate::jobs::{launch, select_entries, RunRecord, RunSpec};

/// Dataset filter for a run.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatasetReq {
    /// Public `testdata/` entries only.
    Public,
    /// Private `privatedata/` entries only.
    Private,
    /// Every available entry.
    #[default]
    All,
}

impl DatasetReq {
    fn kind(self) -> Option<ImageKind> {
        match self {
            DatasetReq::Public => Some(ImageKind::Public),
            DatasetReq::Private => Some(ImageKind::Private),
            DatasetReq::All => None,
        }
    }

    fn slug(self) -> &'static str {
        match self {
            DatasetReq::Public => "public",
            DatasetReq::Private => "private",
            DatasetReq::All => "all",
        }
    }
}

/// Request body for `POST /api/runs`.
#[derive(Deserialize)]
pub struct RunRequest {
    /// Dataset filter (default: all). Ignored when `group` is set.
    #[serde(default)]
    pub dataset: DatasetReq,
    /// Scope the run to a single dataset group (e.g. `130x130_puzzle`). When
    /// set, overrides the kind filter so the run covers exactly that dataset.
    #[serde(default)]
    pub group: Option<String>,
    /// Partial `DetectorParams` override (CLI merge semantics).
    #[serde(default)]
    pub params: serde_json::Value,
    /// Detection engine (default: pipeline).
    #[serde(default)]
    pub engine: EngineReq,
    /// ChESS axis-fit method (default: ring_fit).
    #[serde(default)]
    pub orientation_method: OrientationMethodReq,
}

/// `POST /api/runs` — launch a run (202) or refuse when one is active (409).
pub async fn create(
    State(state): State<AppState>,
    Json(req): Json<RunRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let engine = Engine::from(req.engine);
    let params = effective_params(&req.params, engine)?;
    // A group scopes to one dataset across kinds; otherwise filter by kind.
    let kind = if req.group.is_some() {
        None
    } else {
        req.dataset.kind()
    };
    let entries = select_entries(&state.dataset, kind, req.group.as_deref());
    if entries.is_empty() {
        return Err(ApiError::NotFound(
            "no available images match the dataset filter".into(),
        ));
    }
    let mut chess_cfg = default_chess_config();
    chess_cfg.orientation_method = req.orientation_method.into();
    let config_id = format!(
        "{}.{:?}.{:?}",
        match engine {
            Engine::Pipeline => "pipeline",
            Engine::Grid => "grid",
        },
        params.graph_build_algorithm,
        req.orientation_method,
    )
    .to_lowercase();

    let spec = RunSpec {
        entries,
        params,
        chess_cfg,
        engine,
        config_id,
        dataset: req
            .group
            .clone()
            .unwrap_or_else(|| req.dataset.slug().to_string()),
    };
    match launch(&state.runs, spec) {
        Some(run_id) => Ok((
            StatusCode::ACCEPTED,
            Json(serde_json::json!({ "run_id": run_id })),
        )),
        None => Err(ApiError::Conflict(
            "a run is already in progress — wait for it to finish".into(),
        )),
    }
}

/// `GET /api/runs` — all runs of this server session, newest first.
pub async fn list(State(state): State<AppState>) -> Json<Vec<RunRecord>> {
    Json(state.runs.lock().expect("runs lock").list())
}

/// `GET /api/runs/{id}` — one run with partial per-image rows while live.
pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RunRecord>, ApiError> {
    state
        .runs
        .lock()
        .expect("runs lock")
        .get(&id)
        .map(Json)
        .ok_or_else(|| ApiError::NotFound(format!("no run {id}")))
}
