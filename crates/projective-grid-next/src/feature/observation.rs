//! [`Observation<F>`]: a detected feature — position, axis hints, opaque tag,
//! optional score.
//!
//! `Observation<F>` is the canonical input to every task in the crate.
//! Algorithms read `position` and `axes`; they read `tag` indirectly through
//! the [`LabelPolicy`](crate::policy::LabelPolicy)'s parity rule, and they
//! never read `score` (it is reserved for caller-side bookkeeping and
//! diagnostics).

use nalgebra::Point2;

use super::axis::AxisEstimate;
use crate::float::Float;
use crate::policy::FeatureTag;

/// A single detected feature.
///
/// Construct via [`Observation::new`] and refine with the `with_*` builders.
/// The struct is `#[non_exhaustive]` so adding a future field (e.g. a
/// per-corner inlier count) is non-breaking for downstream literal
/// constructions — they must go through the builder.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct Observation<F: Float> {
    /// Sub-pixel position. Pixel-centre convention: `(x + 0.5, y + 0.5)`.
    pub position: Point2<F>,
    /// Two undirected local grid-axis estimates. Default-constructed axes
    /// carry `sigma = π` — the "no information" sentinel that downstream
    /// consumers naturally drop.
    pub axes: [AxisEstimate<F>; 2],
    /// Opaque per-observation tag interpreted by the active
    /// [`LabelPolicy`](crate::policy::LabelPolicy) parity rule. `None`
    /// means "no tag attached", which the policy treats as "tag-agnostic"
    /// (the no-op rule accepts everything).
    pub tag: Option<FeatureTag>,
    /// Optional confidence score from the upstream feature detector.
    /// Reserved for caller bookkeeping and diagnostic output — algorithms
    /// in this crate never consume it.
    pub score: Option<F>,
}

impl<F: Float> Observation<F> {
    /// Construct an observation with only a position; both axes default to
    /// the uninformative sentinel and there is no attached tag or score.
    pub fn new(position: Point2<F>) -> Self {
        Self {
            position,
            axes: [AxisEstimate::uninformative(); 2],
            tag: None,
            score: None,
        }
    }

    /// Attach two axis estimates to the observation.
    #[must_use]
    pub fn with_axes(mut self, axes: [AxisEstimate<F>; 2]) -> Self {
        self.axes = axes;
        self
    }

    /// Attach a [`FeatureTag`].
    #[must_use]
    pub fn with_tag(mut self, tag: FeatureTag) -> Self {
        self.tag = Some(tag);
        self
    }

    /// Attach a detector confidence score.
    #[must_use]
    pub fn with_score(mut self, score: F) -> Self {
        self.score = Some(score);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::float::lit;

    fn assert_new_defaults<F: Float>() {
        let obs = Observation::<F>::new(Point2::new(lit::<F>(1.5_f32), lit::<F>(2.5_f32)));
        assert!(!obs.axes[0].is_informative());
        assert!(!obs.axes[1].is_informative());
        assert!(obs.tag.is_none());
        assert!(obs.score.is_none());
    }

    fn assert_builders<F: Float>() {
        let axes = [
            AxisEstimate::<F>::from_angle(lit::<F>(0.0_f32)),
            AxisEstimate::<F>::from_angle(lit::<F>(1.5_f32)),
        ];
        let obs = Observation::<F>::new(Point2::new(F::zero(), F::zero()))
            .with_axes(axes)
            .with_tag(FeatureTag::new(7))
            .with_score(lit::<F>(0.9_f32));
        assert_eq!(obs.tag, Some(FeatureTag::new(7)));
        assert!(obs.axes[0].is_informative());
        assert!(obs.axes[1].is_informative());
        assert_eq!(obs.score, Some(lit::<F>(0.9_f32)));
    }

    #[test]
    fn new_defaults_f32() {
        assert_new_defaults::<f32>();
    }
    #[test]
    fn new_defaults_f64() {
        assert_new_defaults::<f64>();
    }
    #[test]
    fn builders_f32() {
        assert_builders::<f32>();
    }
    #[test]
    fn builders_f64() {
        assert_builders::<f64>();
    }
}
