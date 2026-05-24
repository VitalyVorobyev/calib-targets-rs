//! Hex grid consistency check — not implemented in v1.
//!
//! Mirrors [`crate::detect::hex::detect_hex_grid`]: returns
//! [`crate::error::UnsupportedCombination::HexConsistency`]
//! wrapped in a [`ConsistencyError::UnsupportedCombination`].

use crate::error::{ConsistencyError, UnsupportedCombination};
use crate::float::Float;

/// Hex consistency-check placeholder.
///
/// # Errors
///
/// Always returns
/// [`ConsistencyError::UnsupportedCombination`] wrapping
/// [`UnsupportedCombination::HexConsistency`].
pub fn check_hex_labels<F: Float>() -> Result<(), ConsistencyError> {
    let _ = std::marker::PhantomData::<F>;
    Err(ConsistencyError::UnsupportedCombination(
        UnsupportedCombination::HexConsistency,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_hex_consistency_returns_unsupported<F: Float>() {
        let err = check_hex_labels::<F>().expect_err("hex consistency must be unsupported");
        assert!(matches!(
            err,
            ConsistencyError::UnsupportedCombination(UnsupportedCombination::HexConsistency)
        ));
    }

    #[test]
    fn hex_consistency_returns_unsupported_f32() {
        assert_hex_consistency_returns_unsupported::<f32>();
    }
    #[test]
    fn hex_consistency_returns_unsupported_f64() {
        assert_hex_consistency_returns_unsupported::<f64>();
    }
}
