//! `POST /api/detect` — run the chessboard detector on one snap with
//! caller-supplied (partial) params and return corners + baseline diff.

use axum::extract::State;
use axum::Json;
use calib_targets::chessboard::DetectorParams;
use calib_targets::detect::{default_chess_config, OrientationMethod};
use calib_targets_bench::baseline::{Baseline, BaselineImage};
use calib_targets_bench::config::merge_detector_params;
use calib_targets_bench::diff::BaselineDiff;
use calib_targets_bench::runner::{load_entry_image, run_snap};
use calib_targets_bench::Engine;
use serde::{Deserialize, Serialize};

use super::AppState;
use crate::error::ApiError;
use crate::snaps::resolve_label;

/// Detection engine selector (wire form of [`Engine`]).
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineReq {
    /// Full chessboard production pipeline.
    #[default]
    Pipeline,
    /// Raw projective-grid builder (orientation-source head-to-head).
    Grid,
}

impl From<EngineReq> for Engine {
    fn from(v: EngineReq) -> Self {
        match v {
            EngineReq::Pipeline => Engine::Pipeline,
            EngineReq::Grid => Engine::Grid,
        }
    }
}

/// ChESS axis-fit method selector (wire form of [`OrientationMethod`]).
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OrientationMethodReq {
    /// Upstream ring-sample fit (default).
    #[default]
    RingFit,
    /// Disk-sector fit (more accurate, slower).
    DiskFit,
}

impl From<OrientationMethodReq> for OrientationMethod {
    fn from(v: OrientationMethodReq) -> Self {
        match v {
            OrientationMethodReq::RingFit => OrientationMethod::RingFit,
            OrientationMethodReq::DiskFit => OrientationMethod::DiskFit,
        }
    }
}

/// Request body for `POST /api/detect`.
#[derive(Deserialize)]
pub struct DetectRequest {
    /// Snap label (`path` or `path#k`).
    pub label: String,
    /// Detection engine (default: pipeline).
    #[serde(default)]
    pub engine: EngineReq,
    /// Partial [`DetectorParams`] override (same merge semantics as the
    /// bench CLI's `--chessboard-config`). Empty object = defaults.
    #[serde(default)]
    pub params: serde_json::Value,
    /// ChESS axis-fit method (default: ring_fit).
    #[serde(default)]
    pub orientation_method: OrientationMethodReq,
    /// Whether to diff the detection against the pinned baseline.
    #[serde(default = "default_true")]
    pub compare_baseline: bool,
}

fn default_true() -> bool {
    true
}

/// Image dimensions of the fed image (the coordinate frame of all corners).
#[derive(Serialize)]
pub struct ImageDims {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// Baseline comparison block of a detect response.
#[derive(Serialize)]
pub struct BaselineBlock {
    /// Whether a baseline entry exists for this label.
    pub exists: bool,
    /// Structured diff (present only when a baseline exists).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff: Option<BaselineDiff>,
}

/// Response body for `POST /api/detect`.
#[derive(Serialize)]
pub struct DetectResponse {
    /// Wall-clock detection time (corner detect + grid build), milliseconds.
    pub elapsed_ms: f64,
    /// Fed-image dimensions.
    pub image: ImageDims,
    /// Detection result (`null` when no chessboard was found).
    pub detection: Option<BaselineImage>,
    /// Baseline comparison (omitted when `compare_baseline` is false).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline: Option<BaselineBlock>,
}

/// Resolve and validate the effective [`DetectorParams`] from a partial
/// override object. Pipeline-engine params go through
/// [`DetectorParams::validate`] (which rejects the unsupported
/// seed-and-grow + neighbour-edges cell); the grid engine accepts all
/// algorithm × orientation-source combinations, matching the bench CLI.
pub fn effective_params(
    params: &serde_json::Value,
    engine: Engine,
) -> Result<DetectorParams, ApiError> {
    let merged = merge_detector_params(params)
        .map_err(|e| ApiError::BadRequest(format!("invalid params: {e}")))?;
    if engine == Engine::Pipeline {
        merged
            .validate()
            .map_err(|e| ApiError::BadRequest(format!("invalid params: {e}")))?;
    }
    Ok(merged)
}

/// `POST /api/detect` handler.
pub async fn detect(
    State(state): State<AppState>,
    Json(req): Json<DetectRequest>,
) -> Result<Json<DetectResponse>, ApiError> {
    let engine = Engine::from(req.engine);
    let params = effective_params(&req.params, engine)?;
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

    let outcome = tokio::task::spawn_blocking(move || {
        let img = load_entry_image(&abs)?;
        run_snap(&img, &entry, k, &params, &chess_cfg, engine)
    })
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))??;

    let (w, h) = outcome.fed_image.dimensions();
    let baseline = req.compare_baseline.then(|| {
        let (entry, _) = resolve_label(&state.dataset, &req.label).expect("label resolved above");
        let bank = Baseline::load_or_empty(entry.kind);
        match (bank.images.get(&req.label), &outcome.detection) {
            (Some(bi), Some(run)) => BaselineBlock {
                exists: true,
                diff: Some(BaselineDiff::compute(bi, &run.corners)),
            },
            (Some(bi), None) => {
                let mut d = BaselineDiff::default();
                for c in &bi.corners {
                    d.missing_labels.push([c.i, c.j]);
                }
                BaselineBlock {
                    exists: true,
                    diff: Some(d),
                }
            }
            (None, _) => BaselineBlock {
                exists: false,
                diff: None,
            },
        }
    });

    Ok(Json(DetectResponse {
        elapsed_ms: outcome.elapsed_ms,
        image: ImageDims {
            width: w,
            height: h,
        },
        detection: outcome.detection,
        baseline,
    }))
}
