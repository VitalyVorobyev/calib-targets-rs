//! Per-task error types and the [`UnsupportedCombination`] enum.
//!
//! Errors are reserved for "the algorithm cannot run" cases. Per-stage
//! failures (a seed quad that fails the parallelogram test, a grow candidate
//! that loses the ambiguity gate, etc.) are *not* errors — they're typed
//! events emitted to the diagnostic sink. See [`crate::diagnostics`].

use thiserror::Error;

use crate::lattice::LatticeKind;

/// Failure modes for detection tasks (`detect_square_grid`, `detect_hex_grid`).
#[derive(Debug, Clone, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum DetectionError {
    /// Fewer observations supplied than the algorithm requires.
    #[error("insufficient observations: got {found}, need at least {required}")]
    InsufficientObservations {
        /// Number of observations the caller passed in.
        found: usize,
        /// Minimum required by the algorithm (typically 4 for a seed quad).
        required: usize,
    },
    /// All input points are coincident or collinear; no grid can be fit.
    #[error("input is degenerate (collinear or coincident)")]
    DegenerateCloud,
    /// No quadrilateral satisfied the seed-finder gates (edge ratio,
    /// midpoint violation, parity).
    #[error("no seed quad could be found")]
    NoSeedFound,
    /// At least one component was labelled, but none of them passes the
    /// caller's policy (eligibility, parity, minimum size).
    #[error("no component satisfies the policy")]
    NoComponentSatisfiesPolicy,
    /// The caller asked for a (task, lattice) combination that v1 does not
    /// support — typically hex detection.
    #[error("unsupported combination: {0}")]
    UnsupportedCombination(UnsupportedCombination),
    /// Inputs are internally inconsistent (e.g. length mismatch between
    /// positions and labels, NaN positions, tag-vector smaller than the
    /// observation count).
    #[error("inconsistent input: {0}")]
    InconsistentInput(String),
}

/// Failure modes for the consistency-check task (`check_*_labels`).
#[derive(Debug, Clone, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConsistencyError {
    /// The caller passed `n` positions but `m ≠ n` labels.
    #[error("position vs label count mismatch: positions={positions}, labels={labels}")]
    CountMismatch {
        /// Number of positions supplied.
        positions: usize,
        /// Number of labels supplied.
        labels: usize,
    },
    /// The caller asked for a (task, lattice) combination that v1 does not
    /// support — typically oriented-hex evidence checks.
    #[error("unsupported combination: {0}")]
    UnsupportedCombination(UnsupportedCombination),
}

/// Combinations of `(task, lattice, evidence kind)` that v1 does not
/// implement.
///
/// Returned wrapped in [`DetectionError::UnsupportedCombination`] or
/// [`ConsistencyError::UnsupportedCombination`]. The primitive layer (lattice
/// tables, geometry, stats) is lattice-agnostic so future variants close the
/// matching `UnsupportedCombination` without changing the public surface.
#[derive(Debug, Clone, Copy, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum UnsupportedCombination {
    /// Hex detection (`detect_hex_grid`) is not implemented in v1; the
    /// primitive layer is ready, but the algorithm wiring is deferred.
    #[error("hex detection is not implemented in v1")]
    HexDetection,
    /// Hex consistency-check (`check_hex_labels`) is not implemented in v1.
    /// The square consistency check is fully implemented; hex requires an
    /// axial-coordinate fit residual computation that is deferred.
    #[error("hex consistency-check is not implemented in v1")]
    HexConsistency,
    /// Consistency-via-fit is implemented for hex; the oriented evidence
    /// path (axes / parity) is not.
    #[error("hex consistency-via-fit is implemented; oriented hex evidence is not")]
    OrientedHexEvidence,
}

/// Failure modes for component merge, called out separately from
/// [`DetectionError`] because merge is a sub-task that runs inside both
/// `detect` and `refine_grid`.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum MergeError {
    /// The caller passed a symmetry table whose [`LatticeKind`] does not
    /// match the lattice the components live on. Concrete failure mode this
    /// rules out: a future hex variant accidentally calling the merger with
    /// `D4_TRANSFORMS`, which would silently produce garbage `(i, j)` pairs.
    #[error("symmetry transforms do not match the lattice: expected {expected:?}, got {got:?}")]
    SymmetryLatticeMismatch {
        /// The lattice kind the components live on.
        expected: LatticeKind,
        /// The lattice kind of the offending symmetry transform.
        got: LatticeKind,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_error_messages_render() {
        let e = DetectionError::InsufficientObservations {
            found: 2,
            required: 4,
        };
        assert!(e.to_string().contains("got 2"));
        assert!(e.to_string().contains("4"));
    }

    #[test]
    fn unsupported_combination_displays() {
        assert_eq!(
            UnsupportedCombination::HexDetection.to_string(),
            "hex detection is not implemented in v1"
        );
    }

    #[test]
    fn merge_error_carries_lattice_kinds() {
        let e = MergeError::SymmetryLatticeMismatch {
            expected: LatticeKind::Hex,
            got: LatticeKind::Square,
        };
        let s = e.to_string();
        assert!(s.contains("Hex"));
        assert!(s.contains("Square"));
    }
}
