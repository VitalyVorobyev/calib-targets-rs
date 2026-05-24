//! [`FeatureTag`]: opaque per-observation `u32` owned by the consumer.
//!
//! The crate never interprets a `FeatureTag`'s bits directly — the
//! interpretation lives in the [`LabelPolicy`](super::LabelPolicy)
//! `parity_rule`. The chessboard parity rule reads `tag.parity_bit()`; a
//! marker-tag rule could read the upper 16 bits as a marker id, etc.

/// Opaque per-observation tag carried through the pipeline.
///
/// Identity, equality, and hashing all defer to the underlying `u32`. The
/// crate uses [`parity_bit`](Self::parity_bit) when the active
/// [`ParityRule`](super::ParityRule) is `Chessboard`; otherwise the tag is
/// treated as a uniform identifier whose only role is bookkeeping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FeatureTag(pub u32);

impl FeatureTag {
    /// Wrap a `u32` as a tag.
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Return the raw bits.
    #[inline]
    pub const fn value(self) -> u32 {
        self.0
    }

    /// Least-significant bit. Used by `ParityRule::Chessboard`
    /// to encode the per-corner parity (0 / 1) attached by the chessboard
    /// consumer at policy-build time.
    #[inline]
    pub const fn parity_bit(self) -> u32 {
        self.0 & 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parity_bit_matches_low_bit() {
        for v in 0u32..16 {
            assert_eq!(FeatureTag::new(v).parity_bit(), v & 1);
        }
    }

    #[test]
    fn default_is_zero() {
        assert_eq!(FeatureTag::default(), FeatureTag::new(0));
    }
}
