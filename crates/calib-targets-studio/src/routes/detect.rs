//! `POST /api/detect` — run the chessboard detector on one snap with
//! caller-supplied (partial) params and return corners + baseline diff.

use axum::extract::State;
use axum::Json;
use calib_targets::chessboard::DetectorParams;
use calib_targets::detect::{default_chess_config, OrientationMethod};
use calib_targets_bench::baseline::{Baseline, BaselineCorner, BaselineImage};
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

/// Target family selector for detect requests.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DetectorReq {
    /// Plain chessboard (the only family with pinned baselines).
    #[default]
    Chessboard,
    /// ChArUco fusion (requires a `board` spec).
    Charuco,
    /// PuzzleBoard self-identifying chessboard (requires a `board` spec).
    Puzzleboard,
}

/// Board geometry for ChArUco / PuzzleBoard requests.
#[derive(Clone, Debug, Deserialize)]
pub struct BoardReq {
    /// Squares vertically.
    pub rows: u32,
    /// Squares horizontally.
    pub cols: u32,
    /// Square side in board units (mm), default 1.0.
    #[serde(default = "default_cell_size")]
    pub cell_size: f32,
    /// ChArUco: marker side as a fraction of the square side.
    #[serde(default = "default_marker_size_rel")]
    pub marker_size_rel: f32,
    /// ChArUco: builtin ArUco dictionary name (e.g. `DICT_4X4_50`).
    #[serde(default = "default_dictionary")]
    pub dictionary: String,
    /// PuzzleBoard: row offset into the 501×501 master pattern.
    #[serde(default)]
    pub origin_row: u32,
    /// PuzzleBoard: column offset into the 501×501 master pattern.
    #[serde(default)]
    pub origin_col: u32,
}

fn default_cell_size() -> f32 {
    1.0
}

fn default_marker_size_rel() -> f32 {
    0.75
}

fn default_dictionary() -> String {
    "DICT_4X4_50".to_string()
}

/// Request body for `POST /api/detect`.
#[derive(Deserialize)]
pub struct DetectRequest {
    /// Snap label (`path` or `path#k`).
    pub label: String,
    /// Target family (default: chessboard).
    #[serde(default)]
    pub detector: DetectorReq,
    /// Board geometry — required for charuco / puzzleboard.
    #[serde(default)]
    pub board: Option<BoardReq>,
    /// Detection engine (default: pipeline). Chessboard only.
    #[serde(default)]
    pub engine: EngineReq,
    /// Partial [`DetectorParams`] override (same merge semantics as the
    /// bench CLI's `--chessboard-config`). Empty object = defaults. For
    /// charuco / puzzleboard the override merges over the board-tuned
    /// `for_board` chessboard params instead of the plain defaults.
    #[serde(default)]
    pub params: serde_json::Value,
    /// ChESS axis-fit method (default: ring_fit). Chessboard only — the
    /// charuco / puzzleboard facade entry points use the default corner
    /// detector configuration.
    #[serde(default)]
    pub orientation_method: OrientationMethodReq,
    /// Whether to diff the detection against the pinned baseline
    /// (chessboard only; other families have no baselines).
    #[serde(default = "default_true")]
    pub compare_baseline: bool,
    /// Charuco / puzzleboard only: run the `sweep_for_board` preset list
    /// via `detect_*_best` instead of the single `for_board` config.
    /// `params` overrides are ignored in sweep mode (presets run as-is).
    #[serde(default)]
    pub sweep: bool,
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
    /// Detection result (`null` when no target was found).
    pub detection: Option<BaselineImage>,
    /// Baseline comparison (omitted when `compare_baseline` is false).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline: Option<BaselineBlock>,
    /// Family-specific extras (charuco marker count, puzzleboard decode
    /// summary). Omitted for plain chessboard.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info: Option<serde_json::Value>,
}

/// Resolve and validate the effective [`DetectorParams`] from a partial
/// override object. Pipeline-engine params go through
/// [`DetectorParams::validate`] (which enforces the topological cell
/// test constraints); the grid engine accepts all orientation-source
/// combinations, matching the bench CLI.
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

/// Merge a partial override object over an arbitrary base [`DetectorParams`]
/// (top-level key semantics, like [`merge_detector_params`] but with a
/// board-tuned base instead of the plain defaults).
fn merge_params_over(
    base: &DetectorParams,
    overrides: &serde_json::Value,
) -> Result<DetectorParams, ApiError> {
    let mut value = serde_json::to_value(base).map_err(|e| ApiError::Internal(e.to_string()))?;
    if let (Some(base_obj), Some(over_obj)) = (value.as_object_mut(), overrides.as_object()) {
        for (k, v) in over_obj {
            base_obj.insert(k.clone(), v.clone());
        }
    }
    serde_json::from_value(value).map_err(|e| ApiError::BadRequest(format!("invalid params: {e}")))
}

/// `POST /api/detect` handler.
pub async fn detect(
    State(state): State<AppState>,
    Json(req): Json<DetectRequest>,
) -> Result<Json<DetectResponse>, ApiError> {
    let (entry, k) = resolve_label(&state.dataset, &req.label)?;
    let entry = entry.clone();
    let abs = entry.absolute();
    if !abs.exists() {
        return Err(ApiError::NotFound(format!(
            "{} is missing on disk (private dataset not provisioned?)",
            entry.path
        )));
    }
    match req.detector {
        DetectorReq::Chessboard => detect_chessboard_family(state, req, entry, k, abs).await,
        DetectorReq::Charuco | DetectorReq::Puzzleboard => {
            detect_board_family(req, entry, k, abs).await
        }
    }
}

async fn detect_chessboard_family(
    state: AppState,
    req: DetectRequest,
    entry: calib_targets_bench::dataset::DatasetEntry,
    k: u32,
    abs: std::path::PathBuf,
) -> Result<Json<DetectResponse>, ApiError> {
    let engine = Engine::from(req.engine);
    let params = effective_params(&req.params, engine)?;
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
        info: None,
    }))
}

async fn detect_board_family(
    req: DetectRequest,
    entry: calib_targets_bench::dataset::DatasetEntry,
    k: u32,
    abs: std::path::PathBuf,
) -> Result<Json<DetectResponse>, ApiError> {
    let board = req.board.clone().ok_or_else(|| {
        ApiError::BadRequest("charuco / puzzleboard requests need a `board` spec".into())
    })?;
    let detector = req.detector;
    let params_override = req.params.clone();
    let sweep = req.sweep;

    let (detection, info, elapsed_ms, dims) =
        tokio::task::spawn_blocking(move || -> Result<_, ApiError> {
            let img = load_entry_image(&abs)?;
            let fed = calib_targets_bench::runner::fed_image(&img, &entry, k)?;
            let dims = fed.dimensions();
            let started = std::time::Instant::now();
            let (corners, info): (Vec<BaselineCorner>, serde_json::Value) = match detector {
                DetectorReq::Charuco => {
                    let dictionary =
                        calib_targets::aruco::builtins::builtin_dictionary(&board.dictionary)
                            .ok_or_else(|| {
                                ApiError::BadRequest(format!(
                                    "unknown ArUco dictionary {:?}",
                                    board.dictionary
                                ))
                            })?;
                    let spec = calib_targets::charuco::CharucoBoardSpec::new(
                        board.rows,
                        board.cols,
                        board.cell_size,
                        board.marker_size_rel,
                        dictionary,
                    );
                    let result = if sweep {
                        let configs = calib_targets::charuco::CharucoParams::sweep_for_board(&spec);
                        calib_targets::detect::detect_charuco_best(&fed, &configs)
                            .map_err(|e| ApiError::BadRequest(e.to_string()))?
                    } else {
                        let mut cparams = calib_targets::charuco::CharucoParams::for_board(&spec);
                        cparams.chessboard =
                            merge_params_over(&cparams.chessboard, &params_override)?;
                        // The charuco grid runs on the topological default (`for_board`);
                        // DetectorParams overrides flow straight through.
                        calib_targets::detect::detect_charuco(&fed, &cparams)
                            .map_err(|e| ApiError::BadRequest(e.to_string()))?
                    };
                    let corners = result
                        .corners
                        .iter()
                        .map(|c| BaselineCorner {
                            i: c.grid.u,
                            j: c.grid.v,
                            x: c.position.x,
                            y: c.position.y,
                            id: Some(c.id),
                            score: c.score,
                        })
                        .collect();
                    (
                        corners,
                        serde_json::json!({ "markers": result.markers.len() }),
                    )
                }
                DetectorReq::Puzzleboard => {
                    let spec = calib_targets::puzzleboard::PuzzleBoardSpec::with_origin(
                        board.rows,
                        board.cols,
                        board.cell_size,
                        board.origin_row,
                        board.origin_col,
                    )
                    .map_err(|e| ApiError::BadRequest(e.to_string()))?;
                    let result = if sweep {
                        let configs =
                            calib_targets::puzzleboard::PuzzleBoardParams::sweep_for_board(&spec);
                        calib_targets::detect::detect_puzzleboard_best(&fed, &configs)
                            .map_err(|e| ApiError::BadRequest(e.to_string()))?
                    } else {
                        let mut pparams =
                            calib_targets::puzzleboard::PuzzleBoardParams::for_board(&spec);
                        pparams.chessboard =
                            merge_params_over(&pparams.chessboard, &params_override)?;
                        calib_targets::detect::detect_puzzleboard(&fed, &pparams)
                            .map_err(|e| ApiError::BadRequest(e.to_string()))?
                    };
                    let corners = result
                        .corners
                        .iter()
                        .map(|c| BaselineCorner {
                            i: c.grid.u,
                            j: c.grid.v,
                            x: c.position.x,
                            y: c.position.y,
                            id: Some(c.id),
                            score: c.score,
                        })
                        .collect();
                    let decode = serde_json::to_value(&result.decode)
                        .map_err(|e| ApiError::Internal(e.to_string()))?;
                    (corners, serde_json::json!({ "decode": decode }))
                }
                DetectorReq::Chessboard => unreachable!("routed to chessboard family"),
            };
            let elapsed_ms = started.elapsed().as_secs_f64() * 1e3;
            let detection = if corners.is_empty() {
                None
            } else {
                Some(BaselineImage {
                    labelled_count: corners.len(),
                    cell_size_px: 0.0,
                    corners,
                })
            };
            Ok((detection, info, elapsed_ms, dims))
        })
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))??;

    Ok(Json(DetectResponse {
        elapsed_ms,
        image: ImageDims {
            width: dims.0,
            height: dims.1,
        },
        detection,
        baseline: None,
        info: Some(info),
    }))
}
