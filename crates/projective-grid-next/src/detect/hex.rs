//! Hex grid detection — not implemented in v1.
//!
//! The primitive layer (lattice tables, geometry, stats) is already
//! lattice-agnostic, but the algorithm wiring is deferred. Calling
//! [`detect_hex_grid`] always returns
//! [`crate::error::UnsupportedCombination::HexDetection`],
//! wrapped in a [`DetectionError::UnsupportedCombination`].

use crate::error::{DetectionError, UnsupportedCombination};
use crate::feature::Observation;
use crate::float::Float;

/// Hex detection placeholder.
///
/// Returns
/// [`DetectionError::UnsupportedCombination(UnsupportedCombination::HexDetection)`](crate::error::UnsupportedCombination::HexDetection)
/// unconditionally. Exists to surface the typed-error contract from the
/// public surface so callers can pattern-match on
/// [`crate::error::UnsupportedCombination`] without
/// branching on whether the function is present at all.
///
/// # Errors
///
/// Always returns
/// [`DetectionError::UnsupportedCombination`] wrapping
/// [`UnsupportedCombination::HexDetection`].
pub fn detect_hex_grid<F: Float>(_observations: &[Observation<F>]) -> Result<(), DetectionError> {
    Err(DetectionError::UnsupportedCombination(
        UnsupportedCombination::HexDetection,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_hex_detection_returns_unsupported<F: Float>() {
        let obs: Vec<Observation<F>> = Vec::new();
        let err = detect_hex_grid(&obs).expect_err("hex detection must be unsupported");
        assert!(matches!(
            err,
            DetectionError::UnsupportedCombination(UnsupportedCombination::HexDetection)
        ));
    }

    #[test]
    fn hex_detection_returns_unsupported_f32() {
        assert_hex_detection_returns_unsupported::<f32>();
    }
    #[test]
    fn hex_detection_returns_unsupported_f64() {
        assert_hex_detection_returns_unsupported::<f64>();
    }
}
