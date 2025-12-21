#[derive(Clone, Copy, Debug)]
pub struct GrayImageView<'a> {
    pub width: usize,
    pub height: usize,
    pub data: &'a [u8], // row-major, len = w*h
}

#[derive(Clone, Debug)]
pub struct GrayImage {
    pub width: usize,
    pub height: usize,
    pub data: Vec<u8>,
}

#[inline]
fn get_gray(src: &GrayImageView<'_>, x: i32, y: i32) -> u8 {
    if x < 0 || y < 0 || x >= src.width as i32 || y >= src.height as i32 {
        return 0;
    }
    src.data[y as usize * src.width + x as usize]
}

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

#[inline]
pub fn sample_bilinear_u8(src: &GrayImageView<'_>, x: f32, y: f32) -> u8 {
    sample_bilinear(src, x, y).clamp(0.0, 255.0) as u8
}
