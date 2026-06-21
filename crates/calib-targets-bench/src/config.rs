//! Partial-config merge semantics for [`DetectorParams`] shared by the bench
//! CLI's `--chessboard-config` flag and programmatic consumers (e.g. the
//! studio server, whose overrides arrive as JSON request bodies).

use calib_targets::chessboard::DetectorParams;

/// Merge a partial JSON override object over [`DetectorParams::default`].
///
/// Top-level keys present in `overrides` replace the default value wholesale;
/// missing keys keep their default. Non-object `overrides` values are ignored
/// (the defaults pass through), matching the historical `--chessboard-config`
/// behaviour.
pub fn merge_detector_params(overrides: &serde_json::Value) -> std::io::Result<DetectorParams> {
    let mut base =
        serde_json::to_value(DetectorParams::default()).map_err(std::io::Error::other)?;
    if let (Some(base_obj), Some(over_obj)) = (base.as_object_mut(), overrides.as_object()) {
        for (k, v) in over_obj {
            base_obj.insert(k.clone(), v.clone());
        }
    }
    serde_json::from_value(base).map_err(std::io::Error::other)
}

/// Load a [`DetectorParams`] from an optional JSON file, falling back to
/// [`DetectorParams::default`] when the path is `None`. Partial files are
/// supported via [`merge_detector_params`]: any field present overrides the
/// default; missing fields keep their default value.
pub fn load_chessboard_config(path: Option<&str>) -> std::io::Result<DetectorParams> {
    let Some(path) = path else {
        return Ok(DetectorParams::default());
    };
    let text = std::fs::read_to_string(path)?;
    let overrides: serde_json::Value =
        serde_json::from_str(&text).map_err(std::io::Error::other)?;
    merge_detector_params(&overrides)
}
