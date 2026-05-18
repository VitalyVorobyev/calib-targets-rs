/// Borrowed view over an 8-bit grayscale image.
///
/// A lightweight, image-crate-free pixel container: the workspace passes
/// these into detectors so `calib-targets-core` stays decoupled from any
/// concrete image type.
#[derive(Clone, Copy, Debug)]
pub struct GrayImageView<'a> {
    /// Image width in pixels.
    pub width: usize,
    /// Image height in pixels.
    pub height: usize,
    /// Row-major pixel buffer; length must equal `width * height`.
    pub data: &'a [u8],
}

/// Owned 8-bit grayscale image.
///
/// The owning counterpart of [`GrayImageView`]. Call [`GrayImage::view`]
/// to borrow it for a detector.
#[derive(Clone, Debug)]
pub struct GrayImage {
    /// Image width in pixels.
    pub width: usize,
    /// Image height in pixels.
    pub height: usize,
    /// Row-major pixel buffer; length must equal `width * height`.
    pub data: Vec<u8>,
}

impl GrayImage {
    /// Borrow this image as a [`GrayImageView`].
    pub fn view(&self) -> GrayImageView<'_> {
        GrayImageView {
            width: self.width,
            height: self.height,
            data: &self.data,
        }
    }
}

#[inline]
fn get_gray(src: &GrayImageView<'_>, x: i32, y: i32) -> u8 {
    if x < 0 || y < 0 || x >= src.width as i32 || y >= src.height as i32 {
        return 0;
    }
    src.data[y as usize * src.width + x as usize]
}

/// Bilinearly sample a grayscale image at sub-pixel `(x, y)`.
///
/// Coordinates are in pixels; pixel centers sit at integer coordinates.
/// Samples whose 2×2 footprint falls outside the image treat the missing
/// pixels as `0` (zero-padding). Returns the interpolated value as `f32`.
#[inline]
pub fn sample_bilinear(src: &GrayImageView<'_>, x: f32, y: f32) -> f32 {
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
    a + fy * (b - a)
}

/// Bilinearly sample a grayscale image, skipping bounds checks on the hot
/// path.
///
/// Identical result to [`sample_bilinear`], but when the 2×2 footprint is
/// fully inside the image it indexes the buffer directly. Out-of-range
/// footprints fall back to [`sample_bilinear`] (zero-padding semantics).
#[inline]
pub fn sample_bilinear_fast(src: &GrayImageView<'_>, x: f32, y: f32) -> f32 {
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;

    if x0 < 0 || y0 < 0 || x0 + 1 >= src.width as i32 || y0 + 1 >= src.height as i32 {
        return sample_bilinear(src, x, y);
    }

    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let base = y0 as usize * src.width + x0 as usize;

    let p00 = src.data[base] as f32;
    let p10 = src.data[base + 1] as f32;
    let p01 = src.data[base + src.width] as f32;
    let p11 = src.data[base + src.width + 1] as f32;

    let a = p00 + fx * (p10 - p00);
    let b = p01 + fx * (p11 - p01);
    a + fy * (b - a)
}

/// Bilinearly sample a grayscale image and round the result to `u8`.
///
/// Wraps [`sample_bilinear`], clamping the interpolated value to
/// `0..=255` before truncation.
#[inline]
pub fn sample_bilinear_u8(src: &GrayImageView<'_>, x: f32, y: f32) -> u8 {
    sample_bilinear(src, x, y).clamp(0.0, 255.0) as u8
}
