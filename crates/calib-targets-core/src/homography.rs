//! Homography compatibility shim.
//!
//! Re-exports the legacy `projective_grid::Homography` (default `F = f32`)
//! and its estimators so downstream code that writes bare `Homography`
//! (and freely crosses values to the legacy crate's other free functions)
//! keeps the type-identity invariant during the
//! `projective-grid → projective-grid-next` migration window.
//!
//! Bridges to [`projective_grid_next::Homography<f32>`] are provided as
//! free functions for callers that want to interoperate with the new
//! crate's surface — but the canonical `Homography` re-export stays on
//! the legacy type until Phase 8 collapses the two crates.

use crate::{sample_bilinear_u8, GrayImage, GrayImageView};
use nalgebra::Point2;

pub use projective_grid::estimate_homography as estimate_homography_rect_to_img;
pub use projective_grid::{homography_from_4pt, Homography};

/// Convert the legacy [`Homography`] into the [`projective_grid_next`]
/// crate's `Homography<f32>`. The underlying 3×3 matrix is copied byte-for
/// -byte.
#[inline]
pub fn homography_to_next(h: Homography) -> projective_grid_next::Homography<f32> {
    projective_grid_next::Homography::new(h.h)
}

/// Project a [`projective_grid_next::Homography<f32>`] back into the legacy
/// shape.
#[inline]
pub fn homography_from_next(h: projective_grid_next::Homography<f32>) -> Homography {
    Homography::new(h.h)
}

/// Warp into rectified image: for each dst pixel, map to src via H_img_from_rect and sample.
pub fn warp_perspective_gray(
    src: &GrayImageView<'_>,
    h_img_from_rect: Homography,
    out_w: usize,
    out_h: usize,
) -> GrayImage {
    let mut out = vec![0u8; out_w * out_h];

    for y in 0..out_h {
        for x in 0..out_w {
            // sample at pixel center
            let pr = Point2::new(x as f32 + 0.5, y as f32 + 0.5);
            let pi = h_img_from_rect.apply(pr);
            let v = sample_bilinear_u8(src, pi.x, pi.y);
            out[y * out_w + x] = v;
        }
    }

    GrayImage {
        width: out_w,
        height: out_h,
        data: out,
    }
}
