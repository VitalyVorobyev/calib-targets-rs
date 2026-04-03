//! Internal helpers for generic float operations not available on `RealField`.

use crate::Float;

/// Generic `rem_euclid`: result is always in `[0, b)`.
pub(crate) fn rem_euclid<F: Float>(a: F, b: F) -> F {
    let r = a % b;
    if r < F::zero() {
        r + b
    } else {
        r
    }
}

/// Convert radians to degrees.
pub(crate) fn to_degrees<F: Float>(rad: F) -> F {
    rad * F::from_subset(&180.0) / F::pi()
}

/// Convert an `f64` literal to `F`.
///
/// Shorthand for `F::from_subset(&val)` — keeps call sites concise.
pub(crate) fn lit<F: Float>(val: f64) -> F {
    F::from_subset(&val)
}
