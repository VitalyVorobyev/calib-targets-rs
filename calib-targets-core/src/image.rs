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
