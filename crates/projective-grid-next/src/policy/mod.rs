//! [`LabelPolicy<F>`]: the shared parity / tag / eligibility helper consumed
//! by every pipeline `Context` trait.
//!
//! `LabelPolicy<F>` is a **concrete** struct — not a trait — owned by the
//! consumer (e.g. the chessboard crate). The pipeline context traits expose
//! it via `fn label_policy(&self) -> &LabelPolicy<F>`, so the parity and
//! eligibility rules that used to be duplicated across five chessboard
//! validator impls collapse to a single object passed by reference. This
//! closes Gap 4 from `docs/algorithmic_gaps.md`.
//!
//! ## Build once, borrow many
//!
//! ```ignore
//! let policy = LabelPolicy::<f32>::builder(observations.len())
//!     .with_tags(parity_tags)
//!     .with_parity_rule(ParityRule::Chessboard { shift: 0 })
//!     .with_eligibility_mask(&cluster_mask)
//!     .with_cell_size_hint(20.0)
//!     .build();
//! ```
//!
//! Pipeline contexts then return `&policy` from `label_policy`. Every stage
//! consults the same object — no parity drift between seed, grow, fill,
//! extension, and topological recovery.

pub mod tag;

pub use tag::FeatureTag;

use crate::float::Float;
use crate::lattice::Coord;

/// How parity is enforced between [`FeatureTag`]s and lattice coordinates.
///
/// `#[non_exhaustive]` so future parity flavours (e.g. a 4-way marker-cell
/// parity) can be added without breaking exhaustive matches in consumer
/// code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum ParityRule {
    /// No parity constraint. Any tag matches any coordinate; the policy
    /// reduces to eligibility-only filtering.
    #[default]
    None,
    /// Chessboard parity. The required parity at coordinate `(i, j)` is
    /// `(i + j + shift) mod 2`; the observation's tag is admissible iff
    /// `tag.parity_bit() == required`. `shift` accommodates the label
    /// rebase offset (see CLAUDE.md "Grid labels are non-negative").
    Chessboard {
        /// Parity offset applied after rebase.
        shift: u8,
    },
}

/// Concrete parity / tag / eligibility policy.
///
/// Built via [`LabelPolicy::builder`]; consulted by every pipeline stage via
/// `Context::label_policy()`. The struct is `#[non_exhaustive]` so future
/// optional fields (e.g. a per-corner cell-size hint vector) can be added
/// without breaking cross-crate construction; same-crate code still builds
/// it via literal syntax internally for tests, but downstream consumers
/// must route through the builder.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct LabelPolicy<F: Float> {
    tags: Vec<Option<FeatureTag>>,
    eligibility: Vec<bool>,
    parity_rule: ParityRule,
    cell_size_hint: Option<F>,
}

impl<F: Float> LabelPolicy<F> {
    /// Open a builder for a policy covering `n_observations` features.
    /// Defaults: no tags, all features eligible, [`ParityRule::None`], no
    /// cell-size hint.
    pub fn builder(n_observations: usize) -> LabelPolicyBuilder<F> {
        LabelPolicyBuilder {
            inner: Self {
                tags: vec![None; n_observations],
                eligibility: vec![true; n_observations],
                parity_rule: ParityRule::None,
                cell_size_hint: None,
            },
        }
    }

    /// `Some(required_tag)` iff a tag is required at this coordinate under
    /// the active parity rule. `None` means the policy is permissive at
    /// this coordinate (any tag — including no tag — is acceptable).
    pub fn required_label_at(&self, coord: Coord) -> Option<FeatureTag> {
        match self.parity_rule {
            ParityRule::None => None,
            ParityRule::Chessboard { shift } => {
                let (i, j) = coord;
                let parity =
                    ((i.rem_euclid(2) + j.rem_euclid(2) + i32::from(shift)).rem_euclid(2)) as u32;
                Some(FeatureTag::new(parity))
            }
        }
    }

    /// The tag attached to feature `idx`, if any. Returns `None` when `idx`
    /// is out of range, matching the policy's permissive semantics: an
    /// observation we know nothing about is treated as tag-agnostic.
    pub fn label_of(&self, idx: usize) -> Option<FeatureTag> {
        self.tags.get(idx).copied().flatten()
    }

    /// Whether feature `idx` is eligible for labelling. Out-of-range
    /// indices default to ineligible (this is the strict side of
    /// permissive — the policy can confirm an observation is eligible only
    /// if it knows about it).
    pub fn is_eligible(&self, idx: usize) -> bool {
        self.eligibility.get(idx).copied().unwrap_or(false)
    }

    /// Whether feature `idx` may be labelled with `coord` under the policy.
    ///
    /// Returns `true` if:
    ///
    /// * the active parity rule is permissive (`ParityRule::None`), or
    /// * the rule requires a specific parity AND either the feature has no
    ///   tag (treated as tag-agnostic), or the feature's tag's parity bit
    ///   matches the required parity at `coord`.
    ///
    /// **Eligibility is *not* checked here** — callers consult
    /// [`is_eligible`](Self::is_eligible) separately. This keeps the two
    /// dimensions (parity vs. eligibility) orthogonal at the call site.
    pub fn agrees(&self, idx: usize, coord: Coord) -> bool {
        match self.required_label_at(coord) {
            None => true,
            Some(required) => match self.label_of(idx) {
                None => true,
                Some(tag) => tag.parity_bit() == required.parity_bit(),
            },
        }
    }

    /// The active parity rule.
    #[inline]
    pub fn parity_rule(&self) -> ParityRule {
        self.parity_rule
    }

    /// Optional global cell-size hint in pixels. `None` means "no caller-
    /// supplied hint"; the seed finder falls back to a self-consistent
    /// 4-corner edge-ratio estimate in that case.
    #[inline]
    pub fn cell_size_hint(&self) -> Option<F> {
        self.cell_size_hint
    }

    /// Number of observations this policy covers.
    #[inline]
    pub fn n_observations(&self) -> usize {
        self.tags.len()
    }
}

/// Fluent builder for [`LabelPolicy<F>`].
#[derive(Debug, Clone)]
pub struct LabelPolicyBuilder<F: Float> {
    inner: LabelPolicy<F>,
}

impl<F: Float> LabelPolicyBuilder<F> {
    /// Attach a tag to a single feature. Out-of-range indices are ignored
    /// silently; the builder is permissive at construction time so callers
    /// can stream tags from a sparse source.
    #[must_use]
    pub fn with_tag(mut self, idx: usize, tag: FeatureTag) -> Self {
        if let Some(slot) = self.inner.tags.get_mut(idx) {
            *slot = Some(tag);
        }
        self
    }

    /// Attach multiple tags via `(idx, tag)` pairs.
    #[must_use]
    pub fn with_tags(mut self, tags: impl IntoIterator<Item = (usize, FeatureTag)>) -> Self {
        for (idx, tag) in tags {
            if let Some(slot) = self.inner.tags.get_mut(idx) {
                *slot = Some(tag);
            }
        }
        self
    }

    /// Set the eligibility flag for a single feature. Out-of-range indices
    /// are ignored.
    #[must_use]
    pub fn with_eligibility(mut self, idx: usize, eligible: bool) -> Self {
        if let Some(slot) = self.inner.eligibility.get_mut(idx) {
            *slot = eligible;
        }
        self
    }

    /// Copy an eligibility mask into the policy. If `mask` is shorter than
    /// the observation count, the remaining slots are left at the prior
    /// value (default `true`); if longer, the excess is ignored.
    #[must_use]
    pub fn with_eligibility_mask(mut self, mask: &[bool]) -> Self {
        let n = mask.len().min(self.inner.eligibility.len());
        self.inner.eligibility[..n].copy_from_slice(&mask[..n]);
        self
    }

    /// Set the parity rule.
    #[must_use]
    pub fn with_parity_rule(mut self, rule: ParityRule) -> Self {
        self.inner.parity_rule = rule;
        self
    }

    /// Set a global cell-size hint in pixels. Callers pass `None` (omit the
    /// call) to clear; passing a non-positive value is accepted as-is but
    /// downstream consumers will fall back to seed-derived sizing.
    #[must_use]
    pub fn with_cell_size_hint(mut self, hint: F) -> Self {
        self.inner.cell_size_hint = Some(hint);
        self
    }

    /// Finalise the policy.
    pub fn build(self) -> LabelPolicy<F> {
        self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::float::lit;

    fn assert_default_policy_is_permissive<F: Float>() {
        let policy = LabelPolicy::<F>::builder(4).build();
        assert_eq!(policy.n_observations(), 4);
        assert_eq!(policy.parity_rule(), ParityRule::None);
        for idx in 0..4 {
            assert!(policy.is_eligible(idx));
            assert!(policy.label_of(idx).is_none());
            assert!(policy.agrees(idx, (0, 0)));
            assert!(policy.agrees(idx, (5, 7)));
        }
        // Out-of-range queries are conservative on eligibility, permissive
        // on parity (label_of returns None).
        assert!(!policy.is_eligible(4));
        assert!(policy.label_of(4).is_none());
    }

    fn assert_chessboard_parity_zero_shift<F: Float>() {
        let policy = LabelPolicy::<F>::builder(2)
            .with_tag(0, FeatureTag::new(0))
            .with_tag(1, FeatureTag::new(1))
            .with_parity_rule(ParityRule::Chessboard { shift: 0 })
            .build();

        // Required parity at (0, 0) = (0+0+0) mod 2 = 0
        assert_eq!(policy.required_label_at((0, 0)), Some(FeatureTag::new(0)));
        // Required parity at (1, 0) = (1+0+0) mod 2 = 1
        assert_eq!(policy.required_label_at((1, 0)), Some(FeatureTag::new(1)));
        // Required parity at (1, 1) = (1+1+0) mod 2 = 0
        assert_eq!(policy.required_label_at((1, 1)), Some(FeatureTag::new(0)));
        // Required parity at (-1, 0) = ((-1) mod 2 + 0 + 0) = 1
        assert_eq!(policy.required_label_at((-1, 0)), Some(FeatureTag::new(1)));

        // agrees: feature 0 (tag 0) matches (0,0) but not (1,0)
        assert!(policy.agrees(0, (0, 0)));
        assert!(!policy.agrees(0, (1, 0)));
        // feature 1 (tag 1) is the mirror
        assert!(!policy.agrees(1, (0, 0)));
        assert!(policy.agrees(1, (1, 0)));
    }

    fn assert_chessboard_parity_shifted<F: Float>() {
        let policy = LabelPolicy::<F>::builder(2)
            .with_tag(0, FeatureTag::new(0))
            .with_tag(1, FeatureTag::new(1))
            .with_parity_rule(ParityRule::Chessboard { shift: 1 })
            .build();

        // With shift=1 the required parity at (0,0) flips.
        assert_eq!(policy.required_label_at((0, 0)), Some(FeatureTag::new(1)));
        assert_eq!(policy.required_label_at((1, 0)), Some(FeatureTag::new(0)));
        assert!(!policy.agrees(0, (0, 0)));
        assert!(policy.agrees(1, (0, 0)));
    }

    fn assert_no_tag_is_tag_agnostic<F: Float>() {
        // Feature 0 has no tag attached; under Chessboard it still agrees
        // everywhere (tag-agnostic semantics).
        let policy = LabelPolicy::<F>::builder(1)
            .with_parity_rule(ParityRule::Chessboard { shift: 0 })
            .build();
        assert!(policy.label_of(0).is_none());
        assert!(policy.agrees(0, (0, 0)));
        assert!(policy.agrees(0, (1, 0)));
    }

    fn assert_eligibility_mask<F: Float>() {
        let mask = [true, false, true, false];
        let policy = LabelPolicy::<F>::builder(4)
            .with_eligibility_mask(&mask)
            .build();
        for (i, &want) in mask.iter().enumerate() {
            assert_eq!(policy.is_eligible(i), want, "mask[{i}] mismatch");
        }
    }

    fn assert_cell_size_hint<F: Float>() {
        let policy_none = LabelPolicy::<F>::builder(0).build();
        assert!(policy_none.cell_size_hint().is_none());
        let policy = LabelPolicy::<F>::builder(0)
            .with_cell_size_hint(lit::<F>(24.0_f32))
            .build();
        assert_eq!(policy.cell_size_hint(), Some(lit::<F>(24.0_f32)));
    }

    #[test]
    fn default_permissive_f32() {
        assert_default_policy_is_permissive::<f32>();
    }
    #[test]
    fn default_permissive_f64() {
        assert_default_policy_is_permissive::<f64>();
    }
    #[test]
    fn parity_zero_shift_f32() {
        assert_chessboard_parity_zero_shift::<f32>();
    }
    #[test]
    fn parity_zero_shift_f64() {
        assert_chessboard_parity_zero_shift::<f64>();
    }
    #[test]
    fn parity_shifted_f32() {
        assert_chessboard_parity_shifted::<f32>();
    }
    #[test]
    fn parity_shifted_f64() {
        assert_chessboard_parity_shifted::<f64>();
    }
    #[test]
    fn untagged_agrees_everywhere_f32() {
        assert_no_tag_is_tag_agnostic::<f32>();
    }
    #[test]
    fn untagged_agrees_everywhere_f64() {
        assert_no_tag_is_tag_agnostic::<f64>();
    }
    #[test]
    fn eligibility_mask_f32() {
        assert_eligibility_mask::<f32>();
    }
    #[test]
    fn eligibility_mask_f64() {
        assert_eligibility_mask::<f64>();
    }
    #[test]
    fn cell_size_hint_f32() {
        assert_cell_size_hint::<f32>();
    }
    #[test]
    fn cell_size_hint_f64() {
        assert_cell_size_hint::<f64>();
    }
}
