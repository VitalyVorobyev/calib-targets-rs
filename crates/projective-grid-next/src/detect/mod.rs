//! Top-level detection tasks: [`detect_square_grid`], [`detect_square_all`],
//! [`detect_hex_grid`].
//!
//! Compose the Phase 1–4 building blocks into a single user-facing call. See
//! the [`square`] sub-module for the headline entry points; [`hex`] is a
//! placeholder that returns
//! [`UnsupportedCombination::HexDetection`](crate::UnsupportedCombination::HexDetection)
//! in v1.

pub mod hex;
pub mod square;

pub use hex::detect_hex_grid;
pub use square::{
    detect_square_all, detect_square_grid, DetectAlgorithm, DetectParams, GridDetection,
};
