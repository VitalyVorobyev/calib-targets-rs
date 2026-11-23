//! Spatially varying blur field and scatter-PSF corner bias correction.
//!
//! On real lenses — and especially on tilted sensors under the Scheimpflug
//! condition — the defocus PSF covariance `Σ(x)` varies smoothly across the
//! image. Even when a corner's own patch fits `Σ` accurately, a spatially
//! varying *scatter-form* PSF
//! `I(x) = ∫ B(x') N(x; x', Σ(x')) dx'` shifts each straight edge of unit
//! normal `n` by
//!
//! ```text
//! shift(n) = n · (∇ · Σ) − ½ Σ_{k=1}^2 n_k (nᵀ (∂Σ/∂x_k) n),
//! ```
//!
//! where `∇ · Σ` is the tensor divergence `(∂Σ_1j/∂x_1 + ∂Σ_2j/∂x_2)`.
//! Solving the two edge equations of an X-junction recovers the exact
//! 2-D scatter bias `Δx_bias` with no fitted heuristics.

use nalgebra::{DMatrix, DVector, Matrix2, Point2, Vector2};
use serde::{Deserialize, Serialize};

use super::{BlurKernel, CornerBlur};
use crate::sym2::Sym2x2;

/// First-order spatial gradient of the blur covariance `Σ(x, y)` in pixels²
/// per pixel.
///
/// `#[non_exhaustive]`: construct with [`BlurGradient::new`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct BlurGradient {
    /// `∂Σ/∂x` as `(∂Σxx/∂x, ∂Σxy/∂x, ∂Σyy/∂x)`, px²/px.
    pub d_sigma_dx: [f32; 3],
    /// `∂Σ/∂y` as `(∂Σxx/∂y, ∂Σxy/∂y, ∂Σyy/∂y)`, px²/px.
    pub d_sigma_dy: [f32; 3],
}

impl BlurGradient {
    /// Construct from explicit `(∂Σ/∂x, ∂Σ/∂y)` triples (px²/px).
    pub fn new(d_sigma_dx: [f32; 3], d_sigma_dy: [f32; 3]) -> Self {
        Self {
            d_sigma_dx,
            d_sigma_dy,
        }
    }

    /// Signed scatter shift coefficient `C_shift(n)` for an edge with outward
    /// unit normal `n` (px). The apparent constant-`Σ` edge is shifted by
    /// `−C_shift(n)` along `n`, independent of edge polarity.
    pub fn scatter_edge_shift(&self, normal: [f64; 2]) -> f64 {
        let len = normal[0].hypot(normal[1]);
        if !(len.is_finite() && len > 1e-12) {
            return 0.0;
        }
        let [nx, ny] = [normal[0] / len, normal[1] / len];
        let g1 = self.d_sigma_dx.map(f64::from);
        let g2 = self.d_sigma_dy.map(f64::from);
        let div_x = g1[0] + g2[1];
        let div_y = g1[1] + g2[2];
        let q1 = nx * nx * g1[0] + 2.0 * nx * ny * g1[1] + ny * ny * g1[2];
        let q2 = nx * nx * g2[0] + 2.0 * nx * ny * g2[1] + ny * ny * g2[2];
        nx * div_x + ny * div_y - 0.5 * (nx * q1 + ny * q2)
    }

    /// 2-D scatter-PSF corner bias `Δx_bias = measured − true` (px) at an
    /// X-junction whose local board edge lines have angles `axis0_angle` and
    /// `axis1_angle` (radians). Subtract `Δx_bias` from the constant-`Σ`
    /// fitted corner position to recover the unbiased junction.
    ///
    /// Returns `None` if the two axes are nearly collinear.
    pub fn scatter_corner_bias(&self, axis0_angle: f64, axis1_angle: f64) -> Option<[f64; 2]> {
        let n0 = [-axis0_angle.sin(), axis0_angle.cos()];
        let n1 = [-axis1_angle.sin(), axis1_angle.cos()];
        let m = Matrix2::new(n0[0], n0[1], n1[0], n1[1]);
        let det = m.determinant();
        if !(det.is_finite() && det.abs() > 1e-6) {
            return None;
        }
        let rhs = Vector2::new(-self.scatter_edge_shift(n0), -self.scatter_edge_shift(n1));
        let bias = m.try_inverse()? * rhs;
        (bias[0].is_finite() && bias[1].is_finite()).then_some([bias[0], bias[1]])
    }
}

/// Smooth 2-D polynomial surface `(Σxx(x, y), Σxy(x, y), Σyy(x, y))` fitted
/// across a board's converged blur measurements via Huber IRLS.
///
/// `#[non_exhaustive]`: construct with [`BlurField::fit`].
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BlurField {
    /// Normalisation center `(cx, cy)`, px.
    pub center: [f64; 2],
    /// Normalisation half-span `scale`, px.
    pub scale: f64,
    /// Fitted polynomial degree (`0`, `1`, or `2`).
    pub degree: u8,
    /// Polynomial coefficients per `(xx, xy, yy)` over `[1, u, v, u², u·v, v²]`.
    pub coeffs: [[f64; 6]; 3],
    /// Number of corner blur estimates used to fit the field.
    pub sample_count: usize,
}

impl BlurField {
    /// Fit a smooth 2-D blur field to converged `(position, CornerBlur)` pairs.
    ///
    /// Automatically chooses degree 2 for `>= 12` samples, degree 1 for
    /// `>= 5` samples, and constant degree 0 below that, falling back to lower
    /// degree if the design matrix is ill-conditioned (e.g. collinear corners).
    /// Uses inverse-variance weighting plus 4 iterations of Huber IRLS so an
    /// isolated outlier corner cannot corrupt spatial blur derivatives.
    pub fn fit(samples: &[(Point2<f32>, &CornerBlur)]) -> Option<Self> {
        let valid: Vec<(Point2<f32>, &CornerBlur)> = samples
            .iter()
            .copied()
            .filter(|(p, b)| {
                p.x.is_finite()
                    && p.y.is_finite()
                    && b.kernel.xx.is_finite()
                    && b.kernel.xy.is_finite()
                    && b.kernel.yy.is_finite()
            })
            .collect();
        let n = valid.len();
        if n == 0 {
            return None;
        }
        let mut cx = 0.0;
        let mut cy = 0.0;
        for (p, _) in &valid {
            cx += f64::from(p.x);
            cy += f64::from(p.y);
        }
        cx /= n as f64;
        cy /= n as f64;
        let max_r = valid
            .iter()
            .map(|(p, _)| (f64::from(p.x) - cx).hypot(f64::from(p.y) - cy))
            .fold(0.0_f64, f64::max);
        let scale = max_r.max(1.0);

        let uv: Vec<[f64; 2]> = valid
            .iter()
            .map(|(p, _)| [(f64::from(p.x) - cx) / scale, (f64::from(p.y) - cy) / scale])
            .collect();

        let max_deg = if n >= 12 {
            2u8
        } else if n >= 5 {
            1u8
        } else {
            0u8
        };

        for deg in (0..=max_deg).rev() {
            let p_cols = basis_len(deg);
            let mut coeffs = [[0.0_f64; 6]; 3];
            let mut ok = true;
            for (comp_idx, coeff_row) in coeffs.iter_mut().enumerate() {
                let Some(solved) = fit_component_irls(&uv, &valid, comp_idx, p_cols) else {
                    ok = false;
                    break;
                };
                coeff_row[..p_cols].copy_from_slice(&solved);
            }
            if ok {
                return Some(Self {
                    center: [cx, cy],
                    scale,
                    degree: deg,
                    coeffs,
                    sample_count: n,
                });
            }
        }
        None
    }

    /// Evaluate the smoothed blur covariance `Σ(p)` (px²), projected onto the
    /// positive semi-definite cone.
    pub fn eval(&self, p: Point2<f32>) -> BlurKernel {
        let u = (f64::from(p.x) - self.center[0]) / self.scale;
        let v = (f64::from(p.y) - self.center[1]) / self.scale;
        let b = basis_vec(u, v, self.degree);
        let eval_c =
            |row: &[f64; 6]| -> f64 { row.iter().zip(b.iter()).map(|(c, bi)| c * bi).sum::<f64>() };
        let mut s = Sym2x2::new(
            eval_c(&self.coeffs[0]),
            eval_c(&self.coeffs[1]),
            eval_c(&self.coeffs[2]),
        );
        // Project eigenvalues to >= 0 when extrapolating near borders.
        let (lo, _) = s.eigenvalues();
        if lo < 0.0 {
            s.xx -= lo;
            s.yy -= lo;
        }
        BlurKernel::new(s.xx as f32, s.xy as f32, s.yy as f32)
    }

    /// Spatial blur gradient `(∂Σ/∂x, ∂Σ/∂y)` at image point `p` (px²/px).
    pub fn gradient_at(&self, p: Point2<f32>) -> BlurGradient {
        let u = (f64::from(p.x) - self.center[0]) / self.scale;
        let v = (f64::from(p.y) - self.center[1]) / self.scale;
        let inv_s = 1.0 / self.scale;
        let db_du = [0.0, 1.0, 0.0, 2.0 * u, v, 0.0];
        let db_dv = [0.0, 0.0, 1.0, 0.0, u, 2.0 * v];
        let p_cols = basis_len(self.degree);
        let mut gx = [0.0f32; 3];
        let mut gy = [0.0f32; 3];
        for c in 0..3 {
            let row = &self.coeffs[c];
            let dx: f64 = row[..p_cols]
                .iter()
                .zip(&db_du[..p_cols])
                .map(|(a, b)| a * b)
                .sum();
            let dy: f64 = row[..p_cols]
                .iter()
                .zip(&db_dv[..p_cols])
                .map(|(a, b)| a * b)
                .sum();
            gx[c] = (dx * inv_s) as f32;
            gy[c] = (dy * inv_s) as f32;
        }
        BlurGradient::new(gx, gy)
    }
}

fn basis_len(deg: u8) -> usize {
    match deg {
        0 => 1,
        1 => 3,
        _ => 6,
    }
}

fn basis_vec(u: f64, v: f64, deg: u8) -> [f64; 6] {
    match deg {
        0 => [1.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        1 => [1.0, u, v, 0.0, 0.0, 0.0],
        _ => [1.0, u, v, u * u, u * v, v * v],
    }
}

fn component_value_and_weight(blur: &CornerBlur, comp: usize) -> (f64, f64) {
    let val = match comp {
        0 => f64::from(blur.kernel.xx),
        1 => f64::from(blur.kernel.xy),
        _ => f64::from(blur.kernel.yy),
    };
    let var = f64::from(blur.kernel_covariance[comp][comp]).max(1e-6);
    (val, 1.0 / var)
}

fn fit_component_irls(
    uv: &[[f64; 2]],
    samples: &[(Point2<f32>, &CornerBlur)],
    comp: usize,
    p_cols: usize,
) -> Option<Vec<f64>> {
    let n = uv.len();
    let deg = match p_cols {
        1 => 0,
        3 => 1,
        _ => 2,
    };
    let mut y = Vec::with_capacity(n);
    let mut base_w = Vec::with_capacity(n);
    for (_, blur) in samples {
        let (v, w) = component_value_and_weight(blur, comp);
        y.push(v);
        base_w.push(w);
    }
    let mut robust_w = vec![1.0; n];
    let mut beta = DVector::zeros(p_cols);

    for iter in 0..6 {
        let mut ata = DMatrix::<f64>::zeros(p_cols, p_cols);
        let mut aty = DVector::<f64>::zeros(p_cols);
        for i in 0..n {
            let b = basis_vec(uv[i][0], uv[i][1], deg);
            let w = base_w[i] * robust_w[i];
            for r in 0..p_cols {
                aty[r] += w * b[r] * y[i];
                for c in r..p_cols {
                    ata[(r, c)] += w * b[r] * b[c];
                }
            }
        }
        for r in 0..p_cols {
            for c in 0..r {
                ata[(r, c)] = ata[(c, r)];
            }
        }
        let chol = ata.clone().cholesky()?;
        beta = chol.solve(&aty);
        if iter == 5 {
            break;
        }
        let mut abs_res = Vec::with_capacity(n);
        let mut res = Vec::with_capacity(n);
        for i in 0..n {
            let b = basis_vec(uv[i][0], uv[i][1], deg);
            let pred: f64 = (0..p_cols).map(|k| beta[k] * b[k]).sum();
            let ri = (y[i] - pred) * base_w[i].sqrt();
            res.push(ri);
            abs_res.push(ri.abs());
        }
        abs_res.sort_by(f64::total_cmp);
        let mad = (abs_res[n / 2] * 1.4826).max(1e-3);
        if iter < 2 {
            // Initial L1-like Huber warm-up.
            let delta = 1.5 * mad;
            for i in 0..n {
                let ar = res[i].abs();
                robust_w[i] = if ar <= delta { 1.0 } else { delta / ar };
            }
        } else {
            // Tukey biweight rejects gross outliers to exact zero weight.
            let c = 4.685 * mad;
            for i in 0..n {
                let u = res[i] / c;
                robust_w[i] = if u.abs() >= 1.0 {
                    0.0
                } else {
                    let t = 1.0 - u * u;
                    t * t
                };
            }
        }
    }
    Some(beta.as_slice().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blur::{fit_corner_photometric, BlurFitQuality, BlurPattern, PhotometricFitOptions};
    use crate::{GrayImageView, Homography};
    use nalgebra::Matrix3;
    use std::f64::consts::PI;

    #[test]
    fn blur_field_recovers_quadratic_surface_despite_outlier() {
        let mut owned = Vec::new();
        for iy in 0..4 {
            for ix in 0..4 {
                let x = 100.0 + ix as f32 * 50.0;
                let y = 80.0 + iy as f32 * 50.0;
                let u = (f64::from(x) - 175.0) / 100.0;
                let v = (f64::from(y) - 155.0) / 100.0;
                let mut xx = 2.0 + 0.6 * u - 0.3 * v + 0.25 * u * u;
                let xy = 0.2 * u + 0.1 * v;
                let yy = 1.8 + 0.4 * v + 0.15 * v * v;
                if ix == 2 && iy == 1 {
                    xx += 12.0; // Gross outlier.
                }
                let cov = [[0.01f32, 0.0, 0.0], [0.0, 0.01, 0.0], [0.0, 0.0, 0.01]];
                let cb = CornerBlur::new(
                    BlurKernel::new(xx as f32, xy as f32, yy as f32),
                    cov,
                    BlurFitQuality::new(1.0, 100, 180.0, 1.0, 0.0),
                );
                owned.push((Point2::new(x, y), cb));
            }
        }
        let refs: Vec<(Point2<f32>, &CornerBlur)> = owned.iter().map(|(p, b)| (*p, b)).collect();
        let field = BlurField::fit(&refs).expect("field fits");
        assert_eq!(field.degree, 2);

        let query = Point2::new(175.0, 155.0);
        let k = field.eval(query);
        assert!((k.xx - 2.0).abs() < 0.02, "xx={}", k.xx);
        assert!((k.yy - 1.8).abs() < 0.02, "yy={}", k.yy);
        let grad = field.gradient_at(query);
        assert!((grad.d_sigma_dx[0] - 0.006).abs() < 5e-4, "grad={grad:?}");
        assert!((grad.d_sigma_dy[0] + 0.003).abs() < 5e-4, "grad={grad:?}");
    }

    fn chessboard_pattern() -> BlurPattern {
        let mut p = BlurPattern::new();
        for b in -4..4 {
            for a in -4..4 {
                p = p.with_known_cell(a, b);
                if (a + b) % 2 == 0 {
                    let (x, y) = (f64::from(a), f64::from(b));
                    p = p.with_polygon(
                        vec![
                            Point2::new(x, y),
                            Point2::new(x + 1.0, y),
                            Point2::new(x + 1.0, y + 1.0),
                            Point2::new(x, y + 1.0),
                        ],
                        1.0,
                    );
                }
            }
        }
        p
    }

    fn tile_even_fraction(a: f64, b: f64) -> f64 {
        let len = b - a;
        if len <= 0.0 {
            return 0.0;
        }
        let k0 = a.floor() as i32;
        let k1 = b.floor() as i32;
        let mut sum = 0.0;
        for k in k0..=k1 {
            if k.rem_euclid(2) == 0 {
                let lo = a.max(f64::from(k));
                let hi = b.min(f64::from(k) + 1.0);
                if hi > lo {
                    sum += hi - lo;
                }
            }
        }
        sum / len
    }

    /// Forward numerical scatter convolution: deposits exact-area subtiles of
    /// the ideal chessboard through N(x; x', Σ(x')) to verify the exact
    /// 2-D scatter-PSF corner bias theorem for isotropic, diagonal anisotropic,
    /// and off-diagonal shear gradients.
    fn render_scatter_xjunction(
        size: usize,
        corner: [f64; 2],
        cell_px: f64,
        sigma0: [f64; 3],
        grad: BlurGradient,
    ) -> Vec<u8> {
        // Source tile step 0.25 px (4x4 per pixel), target box averaged via 2x2
        // Gauss-Legendre points ±1/(2√3) (exact 4th-order pixel box moment).
        const SUB: usize = 4;
        const GL_OFF: f64 = 0.288_675_134_594_812_9;
        let reach = 16isize;
        let mut accum = vec![0.0f64; size * size];
        let pad = reach + 4;
        let step = 1.0 / SUB as f64;
        let sub_area = step * step;

        for sy in -pad..(size as isize + pad) {
            for sx in -pad..(size as isize + pad) {
                for py in 0..SUB {
                    let y0 = sy as f64 - 0.5 + py as f64 * step;
                    let y1 = y0 + step;
                    let y_src = 0.5 * (y0 + y1);
                    let fy =
                        tile_even_fraction((y0 - corner[1]) / cell_px, (y1 - corner[1]) / cell_px);
                    for px in 0..SUB {
                        let x0 = sx as f64 - 0.5 + px as f64 * step;
                        let x1 = x0 + step;
                        let x_src = 0.5 * (x0 + x1);
                        let fx = tile_even_fraction(
                            (x0 - corner[0]) / cell_px,
                            (x1 - corner[0]) / cell_px,
                        );
                        let black = fx * fy + (1.0 - fx) * (1.0 - fy);
                        if black <= 0.0 {
                            continue;
                        }
                        let dx_c = x_src - corner[0];
                        let dy_c = y_src - corner[1];
                        let sxx = (sigma0[0]
                            + f64::from(grad.d_sigma_dx[0]) * dx_c
                            + f64::from(grad.d_sigma_dy[0]) * dy_c)
                            .max(0.25);
                        let sxy = sigma0[1]
                            + f64::from(grad.d_sigma_dx[1]) * dx_c
                            + f64::from(grad.d_sigma_dy[1]) * dy_c;
                        let syy = (sigma0[2]
                            + f64::from(grad.d_sigma_dx[2]) * dx_c
                            + f64::from(grad.d_sigma_dy[2]) * dy_c)
                            .max(0.25);
                        let det = sxx * syy - sxy * sxy;
                        let inv_det = 1.0 / det;
                        let ixx = syy * inv_det;
                        let ixy = -sxy * inv_det;
                        let iyy = sxx * inv_det;
                        let norm = (0.25 * sub_area) * black / (2.0 * PI * det.sqrt());

                        let ix0 = (x_src.floor() as isize - reach).max(0);
                        let ix1 = (x_src.ceil() as isize + reach).min(size as isize - 1);
                        let iy0 = (y_src.floor() as isize - reach).max(0);
                        let iy1 = (y_src.ceil() as isize + reach).min(size as isize - 1);
                        for iy in iy0..=iy1 {
                            for oy in [-GL_OFF, GL_OFF] {
                                let dy = (iy as f64 + oy) - y_src;
                                for ix in ix0..=ix1 {
                                    let idx = iy as usize * size + ix as usize;
                                    for ox in [-GL_OFF, GL_OFF] {
                                        let dx = (ix as f64 + ox) - x_src;
                                        let q = ixx * dx * dx + 2.0 * ixy * dx * dy + iyy * dy * dy;
                                        if q < 40.0 {
                                            accum[idx] += norm * (-0.5 * q).exp();
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        accum
            .iter()
            .map(|&black| (220.0 - 180.0 * black).round().clamp(0.0, 255.0) as u8)
            .collect()
    }

    #[test]
    fn scatter_corner_bias_theorem_verified_against_2d_scatter_convolution() {
        use crate::blur::frame::{pixel_bounds, select_pixels, Frame};
        use crate::blur::model::{
            BoardPolygon, BoxModel, Model, Vector, Want, I_A, I_ALPHA, I_BETA, I_BLEED, I_C,
            I_GAMMA, I_H, NP,
        };
        use crate::blur::radiometric::RadiometricModel;
        use crate::blur::solve::{levenberg_marquardt, Reparam};

        const SIZE: usize = 96;
        const CORNER: [f64; 2] = [48.25, 47.75];
        const CELL_PX: f64 = 16.0;
        let pattern = chessboard_pattern();
        let polys: Vec<BoardPolygon> = pattern
            .polygons
            .iter()
            .map(|(v, w)| BoardPolygon {
                vertices: v.clone(),
                weight: *w,
            })
            .collect();
        let h_seed = Homography::new(Matrix3::new(
            CELL_PX,
            0.0,
            CORNER[0] + 0.12,
            0.0,
            CELL_PX,
            CORNER[1] - 0.12,
            0.0,
            0.0,
            1.0,
        ));

        let cases = [
            (
                "isotropic gradient",
                [4.0, 0.0, 4.0],
                BlurGradient::new([0.05, 0.0, 0.05], [0.0, 0.0, 0.0]),
            ),
            (
                "diagonal anisotropic gradient",
                [4.2, 0.0, 3.8],
                BlurGradient::new([0.06, 0.0, 0.0], [0.0, 0.0, 0.05]),
            ),
            (
                "off-diagonal shear gradient",
                [4.2, 0.0, 4.2],
                BlurGradient::new([0.0, 0.0, 0.0], [0.0, 0.05, 0.0]),
            ),
        ];

        for (name, sigma0, grad) in cases {
            let pixels = render_scatter_xjunction(SIZE, CORNER, CELL_PX, sigma0, grad);
            let img = GrayImageView {
                width: SIZE,
                height: SIZE,
                data: &pixels,
            };

            // 1) With illumination uniform (α=β=0 fixed), constant-Σ LM reproduces the
            // exact analytic scatter_corner_bias theorem to < 0.005 px.
            let frame = Frame::new(&h_seed).expect("frame");
            let px = select_pixels(&img, &pattern, &frame, 0.75, 8.0);
            let mut fixed = [false; NP];
            fixed[I_ALPHA] = true;
            fixed[I_BETA] = true;
            fixed[I_GAMMA] = true;
            fixed[I_BLEED] = true;
            let model_const = Model {
                polygons: &polys,
                c0: frame.c0,
                scale: frame.scale,
                box_model: BoxModel::Gaussian,
                fixed,
                pixel_bounds: pixel_bounds(&px),
                radiometric: RadiometricModel::Linear,
                fit_ink_bleed: false,
                blur_gradient: None,
            };
            let mut th0 = Vector::zeros();
            for (k, v) in frame.h.iter().enumerate() {
                th0[I_H + k] = *v;
            }
            th0[0] = sigma0[0];
            th0[1] = sigma0[1];
            th0[2] = sigma0[2];
            th0[I_A] = 180.0;
            th0[I_C] = 40.0;
            let e0 = model_const
                .evaluate(
                    &th0,
                    &px,
                    Want {
                        jacobian: true,
                        ..Default::default()
                    },
                )
                .expect("eval0");
            let re = Reparam::new(&model_const, &th0, &e0.jtj, 0.75);
            let (th_const, _) =
                levenberg_marquardt(&model_const, th0, &px, e0, &re, true).expect("lm");
            let uncorr_x = frame.c0[0] + frame.scale * th_const[I_H + 2] - CORNER[0];
            let uncorr_y = frame.c0[1] + frame.scale * th_const[I_H + 5] - CORNER[1];
            let predicted_bias = grad
                .scatter_corner_bias(0.0, PI / 2.0)
                .expect("orthogonal axes");
            assert!(
                (uncorr_x - predicted_bias[0]).abs() < 0.005
                    && (uncorr_y - predicted_bias[1]).abs() < 0.005,
                "{name}: uniform-illum constant-Σ shift ({uncorr_x:.4}, {uncorr_y:.4}) vs theory ({:.4}, {:.4})",
                predicted_bias[0],
                predicted_bias[1],
            );

            // 2) Full free-(α, β, Σ, H) fit WITHOUT vs WITH exact 2-D scatter expansion.
            let raw =
                fit_corner_photometric(&img, &pattern, &h_seed, &PhotometricFitOptions::default())
                    .unwrap_or_else(|e| panic!("{name}: raw fit failed: {e:?}"));
            let opts = PhotometricFitOptions::default().with_known_blur_gradient(grad);
            let corr = fit_corner_photometric(&img, &pattern, &h_seed, &opts)
                .unwrap_or_else(|e| panic!("{name}: corr fit failed: {e:?}"));
            let corr_err = (f64::from(corr.position.x) - CORNER[0])
                .hypot(f64::from(corr.position.y) - CORNER[1]);
            assert!(
                corr_err < 0.005
                    && corr.fit.residual_rms < 0.35
                    && corr.fit.residual_rms < raw.fit.residual_rms,
                "{name}: corr_err={corr_err:.5} px, corr_rms={:.3}, raw_rms={:.3}",
                corr.fit.residual_rms,
                raw.fit.residual_rms
            );
        }
    }
}
