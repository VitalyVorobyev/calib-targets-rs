//! Internal helpers for generic float operations not available on `RealField`.

use crate::Float;

/// Convert an `f64` literal to `F`.
///
/// Shorthand for `F::from_subset(&val)` — keeps call sites concise.
pub(crate) fn lit<F: Float>(val: f64) -> F {
    F::from_subset(&val)
}
