//! WebAssembly bindings for `calib-targets` calibration target detectors.
//!
//! Exposes stateless detection functions that accept grayscale `&[u8]` buffers
//! and JS config objects (deserialized via `serde-wasm-bindgen`).

mod blur;
mod detect;
mod diagnose;
mod gray;
mod params;
mod print;

pub use blur::*;
pub use detect::*;
pub use diagnose::*;
pub use params::*;
pub use print::*;

use calib_targets::detect::DetectError;
use calib_targets_core::{DetectorConfig, GrayImageView};
use wasm_bindgen::prelude::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub(crate) fn from_js<T: serde::de::DeserializeOwned>(val: JsValue) -> Result<T, JsError> {
    serde_wasm_bindgen::from_value(val).map_err(|e| JsError::new(&e.to_string()))
}

/// Serialize a Rust value into the plain JS shape `typescript-extras.d.ts`
/// declares.
///
/// Maps become plain objects, not ES `Map`s: the default `serde_wasm_bindgen`
/// serializer turns every map — including each `serde_json::Value::Object`,
/// which is how the `diagnose_*` functions build their `{ result,
/// diagnostics }` payloads — into a `Map`, whose fields `obj.result` /
/// `obj["diagnostics"]` cannot reach. Every map this crate returns has string
/// keys, which is what the object form requires. An absent value (`None`)
/// becomes `null`, as the typings declare, not `undefined`.
pub(crate) fn to_js<T: serde::Serialize>(val: &T) -> Result<JsValue, JsError> {
    const SERIALIZER: serde_wasm_bindgen::Serializer = serde_wasm_bindgen::Serializer::new()
        .serialize_maps_as_objects(true)
        .serialize_missing_as_null(true);
    val.serialize(&SERIALIZER)
        .map_err(|e| JsError::new(&e.to_string()))
}

/// A facade detection result as JS sees it: the detection, or `null` for a
/// miss ([`DetectError::is_not_found`]). Any other error is a misuse — a
/// malformed image, an invalid board spec or parameters — and throws. Every
/// `detect_*` function follows this one convention.
pub(crate) fn detection_to_js<T: serde::Serialize>(
    result: Result<T, DetectError>,
) -> Result<JsValue, JsError> {
    match result {
        Ok(detection) => to_js(&detection),
        Err(err) if err.is_not_found() => Ok(JsValue::NULL),
        Err(err) => Err(JsError::new(&err.to_string())),
    }
}

pub(crate) fn gray_view<'a>(
    pixels: &'a [u8],
    width: u32,
    height: u32,
) -> Result<GrayImageView<'a>, JsError> {
    calib_targets::detect::gray_view_from_slice(width, height, pixels)
        .map_err(|e| JsError::new(&e.to_string()))
}

/// The workspace-default ChESS detector config.
///
/// Delegates to `calib_targets_core::default_chess_config` rather than
/// restating the threshold override, so the value has one definition.
pub(crate) fn workspace_default_chess_cfg() -> DetectorConfig {
    calib_targets_core::default_chess_config()
}

/// Resolve a ChESS detector config, falling back to the workspace default
/// when JS supplies `undefined` / `null`.
pub(crate) fn resolve_chess_cfg(chess_cfg: JsValue) -> Result<DetectorConfig, JsError> {
    if !chess_cfg.is_undefined() && !chess_cfg.is_null() {
        from_js(chess_cfg)
    } else {
        Ok(workspace_default_chess_cfg())
    }
}

/// Apply an explicit `chess_cfg` argument over a params struct's own `chess`
/// field, leaving it untouched when JS supplies `undefined` / `null`.
pub(crate) fn apply_chess_cfg_override(
    dst: &mut DetectorConfig,
    chess_cfg: JsValue,
) -> Result<(), JsError> {
    if !chess_cfg.is_undefined() && !chess_cfg.is_null() {
        *dst = from_js(chess_cfg)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// RGBA → grayscale utility
// ---------------------------------------------------------------------------

/// Convert an RGBA pixel buffer to grayscale (BT.601 weights).
///
/// Input: RGBA buffer of length `4 * width * height`.
/// Returns: grayscale buffer of length `width * height`.
#[wasm_bindgen]
pub fn rgba_to_gray(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, JsError> {
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(|| {
            JsError::new(&format!(
                "image dimensions {}x{} overflow usize",
                width, height
            ))
        })?;
    if rgba.len() != expected {
        return Err(JsError::new(&format!(
            "RGBA buffer length {} does not match 4*{}*{} = {}",
            rgba.len(),
            width,
            height,
            expected
        )));
    }
    Ok(gray::rgba_to_grayscale(rgba, width, height))
}
