//! Projective homography: image-warp and `nalgebra::Projective2` bridges over
//! the workspace's single-source homography estimator.
//!
//! The [`Homography`] type, [`HomographyQuality`] metrics, and the
//! Hartley-normalised DLT estimators live in the lowest crate,
//! [`projective_grid::geometry`], so the algorithm has exactly one
//! implementation. This module re-exports them and adds only the pieces that
//! need core-level image types ([`warp_perspective_gray`]) or the
//! `nalgebra::Projective2` representation used by the grid-alignment bridge
//! ([`homography_to_next`] / [`homography_from_next`]).
//!
//! Homographies map source-frame points to destination-frame points as
//! `p_dst ~ H * p_src`. Detector code uses this for rectified-grid to
//! image-frame mappings; residuals are measured in image pixels.

use crate::{sample_bilinear_u8, GrayImage, GrayImageView};
use nalgebra::{Point2, Projective2};

// Single source of truth for the homography type, quality metrics, and the
// Hartley-normalised DLT estimators: defined in `projective_grid::geometry` and
// re-exported here so detectors keep importing them from `calib_targets_core`.
// `estimate_homography_rect_to_img` is core's descriptive alias for the generic
// `estimate_homography` — source points are the rectified board, destination
// points are image pixels, so the returned `H` maps board → image.
pub use projective_grid::geometry::{
    estimate_homography as estimate_homography_rect_to_img, estimate_homography_with_quality,
    homography_from_4pt, homography_from_4pt_with_quality, Homography, HomographyQuality,
};

/// Convert [`Homography`] into the next crate's projective transform
/// representation. The underlying 3×3 matrix is copied directly.
#[inline]
pub fn homography_to_next(h: Homography<f32>) -> Projective2<f32> {
    Projective2::from_matrix_unchecked(h.h)
}

/// Project a [`Projective2<f32>`] back into [`Homography`].
#[inline]
pub fn homography_from_next(h: Projective2<f32>) -> Homography<f32> {
    Homography::new(h.into_inner())
}

/// Warp into a rectified image: for each destination pixel center, map to
/// source image coordinates via `H_img_from_rect` and bilinear-sample.
pub fn warp_perspective_gray(
    src: &GrayImageView<'_>,
    h_img_from_rect: Homography,
    out_w: usize,
    out_h: usize,
) -> GrayImage {
    let mut out = vec![0u8; out_w * out_h];

    for y in 0..out_h {
        for x in 0..out_w {
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

#[cfg(test)]
mod tests {
    use super::*;
    use nalgebra::Matrix3;

    fn assert_close(a: Point2<f32>, b: Point2<f32>, tol: f32) {
        let dx = (a.x - b.x).abs();
        let dy = (a.y - b.y).abs();
        assert!(
            dx < tol && dy < tol,
            "expected ({:.6},{:.6}) ~ ({:.6},{:.6}) within {}",
            a.x,
            a.y,
            b.x,
            b.y,
            tol
        );
    }

    /// Smoke-test that the re-exported `estimate_homography_rect_to_img` alias
    /// resolves from `calib_targets_core` and recovers a clean grid. The
    /// estimator itself is exhaustively tested in `projective_grid::geometry`
    /// (4-point, overdetermined, f64, and a random reference-SVD battery); this
    /// only guards the core-level re-export alias.
    #[test]
    fn reexported_estimate_alias_recovers_clean_grid() {
        let ground_truth = Homography::new(Matrix3::new(
            1.0, 0.2, 12.0, -0.1, 0.9, 6.0, 0.0006, 0.0004, 1.0,
        ));
        let rect: Vec<Point2<f32>> = (0..3)
            .flat_map(|y| (0..3).map(move |x| Point2::new(x as f32 * 40.0, y as f32 * 50.0)))
            .collect();
        let img: Vec<Point2<f32>> = rect.iter().map(|&p| ground_truth.apply(p)).collect();

        let estimated = estimate_homography_rect_to_img(&rect, &img).expect("estimate");
        for p in [
            Point2::new(0.0_f32, 0.0),
            Point2::new(60.0, 40.0),
            Point2::new(80.0, 90.0),
        ] {
            assert_close(estimated.apply(p), ground_truth.apply(p), 1e-3);
        }
    }

    /// Core-specific: the `nalgebra::Projective2` bridge round-trips the matrix
    /// exactly (this pair lives in core because it needs `Projective2`).
    #[test]
    fn projective2_bridge_round_trips() {
        let h = Homography::new(Matrix3::new(
            1.2, 0.1, 5.0, -0.05, 0.9, 3.0, 0.001, 0.0005, 1.0,
        ));
        let bridged = homography_from_next(homography_to_next(h));
        for r in 0..3 {
            for c in 0..3 {
                assert_eq!(bridged.h[(r, c)], h.h[(r, c)]);
            }
        }
    }
}
