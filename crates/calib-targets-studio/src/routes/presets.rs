//! Built-in detector presets. Unlike the named configs under
//! `studio_configs/` (user-saved, gitignored), these are hardcoded,
//! committed `DetectorParams` overrides surfaced in the Detect tab so the
//! common starting points (the topological grid build and the per-family
//! ChESS strength floors) are one click away without leaving the workspace.
//!
//! Each preset's `params` is a partial-`DetectorParams` override object in
//! the same top-level-key merge format the Detect tab's draft and the bench
//! CLI's `--chessboard-config` accept.

use axum::Json;
use serde::Serialize;
use serde_json::json;

use crate::error::ApiError;

/// One built-in preset row.
#[derive(Serialize)]
pub struct Preset {
    /// Stable identifier shown in the picker.
    pub name: String,
    /// One-line human description of what the preset selects and when to use it.
    pub description: String,
    /// Partial-`DetectorParams` override applied verbatim to the Detect draft.
    pub params: serde_json::Value,
}

impl Preset {
    fn new(name: &str, description: &str, params: serde_json::Value) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            params,
        }
    }
}

/// The committed preset catalogue. Board family (chessboard / charuco /
/// puzzleboard) is still chosen via the Detect tab's family selector — these
/// presets only carry `DetectorParams` overrides.
fn catalogue() -> Vec<Preset> {
    vec![
        Preset::new(
            "charuco-floor",
            "ChESS strength floor used by the ChArUco detector (clears marker-bit false corners). Select the ChArUco family too.",
            json!({ "min_corner_strength": 33.0 }),
        ),
        Preset::new(
            "puzzle-floor",
            "ChESS strength floor used by the PuzzleBoard detector. Select the PuzzleBoard family too.",
            json!({ "min_corner_strength": 0.1 }),
        ),
    ]
}

/// `GET /api/presets` — list the built-in detector presets.
pub async fn list() -> Result<Json<Vec<Preset>>, ApiError> {
    Ok(Json(catalogue()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use calib_targets_bench::config::merge_detector_params;

    #[test]
    fn every_preset_merges_into_valid_params() {
        for preset in catalogue() {
            merge_detector_params(&preset.params).unwrap_or_else(|e| {
                panic!(
                    "preset {} does not merge into valid DetectorParams: {e}",
                    preset.name
                )
            });
        }
    }
}
