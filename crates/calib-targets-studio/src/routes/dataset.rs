//! Dataset browsing endpoints: manifest listing, fed-image PNGs, and
//! read-only baseline lookup.

use axum::extract::{Path, State};
use axum::http::header;
use axum::response::IntoResponse;
use axum::Json;
use calib_targets_bench::baseline::{Baseline, BaselineImage};
use calib_targets_bench::dataset::DatasetEntry;
use serde::Serialize;

use super::AppState;
use crate::error::ApiError;
use crate::snaps::{fed_png, kind_str, resolve_label};

/// `GET /api/health` — server identity probe.
pub async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "workspace_root": calib_targets_bench::workspace_root().display().to_string(),
    }))
}

/// One logical sub-snap of a dataset entry.
#[derive(Serialize)]
pub struct SnapInfo {
    /// Snap label (`path` or `path#k`) — the key for every other endpoint.
    pub label: String,
    /// Snap index within the entry (0 for plain images).
    pub index: u32,
    /// Fed-image width in pixels (post-upscale), when known without decoding.
    pub width: Option<u32>,
    /// Fed-image height in pixels (post-upscale), when known without decoding.
    pub height: Option<u32>,
}

/// One `datasets.toml` entry with availability + per-snap metadata.
#[derive(Serialize)]
pub struct ImageInfo {
    /// Manifest-relative path.
    pub path: String,
    /// `"public"` or `"private"`.
    pub kind: &'static str,
    /// Free-form manifest note.
    pub note: String,
    /// Bilinear upscale applied before detection.
    pub upscale: u32,
    /// Stitched-composite spec, when the on-disk file tiles several snaps.
    pub stitched: Option<StitchedInfo>,
    /// Whether the file exists on disk (private data may be absent).
    pub available: bool,
    /// Logical sub-snaps (a single element for plain images).
    pub snaps: Vec<SnapInfo>,
}

/// Wire form of [`calib_targets_bench::dataset::Stitched`].
#[derive(Serialize)]
pub struct StitchedInfo {
    /// Number of horizontally tiled snaps.
    pub count: u32,
    /// Width of each snap in source pixels.
    pub snap_width: u32,
    /// Height of each snap in source pixels.
    pub snap_height: u32,
}

/// `GET /api/dataset` — the full manifest with availability flags.
pub async fn dataset(State(state): State<AppState>) -> Json<serde_json::Value> {
    let images: Vec<ImageInfo> = state.dataset.images.iter().map(image_info).collect();
    Json(serde_json::json!({ "images": images }))
}

fn image_info(entry: &DatasetEntry) -> ImageInfo {
    let abs = entry.absolute();
    let available = abs.exists();
    // Fed dims without a full decode: stitched dims come from the spec;
    // plain images read only the header.
    let source_dims: Option<(u32, u32)> = match entry.stitched.as_ref() {
        Some(spec) => Some((spec.snap_width, spec.snap_height)),
        None if available => image::ImageReader::open(&abs)
            .ok()
            .and_then(|r| r.into_dimensions().ok()),
        None => None,
    };
    let fed_dims = source_dims.map(|(w, h)| (w * entry.upscale, h * entry.upscale));
    let snaps = (0..entry.snap_count())
        .map(|k| SnapInfo {
            label: entry.snap_label(k),
            index: k,
            width: fed_dims.map(|(w, _)| w),
            height: fed_dims.map(|(_, h)| h),
        })
        .collect();
    ImageInfo {
        path: entry.path.clone(),
        kind: kind_str(entry.kind),
        note: entry.note.clone(),
        upscale: entry.upscale,
        stitched: entry.stitched.as_ref().map(|s| StitchedInfo {
            count: s.count,
            snap_width: s.snap_width,
            snap_height: s.snap_height,
        }),
        available,
        snaps,
    }
}

/// `GET /api/image/{label}` — the fed-image grayscale PNG for one snap.
pub async fn image(
    State(state): State<AppState>,
    Path(label): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let state2 = state.clone();
    let png = tokio::task::spawn_blocking(move || fed_png(&state2, &label))
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))??;
    Ok((
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "max-age=3600, immutable"),
        ],
        png.to_vec(),
    ))
}

/// `GET /api/baseline/{label}` — the pinned baseline for one snap, or 404.
pub async fn baseline(
    State(state): State<AppState>,
    Path(label): Path<String>,
) -> Result<Json<BaselineImage>, ApiError> {
    let (entry, _) = resolve_label(&state.dataset, &label)?;
    let baseline = Baseline::load_or_empty(entry.kind);
    baseline
        .images
        .get(&label)
        .cloned()
        .map(Json)
        .ok_or_else(|| ApiError::NotFound(format!("no baseline for {label}")))
}
