//! `POST /api/diagnose` — per-stage pipeline introspection for one snap.
//!
//! The detector builds its grid with the topological path (the only builder),
//! whose diagnosis is the labelled-vs-unlabelled breakdown with pre-filter
//! survival counts. The response is a discriminated union on `"kind"`.
//!
//! The legacy seed-and-grow `DebugFrame` diagnosis has been retired together
//! with the seed-and-grow builder; a request asking for it returns a clear
//! error rather than fabricated data.

use axum::extract::State;
use axum::Json;
use calib_targets::chessboard::GraphBuildAlgorithm;
use calib_targets::detect::{default_chess_config, detect_corners};
use calib_targets_bench::diagnose::diagnose_topological;
use calib_targets_bench::runner::load_entry_image;
use serde::Deserialize;
use serde_json::json;

use super::detect::OrientationMethodReq;
use super::AppState;
use crate::error::ApiError;
use crate::snaps::resolve_label;

/// Graph-build algorithm selector for diagnose requests.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AlgorithmReq {
    /// Production topological path → labelled/unlabelled diagnosis.
    #[default]
    Topological,
    /// Retired seed-and-grow `DebugFrame` diagnosis. Accepted for request
    /// back-compat but no longer serviced; returns a clear error.
    SeedAndGrow,
}

/// Request body for `POST /api/diagnose`.
#[derive(Deserialize)]
pub struct DiagnoseRequest {
    /// Snap label (`path` or `path#k`).
    pub label: String,
    /// Which algorithm to diagnose.
    #[serde(default)]
    pub algorithm: AlgorithmReq,
    /// Partial `DetectorParams` override (CLI merge semantics).
    #[serde(default)]
    pub params: serde_json::Value,
    /// ChESS axis-fit method (default: ring_fit).
    #[serde(default)]
    pub orientation_method: OrientationMethodReq,
}

/// `POST /api/diagnose` handler.
pub async fn diagnose(
    State(state): State<AppState>,
    Json(req): Json<DiagnoseRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // The seed-and-grow per-stage `DebugFrame` diagnosis was retired with the
    // seed-and-grow builder; refuse it with a clear error rather than
    // fabricating data. A topological per-stage analog is a separate future
    // addition.
    if req.algorithm == AlgorithmReq::SeedAndGrow {
        return Err(ApiError::BadRequest(
            "seed-and-grow per-stage diagnostics have been retired (topological analog pending); \
             request algorithm \"topological\" instead"
                .to_string(),
        ));
    }

    let mut params = calib_targets_bench::config::merge_detector_params(&req.params)
        .map_err(|e| ApiError::BadRequest(format!("invalid params: {e}")))?;
    // The topological builder is the only builder; force it explicitly.
    params.graph_build_algorithm = GraphBuildAlgorithm::Topological;
    params
        .validate()
        .map_err(|e| ApiError::BadRequest(format!("invalid params: {e}")))?;

    let (entry, k) = resolve_label(&state.dataset, &req.label)?;
    let entry = entry.clone();
    let abs = entry.absolute();
    if !abs.exists() {
        return Err(ApiError::NotFound(format!(
            "{} is missing on disk (private dataset not provisioned?)",
            entry.path
        )));
    }
    let mut chess_cfg = default_chess_config();
    chess_cfg.orientation_method = req.orientation_method.into();

    let value = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, ApiError> {
        let img = load_entry_image(&abs)?;
        let fed = calib_targets_bench::runner::fed_image(&img, &entry, k)?;
        let corners = detect_corners(&fed, &chess_cfg);
        let diagnosis = diagnose_topological(&params, &corners);
        Ok(json!({ "kind": "topological", "diagnosis": diagnosis }))
    })
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))??;

    Ok(Json(value))
}
