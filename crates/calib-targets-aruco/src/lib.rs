//! ArUco/AprilTag marker dictionaries and decoding utilities.
//!
//! This crate focuses on:
//! - embedded built-in dictionaries (compiled into the binary),
//! - matching observed marker codes against those dictionaries,
//! - decoding markers either from rectified grids or from per-cell image quads.
//!
//! It does **not** perform quad detection. Instead, it expects a grid model
//! (for example from `calib-targets-chessboard`) or explicit cell corners.
//!
//! ## Quickstart
//!
//! ```
//! use calib_targets_aruco::{builtins, scan_decode_markers, Matcher, ScanDecodeConfig};
//! use calib_targets_core::GrayImageView;
//!
//! let dict = builtins::builtin_dictionary("DICT_4X4_50").expect("dict");
//! let matcher = Matcher::new(dict, 1);
//!
//! let pixels = vec![0u8; 16 * 16];
//! let view = GrayImageView {
//!     width: 16,
//!     height: 16,
//!     data: &pixels,
//! };
//!
//! let scan_cfg = ScanDecodeConfig::default();
//! let markers = scan_decode_markers(&view, 4, 4, 4.0, &scan_cfg, &matcher);
//! println!("markers: {}", markers.len());
//! ```

pub mod builtins;
mod dictionary;
mod matcher;
mod scan;
mod threshold;

pub use dictionary::Dictionary;
pub use matcher::{rotate_code_u64, Match, Matcher};
pub use scan::{
    decode_marker_in_cell, scan_decode_markers, scan_decode_markers_in_cells, ArucoScanConfig,
    BoardCell, GridCell, MarkerCell, MarkerDetection, ScanDecodeConfig,
};
