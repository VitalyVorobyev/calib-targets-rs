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
//! let params = ChessboardParams::default();
//!
//! match detect::detect_chessboard(&img, &detect::default_chess_config(), &params) {
//!     Ok(detection) => println!("detected {} corners", detection.corners.len()),
//!     Err(err) => println!("no board: {err}"),
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Python bindings
//!
//! Python bindings live in `crates/calib-targets-py` and expose the
//! `calib_targets` module. See `crates/calib-targets-py/README.md` in the
//! repository for setup, the `detect_*` APIs, and printable-target generation.
//! Config inputs accept typed Python classes; `detect_charuco` requires
//! `params` with `params.board`. For marker boards, `target_position` is
//! populated only when `params.board.cell_size` is provided and alignment
//! succeeds.
//!
//! ## API map
//! - `calib_targets::core`: core types (corners, grids, homographies, images).
//! - `calib_targets::chessboard`: chessboard detection from ChESS corners.
//! - `calib_targets::aruco`: ArUco/AprilTag dictionaries and marker decoding.
//! - `calib_targets::charuco`: ChArUco board alignment and IDs.
//! - `calib_targets::puzzleboard`: PuzzleBoard edge-code decoding and IDs.
//! - `calib_targets::marker`: checkerboard + circle marker boards.
//! - `calib_targets::printable`: printable target generation and JSON/SVG/PNG output.
//! - `calib_targets::detect` (feature `image`): end-to-end helpers from `image::GrayImage`.
//!
//! ## Performance
//!
//! The detectors share a single image-free grid builder (Delaunay
//! triangulation plus a local axis-driven cell test) that operates on local
//! neighbourhoods, so cost scales with the detected corner count rather than
//! raw image resolution and degrades gracefully under perspective and radial
//! distortion. Corner detection dominates the runtime on sparse boards; grid
//! assembly and marker / dot decoding dominate on dense ones. Each detector
//! crate ships Criterion benchmarks (`cargo bench`), so you can measure the
//! regime that matters for your inputs on your own hardware.
#![deny(missing_docs)]

pub use calib_targets_aruco as aruco;
pub use calib_targets_charuco as charuco;
pub use calib_targets_chessboard as chessboard;
pub use calib_targets_core as core;
pub use calib_targets_marker as marker;
pub use calib_targets_print as printable;
pub use calib_targets_puzzleboard as puzzleboard;

pub use calib_targets_chessboard::ChessCorner;
pub use calib_targets_core::{Coord, LabeledCorner, TargetDetection, TargetKind};

/// The chessboard detector's introspection entry points, re-exported so a
/// facade-only caller can reach chessboard diagnostics without naming
/// `calib-targets-chessboard` directly.
///
/// Available only with the `diagnostics` feature enabled.
#[cfg(feature = "diagnostics")]
pub use calib_targets_chessboard::{trace_topological, trace_topological_detection};

/// Re-export of the [`image`] crate the [`detect`] helpers accept.
///
/// Reach for `image` through `calib_targets::image` instead of adding a
/// separate `image = "0.25"` dependency: it guarantees your `GrayImage` type
/// is the exact one these helpers take, so the two versions cannot drift apart
/// and produce a confusing "expected `GrayImage`, found `GrayImage`" mismatch.
/// A direct dependency still works — this is purely additive.
#[cfg(feature = "image")]
pub use ::image;

#[cfg(feature = "image")]
pub mod detect;

pub mod generate;

#[cfg(feature = "cli")]
#[doc(hidden)]
pub mod cli;
