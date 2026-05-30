//! Stage 1: pre-filter.
//!
//! Per-corner strength and fit-quality gates. Corners that pass both
//! are promoted from `Raw` to `CornerStage::Strong`; everything else
//! stays `Raw` and is invisible to clustering, seed search, grow, and
//! validation. These are chess-specific gates — they read the ChESS
//! `strength` / `fit_rms` / `contrast` fields directly.

use crate::corner::CornerAug;
use crate::params::DetectorParams;

/// Strength gate: the corner's ChESS response must clear
/// [`DetectorParams::min_corner_strength`](crate::DetectorParams::min_corner_strength).
pub(crate) fn passes_strength(aug: &CornerAug, params: &DetectorParams) -> bool {
    aug.strength >= params.min_corner_strength
}

/// Fit-quality gate: the corner's fit RMS must be small relative to its
/// local contrast. A non-finite
/// [`AdvancedTuning::max_fit_rms_ratio`](crate::AdvancedTuning::max_fit_rms_ratio)
/// or non-positive contrast disables the gate (everything passes).
pub(crate) fn passes_fit_quality(aug: &CornerAug, params: &DetectorParams) -> bool {
    let max_fit_rms_ratio = params.effective_tuning().max_fit_rms_ratio;
    if !max_fit_rms_ratio.is_finite() {
        return true;
    }
    if aug.contrast <= 0.0 {
        return true;
    }
    aug.fit_rms <= max_fit_rms_ratio * aug.contrast
}
