//! Detector pipeline: the shared stage modules the topological dispatch
//! path reuses.
//!
//! [`crate::detector::Detector`] runs the topological grid builder
//! ([`crate::topological`]); this subtree houses the post-build stages and
//! the result types both it and the recovery path consume:
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`types`] | [`ChessboardDetection`] / [`ChessboardCorner`] result types. |
//! | [`geometry_check`] | Stage 12 — the mandatory final precision gate. |
//! | [`output`] | Stage 13 — labelled grid → [`ChessboardDetection`]. |
//!
//! Clustering (Stage 2/3) and boosters (Stage 11) keep their own
//! top-level modules.

mod geometry_check;
mod output;
mod types;

pub use geometry_check::run_geometry_check;
pub use output::build_detection_from_grow;
pub use types::{ChessboardCorner, ChessboardDetection};
