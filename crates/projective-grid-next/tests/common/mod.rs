//! Shared synthetic-grid generators for integration tests.
//!
//! Mirrors the perspective-warped / rotated / clean grids from the legacy
//! crate's `tests/regular_grid.rs::perspective_warped_grid` family. Every
//! generator is generic over `F: Float` so each test runs once for `f32`
//! and once for `f64`.

use nalgebra::{Matrix3, Point2};

use projective_grid_next::feature::Observation;
use projective_grid_next::float::Float;

/// Convert an `f32` literal to `F`.
#[inline]
pub fn lit<F: Float>(v: f32) -> F {
    <F as From<f32>>::from(v)
}

/// Build a clean axis-aligned `rows × cols` grid at spacing `s` shifted by
/// `(ox, oy)`. Row-major: index `j * cols + i`.
pub fn axis_aligned_grid<F: Float>(
    rows: i32,
    cols: i32,
    s: F,
    ox: F,
    oy: F,
) -> Vec<Observation<F>> {
    let mut out = Vec::with_capacity((rows * cols) as usize);
    for j in 0..rows {
        for i in 0..cols {
            let x = lit::<F>(i as f32) * s + ox;
            let y = lit::<F>(j as f32) * s + oy;
            out.push(Observation::new(Point2::new(x, y)));
        }
    }
    out
}

/// Build a rotated `rows × cols` grid with spacing `s` and origin offset
/// `(ox, oy)`. Rotation is CCW by `theta` radians around the origin.
pub fn rotated_grid<F: Float>(
    rows: i32,
    cols: i32,
    s: F,
    theta: F,
    ox: F,
    oy: F,
) -> Vec<Observation<F>> {
    let c = theta.cos();
    let sn = theta.sin();
    let mut out = Vec::with_capacity((rows * cols) as usize);
    for j in 0..rows {
        for i in 0..cols {
            let x = lit::<F>(i as f32) * s;
            let y = lit::<F>(j as f32) * s;
            let rx = x * c - y * sn + ox;
            let ry = x * sn + y * c + oy;
            out.push(Observation::new(Point2::new(rx, ry)));
        }
    }
    out
}

/// Build a perspective-warped `rows × cols` grid using the same homography
/// family as the legacy `tests/regular_grid.rs::perspective_warped_grid`.
pub fn perspective_warped_grid<F: Float>(rows: i32, cols: i32, scale: F) -> Vec<Observation<F>> {
    // Same matrix as legacy:
    //   [[1.0*scale, 0.10*scale, 50.0],
    //    [0.05*scale, 1.0*scale, 50.0],
    //    [2e-4,       1e-4,      1.0]]
    let h = Matrix3::new(
        lit::<F>(1.0_f32) * scale,
        lit::<F>(0.10_f32) * scale,
        lit::<F>(50.0_f32),
        lit::<F>(0.05_f32) * scale,
        lit::<F>(1.0_f32) * scale,
        lit::<F>(50.0_f32),
        lit::<F>(2e-4_f32),
        lit::<F>(1e-4_f32),
        F::one(),
    );
    let mut out = Vec::with_capacity((rows * cols) as usize);
    for j in 0..rows {
        for i in 0..cols {
            let x = lit::<F>(i as f32);
            let y = lit::<F>(j as f32);
            let w = h[(2, 0)] * x + h[(2, 1)] * y + h[(2, 2)];
            let xp = (h[(0, 0)] * x + h[(0, 1)] * y + h[(0, 2)]) / w;
            let yp = (h[(1, 0)] * x + h[(1, 1)] * y + h[(1, 2)]) / w;
            out.push(Observation::new(Point2::new(xp, yp)));
        }
    }
    out
}
