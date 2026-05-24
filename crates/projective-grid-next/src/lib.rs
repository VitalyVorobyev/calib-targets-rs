//! `projective-grid-next` — from-scratch successor to the legacy
//! `projective-grid` crate.
//!
//! Provisional crate name during the rewrite window. After the legacy crate
//! is deleted, this crate will be renamed to `projective-grid`.
//!
//! ## Vocabulary
//!
//! * [`Observation<F>`](feature::Observation) — one detected feature with
//!   sub-pixel position, two undirected axis estimates, an opaque tag, and
//!   an optional score.
//! * [`AxisEstimate<F>`](feature::AxisEstimate) — undirected local grid
//!   direction with an angular sigma.
//! * [`LatticeKind`] / [`Coord`] / [`GridTransform`] — the lattice
//!   taxonomy. Both
//!   square and hex coordinates are `(i32, i32)`; the interpretation is
//!   tagged by `LatticeKind`.
//! * [`LabelPolicy<F>`](policy::LabelPolicy) — concrete consumer-supplied
//!   parity / tag / eligibility object, shared by every pipeline context.
//!   Closes the chessboard crate's five-way parity duplication
//!   (`docs/algorithmic_gaps.md` Gap 4).
//! * [`Event<F>`](diagnostics::Event) / [`DiagnosticSink<F>`](diagnostics::DiagnosticSink)
//!   — typed event spine that replaces the legacy `TopologicalStats`
//!   counter bag.
//!
//! ## Tasks (Phase 5+)
//!
//! The crate is being rewritten in phases. Phases 1–3 ship the foundation
//! layer plus the two algorithm portfolios:
//!
//! * Square seed-and-grow: [`bfs_grow`] with [`SquareGrowContext`]; seed via
//!   [`find_quad`] with [`SeedQuadContext`].
//! * Topological: [`build_grid_topological`] with [`TopologicalContext`] —
//!   the trait surface that closes the parity-hook gap in the legacy crate.
//!
//! Phase 4 adds the refine layer (boundary extension, hole fill, component
//! merge, validation gate). Phase 5 adds the top-level task entry points
//! `detect_square_grid`, `check_square_labels`, and `refine_grid`.
//!
//! ## `Float` policy
//!
//! Every public function is generic over `F: Float`, where `Float` is a
//! sealed alias for `nalgebra::RealField + Copy + From<f32> + 'static`. The
//! crate ships no `f32`-hardcoded internal helper that would prevent f64
//! callers from getting clean genericity (Gap 2 in `docs/algorithmic_gaps.md`).

#![warn(missing_docs)]

pub mod check;
pub mod detect;
pub mod diagnostics;
pub mod error;
pub mod feature;
pub mod float;
pub mod geometry;
pub mod grow;
pub mod lattice;
pub mod merge;
pub mod policy;
pub mod refine;
pub mod refine_task;
pub mod seed;
pub mod stats;
pub mod topological;
pub mod validate;

// ---- Curated public surface (Phases 1–3) ----
//
// Foundation types, plus the two algorithm entry points (square seed-and-grow
// and topological) with their associated Context traits and parameter structs.
// `OpenContext` is intentionally NOT re-exported at the crate root because
// both `grow` and `topological` define their own; consumers access whichever
// they need via the longer path (e.g. `projective_grid_next::grow::OpenContext`).

pub use crate::diagnostics::{
    DiagnosticSink, EdgeClass, Event, GrowRejectReason, MergeRejectReason, NoOpSink,
    QuadRejectReason, RecordingSink, Stage, ValidationReason,
};
pub use crate::error::{ConsistencyError, DetectionError, MergeError, UnsupportedCombination};
pub use crate::feature::{AxisEstimate, Observation};
pub use crate::float::Float;
pub use crate::geometry::{
    dlt_conditioning, estimate_homography, estimate_homography_with_diagnostics,
    homography_from_4pt, homography_from_4pt_with_diagnostics, Affine2, DltConditioning,
    Homography, HomographyDiagnostics,
};
pub use crate::grow::{
    bfs_grow, predict_from_neighbours, EdgeCtx, GrowParams, GrowResult, LabelledNeighbour,
    PredictCtx, PredictedPosition, SquareGrowContext,
};
pub use crate::lattice::{
    Coord, GridTransform, LatticeKind, D4_TRANSFORMS, D6_TRANSFORMS, HEX_AXIAL_OFFSETS,
    SQUARE_CARDINAL_OFFSETS,
};
pub use crate::policy::{FeatureTag, LabelPolicy, LabelPolicyBuilder, ParityRule};
pub use crate::seed::{find_quad, Seed, SeedOutput, SeedQuadContext, SeedQuadParams};
pub use crate::stats::{
    estimate_global_cell_size, estimate_local_steps, GlobalStepEstimate, GlobalStepParams,
    LocalStep, LocalStepParams, LocalStepPointData,
};
pub use crate::topological::{
    build_grid_topological, TopologicalComponent, TopologicalContext, TopologicalGrid,
    TopologicalParams,
};

// ---- Phase 4 surfaces: refine + merge + validate ----

pub use crate::merge::{
    merge_components_local, ComponentInput, MergeMode, MergeParams, MergeReport, MergedComponent,
};
pub use crate::refine::extend_global::{
    extend_via_global_homography, ExtensionParams, ExtensionStats,
};
pub use crate::refine::extend_local::{extend_via_local_homography, LocalExtensionParams};
pub use crate::refine::fill::{fill_grid_holes, FillParams, FillStats};
pub use crate::validate::{
    validate, EdgeFailure, LabelledEntry, ValidationParams, ValidationResult,
};

// ---- Phase 5 task facade ----

pub use crate::check::{check_hex_labels, check_square_labels, CheckParams, CheckReport};
pub use crate::detect::{
    detect_hex_grid, detect_square_all, detect_square_grid, DetectAlgorithm, DetectParams,
    GridDetection,
};
pub use crate::refine_task::{refine_grid, RefineParams};
