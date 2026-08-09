//! Topological grid detection.
//!
//! This module is deliberately only the stage map. The square orchestration
//! lives in `square_detector`; individual geometry stages remain isolated in
//! their own modules, and diagnostics observe that same implementation.

mod axis;
mod classify;
mod delaunay;
mod filter;
pub(crate) mod hex;
mod hex_detect;
mod quads;
mod square_detector;
mod walk;

pub(crate) use hex_detect::detect_hex_oriented3_topological_all;
pub use square_detector::TopologicalParams;
pub(crate) use square_detector::{
    assemble_square_oriented2_components, detect_square_oriented2_all,
};

#[cfg(feature = "diagnostics")]
pub mod trace;
