//! [`Event<F>`]: typed per-stage events emitted to a
//! [`DiagnosticSink<F>`](super::DiagnosticSink).
//!
//! Replaces the legacy `TopologicalStats` counter-bag. Counter aggregation
//! lives in [`super::stats::CounterStats`] and consumes a `RecordingSink`
//! post-hoc; adding a new pipeline stage is a new event variant, not a
//! field that everything has to special-case.

use std::time::Duration;

use crate::float::Float;
use crate::lattice::Coord;

/// Identifies the pipeline stage emitting an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Stage {
    /// Seed-quad finder.
    Seed,
    /// BFS grow engine (square seed-and-grow).
    Grow,
    /// Topological pipeline (Delaunay + classify + walk).
    Topological,
    /// Refinement: interior hole fill + boundary extension.
    Refine,
    /// Component merge (overlap-based or predicted).
    Merge,
    /// Precision-gate validation.
    Validate,
}

/// Per-edge classification reported by the topological pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EdgeClass {
    /// Half-edge aligns with one of the corner's grid axes; counts as a
    /// real grid edge.
    Grid,
    /// Half-edge runs at ~45° to the corner's axes; counts as a diagonal
    /// crossing the cell.
    Diagonal,
    /// Half-edge matches neither axis nor diagonal; the corresponding
    /// triangle is rejected as having a spurious edge.
    Spurious,
    /// Endpoints have insufficient axis information to classify.
    Unknown,
}

/// Why the BFS grow engine rejected a candidate. Float-generic because some
/// reasons carry numeric evidence (ambiguity ratios, edge lengths).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum GrowRejectReason<F: Float> {
    /// No KD-tree hit inside the search radius.
    NoCandidate,
    /// The nearest and second-nearest hits are too close in distance to
    /// commit to one.
    Ambiguous {
        /// Distance from predicted position to the nearest candidate.
        nearest: F,
        /// Distance from predicted position to the runner-up.
        second: F,
        /// `nearest / second` — values close to `1.0` are ambiguous.
        ratio: F,
    },
    /// `Context::edge_ok` failed on at least one of the candidate's
    /// labelled neighbour edges.
    EdgeFailure,
    /// `LabelPolicy::agrees` returned `false` — tag-vs-coord parity
    /// mismatch.
    PolicyDisagreed,
    /// Candidate is marked ineligible by the policy.
    Ineligible,
}

/// Why the topological pipeline rejected a quad.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum QuadRejectReason {
    /// Triangle pair does not satisfy the diamond / parallelogram topology
    /// the algorithm needs.
    Topology,
    /// Opposing edges differ in length by more than the configured ratio.
    OpposingEdgeRatio,
    /// At least one edge falls outside the per-image length band.
    EdgeLengthBounds,
    /// `Context::quad_label_ok` returned `false` (caller's policy rejects
    /// this quad).
    PolicyDisagreed,
}

/// Why the component merger rejected a candidate pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MergeRejectReason {
    /// Neither overlap nor prediction yielded enough evidence.
    NoOverlap,
    /// The two components disagree on cell size by more than the policy
    /// allows.
    CellSizeDisagreement,
    /// Position residual at overlapping labels exceeds the tolerance.
    PositionResidual,
    /// The symmetry table the caller supplied does not belong to the
    /// active lattice family (see [`crate::error::MergeError`]).
    SymmetryMismatch,
}

/// Why the precision-gate dropped a labelled feature.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ValidationReason<F: Float> {
    /// Row or column collinearity residual exceeded the tolerance.
    LineResidualExceeded {
        /// Measured residual in pixels.
        residual: F,
        /// Tolerance in pixels.
        tol: F,
    },
    /// Local homography reprojection residual exceeded the tolerance.
    LocalHResidualExceeded {
        /// Measured residual in pixels.
        residual: F,
        /// Tolerance in pixels.
        tol: F,
    },
    /// Per-edge length lies outside the per-image acceptance band.
    EdgeLengthOutOfBand {
        /// Edge length ÷ expected step.
        ratio: F,
        /// Lower bound of the acceptance band.
        low: F,
        /// Upper bound of the acceptance band.
        high: F,
    },
    /// Axis-slot parity does not match the adjacent labelled neighbour.
    AxisSlotParityMismatch,
}

/// A single typed pipeline event. Variants are minimal: the union covers
/// every interesting decision a stage makes without per-event allocation.
/// `&'static str` payloads (e.g. `SeedRejected::reason`) draw from a small
/// fixed taxonomy rather than free text.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Event<F: Float> {
    /// A stage started.
    StageStarted {
        /// Stage that started.
        stage: Stage,
    },
    /// A stage finished, carrying its wall-clock duration.
    StageFinished {
        /// Stage that finished.
        stage: Stage,
        /// Wall-clock duration of the stage.
        duration: Duration,
    },
    /// The seed finder accepted a quad and measured its cell size.
    SeedFound {
        /// Indices into the input observation slice.
        corners: [usize; 4],
        /// Mean edge length of the seed quad in pixels.
        cell_size: F,
    },
    /// The seed finder rejected a candidate quad. `reason` is a static
    /// string from the finder's internal taxonomy.
    SeedRejected {
        /// Reason from the finder's internal taxonomy.
        reason: &'static str,
    },
    /// The grow engine attempted to expand from `from` toward `to` and
    /// found a candidate observation index (or `None`).
    GrowAttempted {
        /// Source coordinate already labelled.
        from: Coord,
        /// Target coordinate the engine wants to label.
        to: Coord,
        /// Observation index it considered, if any.
        idx: Option<usize>,
    },
    /// The grow engine attached an observation at a coordinate.
    GrowAttached {
        /// Newly-labelled coordinate.
        coord: Coord,
        /// Index of the observation now bound to `coord`.
        idx: usize,
        /// Pixel residual between the predicted and observed position.
        residual: F,
    },
    /// The grow engine rejected a candidate at a coordinate with a reason.
    GrowRejected {
        /// Coordinate the engine was attempting to label.
        coord: Coord,
        /// Why the candidate was rejected.
        reason: GrowRejectReason<F>,
    },
    /// The topological pipeline classified a half-edge belonging to a
    /// triangle.
    TopologicalEdge {
        /// Triangle id (linear index into the Delaunay output).
        triangle: usize,
        /// Half-edge id within the triangle (0, 1, or 2).
        half_edge: usize,
        /// Classification.
        class: EdgeClass,
    },
    /// The topological pipeline decided whether to keep a quad.
    TopologicalQuad {
        /// Quad id (linear index into the merged quad list).
        id: usize,
        /// `true` when the quad survives.
        kept: bool,
        /// Reason for rejection when `kept = false`.
        reason: Option<QuadRejectReason>,
    },
    /// A connected component was assigned labels by the walker.
    ComponentLabelled {
        /// Component id.
        id: usize,
        /// How many corners ended up labelled in this component.
        n_labels: usize,
    },
    /// The component merger joined two components.
    MergeAccepted {
        /// First component id.
        a: usize,
        /// Second component id.
        b: usize,
        /// Number of labels that overlapped (or `0` for predicted-merge).
        overlap: usize,
        /// Max per-label pixel residual after alignment.
        max_residual: F,
    },
    /// The component merger rejected a candidate pair.
    MergeRejected {
        /// First component id.
        a: usize,
        /// Second component id.
        b: usize,
        /// Why the merge was rejected.
        reason: MergeRejectReason,
    },
    /// The precision gate dropped a label.
    ValidationDropped {
        /// Coordinate that was dropped.
        coord: Coord,
        /// Why.
        reason: ValidationReason<F>,
    },
}
