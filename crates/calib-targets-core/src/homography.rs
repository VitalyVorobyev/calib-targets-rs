use crate::{sample_bilinear_u8, GrayImage, GrayImageView};
use nalgebra::Point2;

pub use projective_grid::estimate_homography as estimate_homography_rect_to_img;
pub use projective_grid::{homography_from_4pt, Homography};

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
