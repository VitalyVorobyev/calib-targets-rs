//! Target-agnostic projective grid recovery primitives.
//!
//! This crate is intentionally small at the public boundary. It models two
//! lattice families, two tasks, and explicit evidence shapes. Target-specific
//! concepts such as chessboard parity, marker IDs, ring IDs, and detector
//! cluster labels belong in caller crates and should be converted into generic
//! point features or coordinate hypotheses before entering this crate.

#![warn(missing_docs)]

pub mod check;
pub mod detect;
pub mod error;
pub mod feature;
pub mod float;
pub mod geometry;
pub mod grow;
pub mod lattice;
pub mod result;
pub mod seed;
pub mod validate;

pub use crate::check::{check_consistency, ConsistencyParams, ConsistencyRequest};
pub use crate::detect::{
    detect_grid, DetectionParams, DetectionRequest, Evidence, GrowParams, SquareAlgorithm,
    TopologicalParams,
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
pub use crate::seed::SeedParams;
pub use crate::validate::ValidateParams;
