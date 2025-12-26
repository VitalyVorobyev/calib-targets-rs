//! High-level facade crate for the `calib-targets-*` workspace.
//!
//! This crate provides:
//! - stable, convenient re-exports of the underlying detector crates
//! - (feature-gated) end-to-end helpers that run a ChESS corner detector
//!   (`chess-corners`) and then run a target detector on an image or raw buffer.

pub use calib_targets_aruco as aruco;
pub use calib_targets_charuco as charuco;
pub use calib_targets_chessboard as chessboard;
pub use calib_targets_core as core;
pub use calib_targets_marker as marker;

pub use calib_targets_core::{Corner, GridCoords, LabeledCorner, TargetDetection, TargetKind};

#[cfg(feature = "image")]
pub mod detect;
