use crate::rectify::{GrayImage, GrayImageView, Homography, Point2f};

#[inline]
fn get_gray(src: &GrayImageView<'_>, x: i32, y: i32) -> u8 {
    if x < 0 || y < 0 || x >= src.width as i32 || y >= src.height as i32 {
        return 0;
    }
    src.data[y as usize * src.width + x as usize]
}

fn sample_bilinear(src: &GrayImageView<'_>, x: f32, y: f32) -> u8 {
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;

    let p00 = get_gray(src, x0, y0) as f32;
    let p10 = get_gray(src, x0 + 1, y0) as f32;
    let p01 = get_gray(src, x0, y0 + 1) as f32;
    let p11 = get_gray(src, x0 + 1, y0 + 1) as f32;

    let a = p00 + fx * (p10 - p00);
    let b = p01 + fx * (p11 - p01);
    let v = a + fy * (b - a);

    v.clamp(0.0, 255.0) as u8
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
            let pr = Point2f::new(x as f32 + 0.5, y as f32 + 0.5);
            let pi = h_img_from_rect.apply(pr);
            let v = sample_bilinear(src, pi.x, pi.y);
            out[y * out_w + x] = v;
        }
    }

    GrayImage {
        width: out_w,
        height: out_h,
        data: out,
    }
}
