//! ArUco/AprilTag marker dictionaries and decoding utilities.
//!
//! This crate focuses on:
//! - embedded built-in dictionaries (compiled into the binary),
//! - matching observed marker codes against those dictionaries,
//! - decoding markers either from rectified grids or from per-cell image quads.
//!
//! It does **not** perform quad detection. Instead, it expects a grid model
//! (for example from `calib-targets-chessboard`) or explicit cell corners.

pub mod builtins;
mod dictionary;
mod matcher;
mod scan;
mod threshold;

pub use dictionary::Dictionary;
pub use matcher::{rotate_code_u64, Match, Matcher};
pub use scan::{
    decode_marker_in_cell, scan_decode_markers, scan_decode_markers_in_cells, ArucoScanConfig,
    MarkerCell, MarkerDetection, ScanDecodeConfig,
};
