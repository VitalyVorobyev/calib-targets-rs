//! Per-feature axis cache for the topological pipeline.
//!
//! The pipeline reads per-corner axes many times (pre-filter, classify,
//! local diagonal inference) and the original `LocalAxis::sigma_rad`
//! `Option<F>` dance is cheap individually but verbose at every call
//! site. The cache pre-resolves `(angle_rad, informative)` for both slots
//! of every feature once at the orchestrator entry point.
//!
//! **Informative rule.** An axis is informative iff
//! `sigma_rad.is_none()` OR `sigma_rad < max_axis_sigma_rad`. The
//! `None` case is treated as "no uncertainty info, trust the angle" —
//! matching how the chess-corners adapter and other detectors hand
//! corners to the new contract without always populating sigma.
//!
//! This is **stricter than the seed-and-grow path**: the topological
//! pipeline's per-corner axes drive every classification, so callers
//! that supply explicit `sigma_rad = Some(s)` with `s ≥
//! max_axis_sigma_rad` are opted out — the parallel hand-off path
//! consumes `LocalAxis::new(_, Some(F::pi()))` as the "no information"
//! sentinel, in line with the workspace's axis-only orientation
//! contract.

use crate::feature::{LocalAxis, OrientedFeature};
use crate::float::Float;

/// Precomputed per-corner axis view consumed by the topological stages.
#[derive(Clone, Copy, Debug)]
pub(super) struct AxisCache<F: Float> {
    /// Axis angle in radians per slot.
    pub(super) angle_rad: [F; 2],
    /// Whether each slot's axis carries usable angular evidence.
    pub(super) informative: [bool; 2],
}

impl<F: Float> AxisCache<F> {
    /// Return `true` when at least one slot is informative.
    #[inline]
    pub(super) fn any_informative(&self) -> bool {
        self.informative[0] || self.informative[1]
    }
}

/// Decide whether a single [`LocalAxis`] carries usable evidence under
/// the topological pipeline's policy.
#[inline]
pub(super) fn is_informative<F: Float>(axis: &LocalAxis<F>, max_sigma_rad: F) -> bool {
    match axis.sigma_rad {
        None => true,
        Some(s) => s.is_finite() && s < max_sigma_rad,
    }
}

/// Build the `[AxisCache; n]` slice from the input features under the
/// active `max_axis_sigma_rad` threshold.
pub(super) fn build_axis_caches<F: Float>(
    features: &[OrientedFeature<F, 2>],
    max_sigma_rad: F,
) -> Vec<AxisCache<F>> {
    features
        .iter()
        .map(|f| AxisCache {
            angle_rad: [f.axes[0].angle_rad, f.axes[1].angle_rad],
            informative: [
                is_informative(&f.axes[0], max_sigma_rad),
                is_informative(&f.axes[1], max_sigma_rad),
            ],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feature::{LocalAxis, PointFeature};
    use nalgebra::Point2;

    fn feature<F: Float>(idx: usize, axes: [LocalAxis<F>; 2]) -> OrientedFeature<F, 2> {
        OrientedFeature::new(
            PointFeature::new(idx, Point2::new(F::zero(), F::zero())),
            axes,
        )
    }

    #[test]
    fn none_sigma_is_informative() {
        let cache = build_axis_caches::<f32>(
            &[feature(
                0,
                [
                    LocalAxis::new(0.0, None),
                    LocalAxis::new(std::f32::consts::FRAC_PI_2, None),
                ],
            )],
            0.6,
        );
        assert!(cache[0].informative[0]);
        assert!(cache[0].informative[1]);
    }

    #[test]
    fn high_sigma_is_not_informative() {
        let cache = build_axis_caches::<f32>(
            &[feature(
                0,
                [
                    LocalAxis::new(0.0, Some(0.1)),
                    LocalAxis::new(std::f32::consts::FRAC_PI_2, Some(std::f32::consts::PI)),
                ],
            )],
            0.6,
        );
        assert!(cache[0].informative[0]);
        assert!(!cache[0].informative[1]);
    }

    #[test]
    fn pi_sigma_is_not_informative() {
        let cache = build_axis_caches::<f64>(
            &[feature(
                7,
                [
                    LocalAxis::new(0.0, Some(std::f64::consts::PI)),
                    LocalAxis::new(std::f64::consts::FRAC_PI_2, Some(std::f64::consts::PI)),
                ],
            )],
            0.6,
        );
        assert!(!cache[0].informative[0]);
        assert!(!cache[0].informative[1]);
        assert!(!cache[0].any_informative());
    }

    #[test]
    fn non_finite_sigma_is_not_informative() {
        let cache = build_axis_caches::<f32>(
            &[feature(
                0,
                [
                    LocalAxis::new(0.0, Some(f32::INFINITY)),
                    LocalAxis::new(std::f32::consts::FRAC_PI_2, Some(f32::NAN)),
                ],
            )],
            0.6,
        );
        assert!(!cache[0].informative[0]);
        assert!(!cache[0].informative[1]);
    }
}
