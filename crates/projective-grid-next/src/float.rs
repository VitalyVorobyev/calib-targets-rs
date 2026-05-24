//! Sealed [`Float`] trait alias used throughout the crate.
//!
//! Every algorithm in this crate is generic over `F: Float`, where `Float`
//! is `nalgebra::RealField + Copy + From<f32> + 'static`. This single bound
//! replaces the inconsistent `f32`-hardcoded / `Float`-generic split of the
//! legacy `projective-grid` crate and closes Gap 2 from `docs/algorithmic_gaps.md`.
//!
//! ## Sealed
//!
//! The trait is sealed (callers cannot add their own `impl Float for MyType`)
//! to keep the trait bound semver-flexible. Extending the bound (for example
//! by adding a `kiddo::float::kdtree::Axis` super-bound) becomes a non-breaking
//! change: external types that already satisfy the blanket `impl<T: …> Float for T`
//! pick up the new bound automatically; types that don't were never callable
//! in the first place.
//!
//! ## Why `From<f32>`
//!
//! Literal constants — `0.5`, the histogram smoothing weights, `π / 6`, etc.
//! — are expressed via `F::from(0.5_f32)`. The `From<f32>` bound makes that
//! conversion total instead of going through `nalgebra`'s `SupersetOf`
//! machinery. Both `f32` and `f64` implement `From<f32>` directly, so the
//! two common instantiations Just Work.

mod sealed {
    /// Sealed marker trait. Implemented for every type that satisfies the
    /// public `Float` super-bound; callers cannot add their own impls.
    pub trait Seal {}
    impl<T> Seal for T where T: nalgebra::RealField + Copy + From<f32> + 'static {}
}

/// Float type alias used throughout the crate.
///
/// Constraints:
///
/// * [`nalgebra::RealField`] — the algorithm-shaped numeric bound (covers
///   arithmetic, ordering, transcendentals, `pi()`, `default_epsilon()`).
/// * [`Copy`] — every Float value is small enough to pass by value.
/// * `From<f32>` — literal constants convert directly via `F::from(0.5_f32)`.
/// * `'static` — required by `kiddo` and serde where the bound surfaces.
///
/// The bound is sealed via a private `Seal` super-trait so that
/// extending the constraint in a future minor release is non-breaking.
pub trait Float: nalgebra::RealField + Copy + From<f32> + 'static + sealed::Seal {}

impl<T> Float for T where T: nalgebra::RealField + Copy + From<f32> + 'static {}

/// Convert an `f32` literal to `F`. Avoids the `From::from` vs `NumCast::from`
/// ambiguity that fires when `RealField` brings `num_traits::NumCast` into
/// scope — `<F as From<f32>>::from(...)` works but reads worse than this
/// helper. Internal use; not re-exported at the crate root.
#[inline]
pub(crate) fn lit<F: Float>(value: f32) -> F {
    <F as From<f32>>::from(value)
}

/// Absolute value of `F`, routed through `nalgebra::ComplexField::abs`.
/// The bare method name collides with the `num_traits::Float::abs` import
/// some test modules bring in; a free function keeps call sites
/// unambiguous.
#[cfg(test)]
#[inline]
pub(crate) fn abs<F: Float>(value: F) -> F {
    nalgebra::ComplexField::abs(value)
}
