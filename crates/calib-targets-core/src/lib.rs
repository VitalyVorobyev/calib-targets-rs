//! Core types and utilities for calibration target detection.
//!
//! This crate is intentionally small and purely geometric. It does *not*
//! depend on any concrete corner detector implementation or image type, but it
//! owns the shared detector configuration contracts used across the workspace.
//!
//! ## Quickstart
//!
//! ```
//! use calib_targets_core::{LabeledCorner, TargetDetection, TargetKind};
//! use nalgebra::Point2;
//!
//! let corner = LabeledCorner::new(Point2::new(12.0, 8.0), 0.9);
//! let detection = TargetDetection::new(TargetKind::Chessboard, vec![corner]);
//!
//! println!("{}", detection.corners.len());
//! ```
//!
//! ## Includes
//!
//! - Homography estimation and warping helpers.
//! - Lightweight grayscale image views and sampling.
//! - Grid alignment and target detection types.
#![deny(missing_docs)]

mod bit_likelihood;
mod chess;
mod corner;
mod corner_map;
mod grid_alignment;
mod grid_smoothness;
mod homography;
mod image;
pub mod io;
mod logger;
mod rectify;

pub use bit_likelihood::log_sigmoid;
pub use grid_smoothness::square_predict_grid_position;
pub use homography::{
    estimate_homography_rect_to_img, estimate_homography_with_quality, homography_from_4pt,
    homography_from_4pt_with_quality, homography_from_next, homography_to_next,
    warp_perspective_gray, Homography, HomographyQuality,
};
pub use image::{
    sample_bilinear, sample_bilinear_fast, sample_bilinear_u8, GrayImage, GrayImageView,
};
pub use rectify::{RectToImgMapper, RectifiedView};

// Only the two `chess-corners` types the workspace's own public API
// legitimately exposes are re-exported: `DetectorConfig` is the ChESS config
// object a consumer constructs, `OrientationMethod` is the documented
// orientation knob. Advanced ChESS tuning types are imported from the
// `chess-corners` crate directly, where they belong — re-exporting the whole
// upstream surface would freeze it into this crate's semver contract.
pub use chess::{DetectorConfig, OrientationMethod};
pub use corner::{
    axis_estimate_from_next, axis_estimate_to_next, AxisEstimate, GridCoords, LabeledCorner,
    TargetDetection, TargetKind,
};
pub use corner_map::{complete_cell_corners, corner_map_bounds, CornerMap};
pub use grid_alignment::{
    cell_rect_corners_at, grid_alignment_from_next, grid_alignment_to_next, grid_coords_from_next,
    grid_coords_to_next, grid_transform_from_next, grid_transform_to_next, GridAlignment,
    GridTransform, GRID_TRANSFORMS_D4,
};

#[cfg(feature = "tracing")]
pub use logger::init_tracing;

pub use logger::init_with_level;
