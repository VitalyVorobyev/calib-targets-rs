//! `POST /api/diagnose` — per-stage pipeline introspection for one snap.
//!
//! The detector builds its grid with the topological path (the only builder),
//! whose diagnosis is the labelled-vs-unlabelled breakdown with pre-filter
//! survival counts. The response is a discriminated union on `"kind"`.

use axum::extract::State;
use axum::Json;
use calib_targets::detect::{default_chess_config, detect_corners};
use calib_targets_bench::diagnose::diagnose_topological;
use calib_targets_bench::runner::load_entry_image;
use serde::Deserialize;
use serde_json::json;

use super::detect::OrientationMethodReq;
use super::AppState;
use crate::error::ApiError;
use crate::snaps::resolve_label;

/// Request body for `POST /api/diagnose`.
#[derive(Deserialize)]
pub struct DiagnoseRequest {
    /// Snap label (`path` or `path#k`).
    pub label: String,
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
    let params = calib_targets_bench::config::merge_detector_params(&req.params)
        .map_err(|e| ApiError::BadRequest(format!("invalid params: {e}")))?;
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
