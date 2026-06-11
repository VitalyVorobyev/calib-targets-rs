//! Snap-label parsing and fed-image production.
//!
//! A *label* is the bench's baseline JSON key: the manifest-relative image
//! path, with a `#k` suffix selecting one sub-snap of a stitched composite
//! (e.g. `privatedata/<set>/target_15.png#3`). The *fed image* is the
//! post-crop, post-upscale grayscale buffer the detector actually saw —
//! every coordinate the API returns lives in that frame.

use std::io::Cursor;
use std::sync::Arc;

use calib_targets_bench::dataset::{Dataset, DatasetEntry, ImageKind};
use calib_targets_bench::runner::{fed_image, load_entry_image};
use image::GrayImage;

use crate::error::ApiError;
use crate::state::StudioState;

/// Split a label into its base path and optional `#k` snap index.
pub fn parse_label(label: &str) -> (&str, Option<u32>) {
    match label.rsplit_once('#') {
        Some((base, k)) => match k.parse() {
            Ok(idx) => (base, Some(idx)),
            Err(_) => (label, None),
        },
        None => (label, None),
    }
}

/// Resolve a label against the manifest: returns the entry and the snap
/// index (`0` for plain entries). Rejects out-of-range or missing `#k` on
/// stitched entries and stray `#k` on plain entries.
pub fn resolve_label<'a>(
    dataset: &'a Dataset,
    label: &str,
) -> Result<(&'a DatasetEntry, u32), ApiError> {
    let (base, sub) = parse_label(label);
    let entry = dataset
        .find(base)
        .ok_or_else(|| ApiError::NotFound(format!("{base} is not in datasets.toml")))?;
    match (entry.stitched.as_ref(), sub) {
        (Some(spec), Some(k)) if k < spec.count => Ok((entry, k)),
        (Some(spec), Some(k)) => Err(ApiError::BadRequest(format!(
            "snap index {k} out of range (entry has {} snaps)",
            spec.count
        ))),
        (Some(_), None) => Err(ApiError::BadRequest(format!(
            "{base} is a stitched composite — address one snap with `{base}#k`"
        ))),
        (None, Some(_)) => Err(ApiError::BadRequest(format!(
            "{base} is not stitched — drop the `#k` suffix"
        ))),
        (None, None) => Ok((entry, 0)),
    }
}

/// The string form of an entry's [`ImageKind`] used on the wire.
pub fn kind_str(kind: ImageKind) -> &'static str {
    match kind {
        ImageKind::Public => "public",
        ImageKind::Private => "private",
    }
}

/// Decode + crop + upscale the fed image for a resolved label.
pub fn load_fed_image(entry: &DatasetEntry, k: u32) -> Result<GrayImage, ApiError> {
    let abs = entry.absolute();
    if !abs.exists() {
        return Err(ApiError::NotFound(format!(
            "{} is missing on disk (private dataset not provisioned?)",
            entry.path
        )));
    }
    let img = load_entry_image(&abs)?;
    Ok(fed_image(&img, entry, k)?)
}

/// Encoded fed-image PNG for a label, served from the state cache when warm.
pub fn fed_png(state: &StudioState, label: &str) -> Result<Arc<Vec<u8>>, ApiError> {
    if let Some(png) = state.cached_png(label) {
        return Ok(png);
    }
    let (entry, k) = resolve_label(&state.dataset, label)?;
    let fed = load_fed_image(entry, k)?;
    let mut buf = Vec::new();
    fed.write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
        .map_err(|e| ApiError::Internal(format!("encode {label}: {e}")))?;
    let png = Arc::new(buf);
    state.cache_png(label, png.clone());
    Ok(png)
}
