//! Helpers for rendering PuzzleBoard targets.
//!
//! The actual SVG / PNG renderer lives in the `calib-targets-print` crate;
//! this module exposes the **authoritative** master-map bit lookups needed
//! to place dots so that detector and renderer stay in lockstep.
//!
//! ## Dot convention
//!
//! The renderer places a filled circle at every interior edge midpoint of the
//! checkerboard with diameter `dot_diameter_rel * square_size`. The fill colour
//! encodes the bit:
//!
//! - `bit = 0` → **white** dot
//! - `bit = 1` → **black** dot
//!
//! This matches the detector sampling convention.

use crate::code_maps::{horizontal_edge_bit, vertical_edge_bit};

/// Query the expected dot colour for the horizontal edge at master
/// coordinates `(master_row, master_col)`.
///
/// `master_row` / `master_col` are absolute row / col indices into the
/// 501×501 master pattern, *not* local board indices. Returns
/// `true` for a white dot, `false` for a black dot.
#[inline]
pub fn horizontal_edge_is_white(master_row: i32, master_col: i32) -> bool {
    horizontal_edge_bit(master_row, master_col) == 0
}

/// Dual to [`horizontal_edge_is_white`] for vertical edges.
#[inline]
pub fn vertical_edge_is_white(master_row: i32, master_col: i32) -> bool {
    vertical_edge_bit(master_row, master_col) == 0
}
