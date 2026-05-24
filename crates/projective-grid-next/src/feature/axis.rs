//! [`AxisEstimate<F>`]: an undirected local grid direction with angular sigma.
//!
//! Float-generic replacement for the legacy `f32`-only
//! `projective_grid::topological::AxisEstimate` plus the legacy
//! `calib-targets-core` re-export. Per-corner orientation now lives *only*
//! in `Corner.axes: [AxisEstimate; 2]`; a single-axis `Corner::orientation`
//! field has been removed workspace-wide (see CLAUDE.md "Corner orientation
//! contract (axes-only)").
//!
//! ## Convention
//!
//! The two axes on a single observation satisfy:
//!
//! * `axes[0].angle ∈ [0, π)`
//! * `axes[1].angle ∈ (axes[0].angle, axes[0].angle + π)`
//! * `axes[1].angle − axes[0].angle ≈ π/2` (two orthogonal grid directions,
//!   *not* unit-square diagonals).
//! * The CCW sweep from `axes[0]` to `axes[1]` crosses a **dark** sector
//!   (this is what encodes chessboard parity: at parity-0 corners
//!   `axes[0] ≈ Θ_horizontal`, at parity-1 corners `axes[0] ≈ Θ_vertical`).
//!
//! ## Sigma convention
//!
//! `sigma = π` is the "no information" sentinel and is the value returned by
//! [`AxisEstimate::default`] / [`AxisEstimate::uninformative`]. Downstream
//! consumers that weight by `sigma` will naturally drop such axes from
//! consideration.
//!
//! ## Undirected circular mean discipline
//!
//! Any helper that computes a circular mean of axis angles MUST accumulate
//! `(cos 2θ, sin 2θ)` and halve the resulting `atan2`. Accumulating raw
//! `(cos θ, sin θ)` breaks at the 0°/180° seam — the v1 Phase-4 regression
//! root cause called out in CLAUDE.md. The fix lives in
//! [`crate::stats::circular`].

use crate::float::Float;

/// Per-feature undirected grid-axis estimate.
///
/// See the module-level docs for the full sign / sigma convention.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct AxisEstimate<F: Float> {
    /// Axis angle in radians. Undirected: equivalent under `θ ≡ θ + π`.
    pub angle: F,
    /// 1σ angular uncertainty in radians. `sigma >= π` is treated as "no
    /// information" by downstream consumers.
    pub sigma: F,
}

impl<F: Float> AxisEstimate<F> {
    /// Construct an axis estimate from an angle and sigma.
    pub fn new(angle: F, sigma: F) -> Self {
        Self { angle, sigma }
    }

    /// Construct an axis estimate from a bare angle, with zero uncertainty.
    ///
    /// Useful for synthetic / ground-truth inputs and for callers that do
    /// not track per-axis uncertainty.
    pub fn from_angle(angle: F) -> Self {
        Self {
            angle,
            sigma: F::zero(),
        }
    }

    /// The "no information" sentinel: `angle = 0`, `sigma = π`. Downstream
    /// code that weights by `sigma` will naturally treat this as unusable.
    pub fn uninformative() -> Self {
        Self {
            angle: F::zero(),
            sigma: F::pi(),
        }
    }

    /// `true` iff the estimate carries information (`sigma < π`).
    #[inline]
    pub fn is_informative(&self) -> bool {
        self.sigma < F::pi()
    }
}

impl<F: Float> Default for AxisEstimate<F> {
    fn default() -> Self {
        Self::uninformative()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::float::lit;

    fn assert_uninformative_default<F: Float>() {
        let a: AxisEstimate<F> = AxisEstimate::default();
        assert!(!a.is_informative());
        assert_eq!(a.sigma, F::pi());
    }

    fn assert_from_angle_zero_sigma<F: Float>() {
        let a: AxisEstimate<F> = AxisEstimate::from_angle(lit::<F>(0.5_f32));
        assert!(a.is_informative());
        assert_eq!(a.sigma, F::zero());
    }

    fn assert_new_round_trips<F: Float>() {
        let angle = lit::<F>(1.25_f32);
        let sigma = lit::<F>(0.1_f32);
        let a = AxisEstimate::<F>::new(angle, sigma);
        assert_eq!(a.angle, angle);
        assert_eq!(a.sigma, sigma);
    }

    #[test]
    fn uninformative_default_f32() {
        assert_uninformative_default::<f32>();
    }
    #[test]
    fn uninformative_default_f64() {
        assert_uninformative_default::<f64>();
    }
    #[test]
    fn from_angle_zero_sigma_f32() {
        assert_from_angle_zero_sigma::<f32>();
    }
    #[test]
    fn from_angle_zero_sigma_f64() {
        assert_from_angle_zero_sigma::<f64>();
    }
    #[test]
    fn new_round_trips_f32() {
        assert_new_round_trips::<f32>();
    }
    #[test]
    fn new_round_trips_f64() {
        assert_new_round_trips::<f64>();
    }
}
