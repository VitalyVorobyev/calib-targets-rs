//! Named detector-config CRUD. Configs are raw partial-`DetectorParams`
//! JSON files in the gitignored `<workspace_root>/studio_configs/` directory
//! — the same format the bench CLI's `--chessboard-config` accepts, so a
//! config saved in the studio works verbatim on the command line.

use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use axum::extract::Path;
use axum::http::StatusCode;
use axum::Json;
use calib_targets::chessboard::{AdvancedTuning, DetectorParams};
use calib_targets_bench::config::merge_detector_params;
use serde::Serialize;

use crate::error::ApiError;

fn configs_dir() -> PathBuf {
    calib_targets_bench::workspace_root().join("studio_configs")
}

fn config_path(name: &str) -> Result<PathBuf, ApiError> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        return Err(ApiError::BadRequest(
            "config name must be 1-64 chars of [a-z0-9_-]".into(),
        ));
    }
    Ok(configs_dir().join(format!("{name}.json")))
}

/// One row of the config listing.
#[derive(Serialize)]
pub struct ConfigSummary {
    /// Config name (file stem under `studio_configs/`).
    pub name: String,
    /// Unix seconds of the file's last modification.
    pub modified_at: u64,
    /// Effective `graph_build_algorithm` after merging over defaults.
    pub algorithm: String,
    /// Whether the file overrides any `advanced` tuning.
    pub has_advanced: bool,
}

/// `GET /api/configs` — list saved configs (newest first).
pub async fn list() -> Result<Json<Vec<ConfigSummary>>, ApiError> {
    let dir = configs_dir();
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return Ok(Json(out)), // no directory yet → empty list
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let Ok(merged) = merge_detector_params(&value) else {
            continue;
        };
        let modified_at = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        out.push(ConfigSummary {
            name: name.to_string(),
            modified_at,
            algorithm: format!("{:?}", merged.graph_build_algorithm).to_lowercase(),
            has_advanced: value.get("advanced").is_some(),
        });
    }
    out.sort_by_key(|c| std::cmp::Reverse(c.modified_at));
    Ok(Json(out))
}

/// `GET /api/configs/_defaults` — fully materialised defaults (stable params
/// + every `advanced` knob) so the config form never hardcodes Rust values.
pub async fn defaults() -> Result<Json<serde_json::Value>, ApiError> {
    let full = DetectorParams::default().with_advanced(AdvancedTuning::default());
    serde_json::to_value(&full)
        .map(Json)
        .map_err(|e| ApiError::Internal(e.to_string()))
}

/// `GET /api/configs/{name}` — the raw saved override object.
pub async fn get(Path(name): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    let path = config_path(&name)?;
    let text = std::fs::read_to_string(&path)
        .map_err(|_| ApiError::NotFound(format!("no config named {name}")))?;
    serde_json::from_str(&text)
        .map(Json)
        .map_err(|e| ApiError::Internal(format!("corrupt config {name}: {e}")))
}

/// `PUT /api/configs/{name}` — validate and save an override object.
pub async fn put(
    Path(name): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<StatusCode, ApiError> {
    let path = config_path(&name)?;
    if !body.is_object() {
        return Err(ApiError::BadRequest(
            "config body must be a JSON object of DetectorParams overrides".into(),
        ));
    }
    // Reject overrides that do not merge into valid params.
    merge_detector_params(&body)
        .map_err(|e| ApiError::BadRequest(format!("invalid config: {e}")))?;
    std::fs::create_dir_all(configs_dir())?;
    let text =
        serde_json::to_string_pretty(&body).map_err(|e| ApiError::Internal(e.to_string()))?;
    std::fs::write(&path, text)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /api/configs/{name}`.
pub async fn delete(Path(name): Path<String>) -> Result<StatusCode, ApiError> {
    let path = config_path(&name)?;
    std::fs::remove_file(&path)
        .map_err(|_| ApiError::NotFound(format!("no config named {name}")))?;
    Ok(StatusCode::NO_CONTENT)
}
