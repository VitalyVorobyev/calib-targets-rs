//! High-level facade crate for the `calib-targets-*` workspace.
//!
//! This crate provides:
//! - stable, convenient re-exports of the underlying detector crates
//! - (feature-gated) end-to-end helpers that run a ChESS corner detector
//!   (`chess-corners`) and then run a target detector on an image or raw buffer.
//!
//! ## Quickstart
//!
//! ```no_run
//! use calib_targets::detect;
//! use calib_targets::chessboard::ChessboardParams;
//! use image::ImageReader;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let img = ImageReader::open("board.png")?.decode()?.to_luma8();
//! let chess_cfg = detect::default_chess_config();
//! let params = ChessboardParams::default();
//!
//! let result = detect::detect_chessboard(&img, &chess_cfg, params);
//! println!("detected: {}", result.is_some());
//! # Ok(())
//! # }
//! ```
//!
//! ## Python bindings
//!
//! Python bindings live in `crates/calib-targets-py` and expose the
//! `calib_targets` module. See `python/README.md` in the repository for setup
//! and the `detect_*` APIs. Config inputs accept typed Python classes or dict
//! overrides. For marker boards, `target_position` is populated only when
//! `params["layout"]["cell_size"]` is provided and alignment succeeds.
//!
//! ## API map
//! - `calib_targets::core`: core types (corners, grids, homographies, images).
//! - `calib_targets::chessboard`: chessboard detection from ChESS corners.
//! - `calib_targets::aruco`: ArUco/AprilTag dictionaries and marker decoding.
//! - `calib_targets::charuco`: ChArUco board alignment and IDs.
//! - `calib_targets::marker`: checkerboard + circle marker boards.
//! - `calib_targets::detect` (feature `image`): end-to-end helpers from `image::GrayImage`.
//!
//! ## Performance
//!
//! Benchmarks are coming. The goal is to be the fastest detector in this class
//! while maintaining high sensitivity and accuracy.

pub use calib_targets_aruco as aruco;
pub use calib_targets_charuco as charuco;
pub use calib_targets_chessboard as chessboard;
pub use calib_targets_core as core;
pub use calib_targets_marker as marker;

pub use calib_targets_chessboard::ChessboardParams;
pub use calib_targets_core::{Corner, GridCoords, LabeledCorner, TargetDetection, TargetKind};

#[cfg(feature = "image")]
pub mod detect;
