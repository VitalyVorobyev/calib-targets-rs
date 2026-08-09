//! Image-free recovery of regular projective grids from 2D point evidence.
//!
//! The ordinary workflow has one entry point and production defaults:
//!
//! ```rust
//! use nalgebra::Point2;
//! use projective_grid::{
//!     detect_grid, DetectionRequest, Evidence, LatticeKind, LocalAxis,
//!     OrientedFeature, PointFeature,
//! };
//!
//! let features = vec![
//!     OrientedFeature::new(
//!         PointFeature::new(0, Point2::new(0.0, 0.0)),
//!         [LocalAxis::new(0.0, None), LocalAxis::new(1.57, None)],
//!     ),
//!     OrientedFeature::new(
//!         PointFeature::new(1, Point2::new(10.0, 0.0)),
//!         [LocalAxis::new(0.0, None), LocalAxis::new(1.57, None)],
//!     ),
//!     OrientedFeature::new(
//!         PointFeature::new(2, Point2::new(10.0, 10.0)),
//!         [LocalAxis::new(0.0, None), LocalAxis::new(1.57, None)],
//!     ),
//!     OrientedFeature::new(
//!         PointFeature::new(3, Point2::new(0.0, 10.0)),
//!         [LocalAxis::new(0.0, None), LocalAxis::new(1.57, None)],
//!     ),
//! ];
//! let request = DetectionRequest::new(
//!     LatticeKind::Square,
//!     Evidence::Oriented2(&features),
//! );
//! let detection = detect_grid(request)?;
//! # assert_eq!(detection.grid().entries().len(), 4);
//! # Ok::<(), projective_grid::GridError>(())
//! ```
//!
//! Pattern-specific detector builders use the curated [`expert`] namespace.
//! Exact intermediate evidence is available through the opt-in `diagnostics`
//! feature.

#![deny(missing_docs)]

mod check;
mod cluster;
mod detect;
#[cfg(feature = "diagnostics")]
pub mod diagnostics;
mod error;
pub mod expert;
mod feature;
mod float;
mod geometry;
mod lattice;
mod orient;
mod result;
mod shared;
mod topological;

pub use crate::check::{check_consistency, ConsistencyParams, ConsistencyRequest};
pub use crate::detect::{
    detect_grid, detect_grid_all, DetectionParams, DetectionRequest, Evidence,
};
pub use crate::error::{EvidenceKind, GridError, GridTask};
pub use crate::feature::{CoordinateHypothesis, LocalAxis, OrientedFeature, PointFeature};
pub use crate::lattice::{Coord, GridDimensions, LatticeKind};
pub use crate::result::{
    ConsistencyReport, GridDetection, GridEntry, LabelledGrid, LatticeFit, ResidualSummary,
};
