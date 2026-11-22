//! The local frame of a corner and the patch's pixels: the caller's
//! homography normalised to the corner, the known-print support test, and
//! the pixel selection.

use nalgebra::{Matrix3, Vector3};

use super::model::Pixel;
use super::BlurPattern;
use crate::{GrayImageView, Homography};

/// The frame the fit works in, from the caller's homography.
pub(super) struct Frame {
    /// Corner image position, px.
    pub(super) c0: [f64; 2],
    /// Local scale, px per cell (`√|det J|`).
    pub(super) scale: f64,
    /// Smallest local scale, px per cell (smaller singular value of `J`).
    pub(super) scale_min: f64,
    /// Normalised homography entries (`h₂₂ = 1`).
    pub(super) h: [f64; 8],
    /// Image px → board units.
    pub(super) inverse: Matrix3<f64>,
}

impl Frame {
    pub(super) fn new(h: &Homography<f64>) -> Option<Self> {
        let m = h.h;
        if !(m[(2, 2)].is_finite() && m[(2, 2)].abs() > 1e-12) {
            return None;
        }
        let m = m / m[(2, 2)];
        let c0 = [m[(0, 2)], m[(1, 2)]];
        // ∂x/∂q at q = 0 (w = 1 there).
        let j = [
            [m[(0, 0)] - c0[0] * m[(2, 0)], m[(0, 1)] - c0[0] * m[(2, 1)]],
            [m[(1, 0)] - c0[1] * m[(2, 0)], m[(1, 1)] - c0[1] * m[(2, 1)]],
        ];
        let det = j[0][0] * j[1][1] - j[0][1] * j[1][0];
        let scale = det.abs().sqrt();
        let fro2 = j[0][0].powi(2) + j[0][1].powi(2) + j[1][0].powi(2) + j[1][1].powi(2);
        // Singular values: s² = (F ± √(F² − 4 det²)) / 2.
        let disc = (fro2 * fro2 - 4.0 * det * det).max(0.0).sqrt();
        let scale_min = (0.5 * (fro2 - disc)).max(0.0).sqrt();
        if !(scale.is_finite() && scale > 1.0 && scale_min > 0.5) {
            return None;
        }
        // Normalised: H̃ = N · H, N(x) = (x − c₀)/s.
        let n = Matrix3::new(
            1.0 / scale,
            0.0,
            -c0[0] / scale,
            0.0,
            1.0 / scale,
            -c0[1] / scale,
            0.0,
            0.0,
            1.0,
        );
        let hn = n * m;
        let hn = hn / hn[(2, 2)];
        let inverse = m.try_inverse()?;
        Some(Self {
            c0,
            scale,
            scale_min,
            h: [
                hn[(0, 0)],
                hn[(0, 1)],
                hn[(0, 2)],
                hn[(1, 0)],
                hn[(1, 1)],
                hn[(1, 2)],
                hn[(2, 0)],
                hn[(2, 1)],
            ],
            inverse,
        })
    }

    pub(super) fn to_board(&self, x: f64, y: f64) -> [f64; 2] {
        let v = self.inverse * Vector3::new(x, y, 1.0);
        [v.x / v.z, v.y / v.z]
    }

    pub(super) fn to_image(&self, q: [f64; 2]) -> [f64; 2] {
        let h = &self.h;
        let w = h[6] * q[0] + h[7] * q[1] + 1.0;
        [
            self.c0[0] + self.scale * (h[0] * q[0] + h[1] * q[1] + h[2]) / w,
            self.c0[1] + self.scale * (h[3] * q[0] + h[4] * q[1] + h[5]) / w,
        ]
    }
}

/// Whether `q` (board units) lies at least `m` inside the pattern's known
/// region and `m` clear of every uncertain region.
fn in_support(pattern: &BlurPattern, q: [f64; 2], m: f64) -> bool {
    let (x0, x1) = ((q[0] - m).floor() as i32, (q[0] + m).floor() as i32);
    let (y0, y1) = ((q[1] - m).floor() as i32, (q[1] + m).floor() as i32);
    for b in y0..=y1 {
        for a in x0..=x1 {
            if !pattern.is_known(a, b) {
                return false;
            }
        }
    }
    pattern.uncertain.iter().all(|region| {
        // Outward signed distance to every edge ≤ m ⇒ within m of the
        // (convex) region; conservative at its corners.
        let v = region;
        let n = v.len();
        let o = super::gauss_polygon::signed_area(v).signum();
        let mut within = true;
        for i in 0..n {
            let (a, b) = (v[i], v[(i + 1) % n]);
            let e = [b[0] - a[0], b[1] - a[1]];
            let len = e[0].hypot(e[1]);
            if len <= 0.0 {
                continue;
            }
            let d = o * ((q[0] - a[0]) * e[1] - (q[1] - a[1]) * e[0]) / len;
            if d > m {
                within = false;
                break;
            }
        }
        !within
    })
}

/// The patch's pixels: the board square `[−ρ, ρ]²`, `margin_px` clear of the
/// unknown print, not clipped at 0 or 255.
pub(super) fn select_pixels(
    image: &GrayImageView<'_>,
    pattern: &BlurPattern,
    frame: &Frame,
    rho: f64,
    margin_px: f64,
) -> Vec<Pixel> {
    let m = margin_px / frame.scale_min;
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        let p = frame.to_image([sx * rho, sy * rho]);
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    if !(lo[0].is_finite() && lo[1].is_finite() && hi[0].is_finite() && hi[1].is_finite()) {
        return Vec::new();
    }
    let x0 = lo[0].ceil().max(0.0) as i64;
    let y0 = lo[1].ceil().max(0.0) as i64;
    let x1 = (hi[0].floor() as i64).min(image.width as i64 - 1);
    let y1 = (hi[1].floor() as i64).min(image.height as i64 - 1);
    let mut out = Vec::new();
    for y in y0..=y1 {
        for x in x0..=x1 {
            let q = frame.to_board(x as f64, y as f64);
            if q[0].abs() > rho || q[1].abs() > rho || !in_support(pattern, q, m) {
                continue;
            }
            let v = image.data[y as usize * image.width + x as usize];
            if v == 0 || v == 255 {
                continue;
            }
            out.push(Pixel {
                x: x as i32,
                y: y as i32,
                value: f64::from(v),
            });
        }
    }
    out
}

/// `[x0, y0, x1, y1]` of the pixel centres.
pub(super) fn pixel_bounds(pixels: &[Pixel]) -> [f64; 4] {
    let mut b = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for p in pixels {
        let (x, y) = (f64::from(p.x), f64::from(p.y));
        b[0] = b[0].min(x);
        b[1] = b[1].min(y);
        b[2] = b[2].max(x);
        b[3] = b[3].max(y);
    }
    b
}
