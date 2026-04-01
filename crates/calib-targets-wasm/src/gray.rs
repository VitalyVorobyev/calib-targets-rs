//! RGBA-to-grayscale conversion and image view construction.

use calib_targets_core::GrayImageView;

/// Convert RGBA pixel buffer to grayscale using BT.601 weights.
///
/// Input: `rgba` with length `4 * width * height`, row-major RGBA.
/// Returns grayscale buffer with length `width * height`.
pub fn rgba_to_grayscale(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let npix = (width as usize) * (height as usize);
    let mut gray = Vec::with_capacity(npix);
    for i in 0..npix {
        let r = rgba[4 * i] as f32;
        let g = rgba[4 * i + 1] as f32;
        let b = rgba[4 * i + 2] as f32;
        gray.push((0.299 * r + 0.587 * g + 0.114 * b).round() as u8);
    }
    gray
}

/// Build a `GrayImageView` from a grayscale buffer.
pub fn make_view<'a>(data: &'a [u8], width: u32, height: u32) -> GrayImageView<'a> {
    GrayImageView {
        width: width as usize,
        height: height as usize,
        data,
    }
}
