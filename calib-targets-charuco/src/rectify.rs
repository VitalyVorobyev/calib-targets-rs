use nalgebra as na;
use thiserror;

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

#[derive(Clone, Copy, Debug)]
pub struct Point2f {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Homography {
    // row-major 3x3
    pub h: [[f64; 3]; 3],
}

impl Homography {
    #[inline]
    pub fn apply(&self, p: Point2f) -> Point2f {
        let x = p.x as f64;
        let y = p.y as f64;
        let w = self.h[2][0] * x + self.h[2][1] * y + self.h[2][2];
        let u = (self.h[0][0] * x + self.h[0][1] * y + self.h[0][2]) / w;
        let v = (self.h[1][0] * x + self.h[1][1] * y + self.h[1][2]) / w;
        Point2f {
            x: u as f32,
            y: v as f32,
        }
    }

    pub fn inverse(&self) -> Option<Self> {
        let m = na::Matrix3::<f64>::from_row_slice(&[
            self.h[0][0],
            self.h[0][1],
            self.h[0][2],
            self.h[1][0],
            self.h[1][1],
            self.h[1][2],
            self.h[2][0],
            self.h[2][1],
            self.h[2][2],
        ]);
        m.try_inverse().map(|inv| Homography {
            h: [
                [inv[(0, 0)], inv[(0, 1)], inv[(0, 2)]],
                [inv[(1, 0)], inv[(1, 1)], inv[(1, 2)]],
                [inv[(2, 0)], inv[(2, 1)], inv[(2, 2)]],
            ],
        })
    }
}

#[derive(thiserror::Error, Debug)]
pub enum RectifyError {
    #[error("not enough labeled inlier corners with grid coords (need >=4)")]
    NotEnoughPoints,
    #[error("homography estimation failed")]
    HomographyFailed,
    #[error("homography not invertible")]
    NonInvertible,
}
