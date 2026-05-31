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

#![warn(missing_docs)]

pub mod check;
pub mod detect;
pub mod error;
pub mod feature;
pub mod float;
pub mod geometry;
pub mod lattice;
pub mod result;

pub use crate::check::{check_consistency, ConsistencyParams, ConsistencyRequest};
pub use crate::detect::{
    detect_grid, detect_grid_all, DetectionParams, DetectionReport, DetectionRequest, Evidence,
    SquareAlgorithm, TopologicalParams,
};
pub use crate::error::{EvidenceKind, GridError, GridTask};
pub use crate::feature::{CoordinateHypothesis, LocalAxis, OrientedFeature, PointFeature};
pub use crate::float::Float;
pub use crate::lattice::{
    Coord, GridDimensions, GridTransform, LatticeKind, D4_TRANSFORMS, D6_TRANSFORMS,
    HEX_AXIAL_OFFSETS, SQUARE_CARDINAL_OFFSETS,
};
pub use crate::result::{
    ConsistencyReport, GridEntry, GridSolution, LabelledGrid, LatticeFit, RejectedFeature,
    RejectionReason, ResidualSummary,
};
