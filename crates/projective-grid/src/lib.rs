//! Target-agnostic projective grid recovery primitives.
//!
//! This crate is intentionally small at the public boundary. It models two
//! lattice families, two tasks, and explicit evidence shapes. Target-specific
//! identifiers and detector classes belong in caller crates and should be
//! converted into generic point features or coordinate hypotheses before
//! entering this crate.
//!
//! # Design references
//!
//! The crate is organised around three orthogonal axes — lattice family,
//! recovery strategy, and input-feature kind. The architecture and the
//! orientation-as-an-optional-cue model are documented under `docs/` in the
//! crate source tree:
//!
//! - `docs/DESIGN.md` — the three design axes, the shared pipeline back-half,
//!   and how the lattice family extends to hex.
//! - `docs/ORIENTATION.md` — where each strategy consumes per-corner
//!   orientation and how each can run orientation-free (the dot-grid path).
//!
//! The per-strategy stage maps live in `docs/topological-grid-detection.md`
//! (repo root) and `calib-targets-chessboard/docs/PIPELINE.md`.
//!
//! # Two public tiers
//!
//! The crate exposes two tiers with different stability promises:
//!
//! * **Stable tier — the facade.** The items re-exported at the crate root
//!   ([`detect_grid`], [`detect_grid_all`], [`check_consistency`], the
//!   [`Evidence`] / [`DetectionParams`] / [`DetectionRequest`] request types,
//!   the [`GridSolution`] / [`LabelledGrid`] result types, the [`Lattice`] /
//!   [`LatticeKind`] / [`Coord`] model, the feature evidence types, and the
//!   `orient::synthesize_*` helpers, and the [`cluster_axes`]
//!   global-direction prior with its [`AxisClusterCenters`] /
//!   [`AxisAssignment`] types). This is the supported surface for
//!   external callers and follows normal semver intent.
//! * **Advanced tier — the engine modules.** [`seed_and_grow`], [`shared`],
//!   and [`topological`] expose the assembly engines the facade is built from,
//!   for in-workspace consumers (the chessboard detector) that compose the
//!   engine directly with their own policies. These are **semver-exempt
//!   pre-1.0**: items here may change shape between minor releases as the
//!   engine is refactored. Depend on the facade unless you are building a new
//!   detector on top of the engine.

#![warn(missing_docs)]

pub mod check;
pub mod cluster;
pub mod detect;
pub mod error;
pub mod feature;
pub mod float;
pub mod geometry;
pub mod lattice;
pub mod orient;
pub mod result;

pub use crate::check::{check_consistency, ConsistencyParams, ConsistencyRequest};
pub use crate::cluster::{
    cluster_axes, AxisAssignment, AxisClusterCenters, AxisClusterDebug, AxisFeature,
    AxisObservation, ClusterParams,
};
pub use crate::detect::{
    detect_grid, detect_grid_all, DetectionParams, DetectionReport, DetectionRequest, Evidence,
    SquareAlgorithm, TopologicalParams,
};
pub use crate::error::{EvidenceKind, GridError, GridTask};
pub use crate::feature::{CoordinateHypothesis, LocalAxis, OrientedFeature, PointFeature};
pub use crate::float::Float;
pub use crate::lattice::{
    Coord, GridDimensions, GridTransform, Hex, Lattice, LatticeKind, Square, D4_TRANSFORMS,
    D6_TRANSFORMS, HEX_AXIAL_OFFSETS, SQUARE_CARDINAL_OFFSETS,
};
pub use crate::orient::{synthesize_oriented2, synthesize_oriented2_from_oriented1};
pub use crate::result::{
    ConsistencyReport, GridEntry, GridSolution, LabelledGrid, LatticeFit, RejectedFeature,
    RejectionReason, ResidualSummary,
};

pub mod seed_and_grow;
pub mod shared;
pub mod topological;
