//! Knobs for the decoding stage.

use serde::{Deserialize, Serialize};

/// Tuning parameters for the decoding stage.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DecodeConfig {
    /// Minimum window size (in squares) required to attempt a decode.
    ///
    /// The paper guarantees 3×3 = 99.33 % unique and 4×4 = 100 % unique.
    /// Default is 4 for calibration use.
    #[serde(default = "default_min_window")]
    pub min_window: u32,
    /// Per-bit confidence floor — bits below this are treated as unknown.
    #[serde(default = "default_min_bit_confidence")]
    pub min_bit_confidence: f32,
    /// Maximum fraction of bits allowed to be wrong after majority voting.
    ///
    /// The paper allows up to 40 % (401 / 1002). Default is 0.3.
    #[serde(default = "default_max_bit_error_rate")]
    pub max_bit_error_rate: f32,
    /// If true, attempt to decode each connected component independently.
    #[serde(default = "default_search_all_components")]
    pub search_all_components: bool,
    /// Sample radius for edge-midpoint disk (fraction of the edge length).
    #[serde(default = "default_sample_radius_rel")]
    pub sample_radius_rel: f32,
}

fn default_min_window() -> u32 {
    4
}
fn default_min_bit_confidence() -> f32 {
    0.15
}
fn default_max_bit_error_rate() -> f32 {
    0.30
}
fn default_search_all_components() -> bool {
    true
}
fn default_sample_radius_rel() -> f32 {
    1.0 / 6.0
}

impl Default for DecodeConfig {
    fn default() -> Self {
        Self {
            min_window: default_min_window(),
            min_bit_confidence: default_min_bit_confidence(),
            max_bit_error_rate: default_max_bit_error_rate(),
            search_all_components: default_search_all_components(),
            sample_radius_rel: default_sample_radius_rel(),
        }
    }
}
