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
//! let detection = TargetDetection {
//!     kind: TargetKind::Chessboard,
//!     corners: Vec::new(),
//! };
//!
//! println!("{}", detection.corners.len());
//! ```
//!
//! ## Includes
//!
//! - Homography estimation and warping helpers.
//! - Lightweight grayscale image views and sampling.
//! - Grid alignment and target detection types.

mod chess;
mod corner;
mod grid_alignment;
mod homography;
mod image;
pub mod io;
mod logger;
mod orientation_clustering;
mod rectify;

pub use homography::{
    estimate_homography_rect_to_img, homography_from_4pt, warp_perspective_gray, Homography,
};
pub use image::{
    sample_bilinear, sample_bilinear_fast, sample_bilinear_u8, GrayImage, GrayImageView,
};
pub use rectify::{RectToImgMapper, RectifiedView};

pub use chess::{
    CenterOfMassConfig, ChessConfig, ChessCornerParams, ChessRefiner, ChessRing,
    CoarseToFineParams, DescriptorRing, DetectionStrategy, Detector, DetectorConfig,
    ForstnerConfig, MultiscaleConfig, OrientationMethod, PyramidParams, RadonConfig,
    RadonDetectorParams, RadonPeakConfig, RadonRefiner, RefinerKind, SaddlePointConfig, Threshold,
    UpscaleConfig, UpscaleConfigError,
};
pub use corner::{AxisEstimate, GridCoords, LabeledCorner, TargetDetection, TargetKind};
pub use grid_alignment::{GridAlignment, GridTransform, GRID_TRANSFORMS_D4};

#[cfg(feature = "tracing")]
pub use logger::init_tracing;

pub use logger::init_with_level;
