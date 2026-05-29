//! Shared per-bit soft-decision math for the self-identifying decoders.
//!
//! The ChArUco board matcher
//! (`calib-targets-charuco`) and the PuzzleBoard edge-code decoder
//! (`calib-targets-puzzleboard`) both score observed bits against an
//! expected pattern with a numerically-stable `log(sigmoid(·))` of a
//! per-bit logit. That base transfer function used to be copy-pasted into
//! both crates; it lives here so the two decoders share one definition and
//! cannot drift.
//!
//! Only the transfer function itself is shared. Each decoder keeps its own
//! logit construction and per-bit flooring policy, because those differ:
//! the PuzzleBoard decoder floors a `kappa * confidence` logit symmetrically
//! (see its `ll_pair`), while the ChArUco matcher floors an intensity-margin
//! logit and, in its diagnostic path, does not floor at all.

/// Numerically stable `log(sigmoid(x))`.
///
/// Evaluates `ln(1 / (1 + e^-x))` without overflow for large-magnitude `x`
/// by branching on the sign:
///
/// - `x ≥ 0`: `-ln(1 + e^-x)` (the `e^-x` term is in `(0, 1]`).
/// - `x < 0`: `x - ln(1 + e^x)` (the `e^x` term is in `(0, 1)`).
///
/// Both branches are pure `f32` arithmetic, so the result is deterministic
/// and bit-for-bit identical across the decoders that call it.
#[inline]
pub fn log_sigmoid(x: f32) -> f32 {
    if x >= 0.0 {
        -(1.0 + (-x).exp()).ln()
    } else {
        x - (1.0 + x.exp()).ln()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_naive_reference_away_from_extremes() {
        for &x in &[-5.0_f32, -1.0, 0.0, 1.0, 5.0] {
            let got = log_sigmoid(x);
            let want = (1.0 / (1.0 + (-x).exp())).ln();
            assert!(
                (got - want).abs() < 1e-5,
                "log_sigmoid({x}) = {got}, want {want}"
            );
        }
    }

    #[test]
    fn stable_for_large_magnitude_inputs() {
        // The naive form underflows/overflows here; the branchy form stays finite.
        assert!(log_sigmoid(60.0).abs() < 1e-6, "log_sigmoid(+large) → ~0");
        assert!(log_sigmoid(-60.0).is_finite());
        assert!(
            (log_sigmoid(-60.0) - (-60.0)).abs() < 1e-3,
            "log_sigmoid(-x) ≈ x for large x"
        );
    }
}
