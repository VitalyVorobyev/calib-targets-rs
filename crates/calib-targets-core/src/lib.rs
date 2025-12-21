//! Core types and utilities for calibration target detection.
//!
//! This crate is intentionally small and purely geometric. It does *not*
//! depend on any concrete corner detector or image type.

mod corner;
mod homography;
mod image;
mod logger;
mod orientation_clustering;
mod rectify;

pub use homography::{
    estimate_homography_rect_to_img, homography_from_4pt, warp_perspective_gray, Homography,
};
pub use image::{sample_bilinear, sample_bilinear_u8, GrayImage, GrayImageView};
pub use rectify::{RectToImgMapper, RectifiedView};

pub use corner::{Corner, GridCoords, LabeledCorner, TargetDetection, TargetKind};

#[cfg(feature = "tracing")]
pub use logger::init_tracing;

pub use logger::init_with_level;
pub use orientation_clustering::{
    cluster_orientations, compute_orientation_histogram, estimate_grid_axes_from_orientations,
    OrientationClusteringParams, OrientationClusteringResult, OrientationHistogram,
};
