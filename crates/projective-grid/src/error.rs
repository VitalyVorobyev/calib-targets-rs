//! Error types for grid tasks.

use thiserror::Error;

use crate::lattice::LatticeKind;

/// Grid task that reported an error.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum GridTask {
    /// Recover lattice labels from features.
    Detection,
    /// Check caller-supplied coordinate hypotheses.
    Consistency,
}

/// Evidence kind supplied to a grid task.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EvidenceKind {
    /// Position-only point features.
    Positions,
    /// Features with one local axis.
    Oriented1,
    /// Features with two local axes.
    Oriented2,
    /// Features with three local axes.
    Oriented3,
    /// Caller-supplied coordinate hypotheses.
    CoordinateHypotheses,
}

/// User-facing failure modes for projective-grid tasks.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum GridError {
    /// The requested task/lattice/evidence combination is intentionally not
    /// implemented yet.
    #[error("unsupported combination: task={task:?}, lattice={lattice:?}, evidence={evidence:?}")]
    UnsupportedCombination {
        /// Requested task.
        task: GridTask,
        /// Requested lattice family.
        lattice: LatticeKind,
        /// Supplied evidence kind.
        evidence: EvidenceKind,
    },
    /// There is not enough evidence to run the requested task.
    #[error("insufficient evidence")]
    InsufficientEvidence,
    /// Input slices disagree or contain duplicate/conflicting identifiers.
    #[error("inconsistent input: {0}")]
    InconsistentInput(String),
    /// Geometry is degenerate, for example all model or image points are
    /// collinear.
    #[error("degenerate geometry")]
    DegenerateGeometry,
}

/// Convenience result alias for this crate.
pub type Result<T> = std::result::Result<T, GridError>;
