//! `POST /api/diagnose` — per-stage pipeline introspection for one snap.
//!
//! The two graph-build algorithms expose different diagnostics by
//! construction: seed-and-grow has the full `DebugFrame` (per-corner stage
//! cursors + iteration traces), the topological path has the
//! labelled-vs-unlabelled breakdown with pre-filter survival counts. The
//! response is a discriminated union on `"kind"`.

use axum::extract::State;
use axum::Json;
use calib_targets::chessboard::{Detector, GraphBuildAlgorithm};
use calib_targets::detect::{default_chess_config, detect_corners};
use calib_targets_bench::diagnose::{diagnose_topological, stage_counts_by_name};
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
    /// Seed-and-grow pipeline → full `DebugFrame`.
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
    let mut params = calib_targets_bench::config::merge_detector_params(&req.params)
        .map_err(|e| ApiError::BadRequest(format!("invalid params: {e}")))?;
    // Mirror the bench CLI exactly: the topological diagnosis forces the
    // production topological path; the seed-and-grow diagnosis runs
    // `detect_with_diagnostics` with the caller's params untouched (the
    // DebugFrame pipeline is seed-and-grow by construction, but stages
    // like the geometry check still consult `graph_build_algorithm`).
    if req.algorithm == AlgorithmReq::Topological {
        params.graph_build_algorithm = GraphBuildAlgorithm::Topological;
    }
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
    let algorithm = req.algorithm;

    let value = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, ApiError> {
        let img = load_entry_image(&abs)?;
        let fed = calib_targets_bench::runner::fed_image(&img, &entry, k)?;
        let corners = detect_corners(&fed, &chess_cfg);
        match algorithm {
            AlgorithmReq::Topological => {
                let diagnosis = diagnose_topological(&params, &corners);
                Ok(json!({ "kind": "topological", "diagnosis": diagnosis }))
            }
            AlgorithmReq::SeedAndGrow => {
                let detector = Detector::new(params)
                    .map_err(|e| ApiError::BadRequest(format!("invalid params: {e}")))?;
                let frame = detector.detect_with_diagnostics(&corners);
                let stage_counts = stage_counts_by_name(&frame);
                Ok(json!({
                    "kind": "seed_and_grow",
                    "frame": frame,
                    "stage_counts": stage_counts,
                }))
            }
        }
    })
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))??;

    Ok(Json(value))
}
