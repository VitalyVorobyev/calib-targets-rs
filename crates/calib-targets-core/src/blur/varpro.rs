//! Schur-complement Variable Projection (VarPro) for linear photometric
//! amplitude & black level `(A, C)`, with Kaufman's (1975) approximation of
//! the Golub–Pereyra (1973) reduced Jacobian.
//!
//! When the radiometric response is linear (`I(p) = S(p) · (C + A · (1 - B(p)))`),
//! `(A, C)` enters strictly linearly once the nonlinear parameters `η`
//! (blur `Σ`, geometry `H`, illumination gradient `α, β`, bleed `κ`) are
//! fixed. Projecting `(A, C)` onto their closed-form 2×2 normal-equation
//! optimum on every trial step and reducing the Gauss-Newton normal matrix by
//! its exact Schur complement eliminates ill-conditioned `(σ, A)` and `(A, C)`
//! valleys without altering the objective minimum.

use nalgebra::Matrix2;

use super::model::{Matrix, Model, Pixel, Vector, I_A, I_C, NP};

/// Condition floor below which the 2×2 `(A, C)` Gram matrix is treated as
/// singular (e.g. an almost uniform patch).
const AC_COND_TOL: f64 = 1e-9;

/// Project `(theta[I_A], theta[I_C])` onto their closed-form least-squares
/// optimum holding all nonlinear parameters fixed.
///
/// Returns `false` (leaving `theta` unchanged) if `Σ_c` is outside the model's
/// positive-definite domain or the `(A, C)` design matrix is nearly singular.
pub(super) fn project_ac(model: &Model<'_>, theta: &mut Vector, pixels: &[Pixel]) -> bool {
    let Some(bs) = model.blackness_with_bleed(theta, pixels) else {
        return false;
    };
    let (alpha, beta) = (theta[super::model::I_ALPHA], theta[super::model::I_BETA]);
    let (mut g00, mut g01, mut g11, mut r0, mut r1) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for (px, b) in pixels.iter().zip(&bs) {
        let dx = (f64::from(px.x) - model.c0[0]) / model.scale;
        let dy = (f64::from(px.y) - model.c0[1]) / model.scale;
        let s = 1.0 + alpha * dx + beta * dy;
        let p0 = s * (1.0 - b);
        let p1 = s;
        g00 += p0 * p0;
        g01 += p0 * p1;
        g11 += p1 * p1;
        r0 += p0 * px.value;
        r1 += p1 * px.value;
    }
    let det = g00 * g11 - g01 * g01;
    if !(det.is_finite() && g00 > 0.0 && g11 > 0.0 && det > AC_COND_TOL * g00 * g11) {
        return false;
    }
    let inv_det = 1.0 / det;
    let a = (g11 * r0 - g01 * r1) * inv_det;
    let c = (g00 * r1 - g01 * r0) * inv_det;
    if a.is_finite() && c.is_finite() {
        theta[I_A] = a;
        theta[I_C] = c;
        true
    } else {
        false
    }
}

/// Form the exact Schur complement eliminating linear parameters `{I_A, I_C}`
/// from the `φ`-space normal matrix `n` and gradient `g`.
///
/// `Reparam::new` always maps `I_A` and `I_C` 1-to-1 to themselves, so the
/// block in `n` IS `ΦᵀΦ`. Returns `None` if `{I_A, I_C}` is singular or
/// inactive, letting the caller fall back to a joint LM step.
pub(super) fn schur_reduce_ac(
    n: &Matrix,
    g: &Vector,
    active: &[bool; NP],
) -> Option<(Matrix, Vector)> {
    if !active[I_A] || !active[I_C] {
        return None;
    }
    let g_mat = Matrix2::new(n[(I_A, I_A)], n[(I_A, I_C)], n[(I_C, I_A)], n[(I_C, I_C)]);
    let det = g_mat.determinant();
    let diag_prod = g_mat[(0, 0)] * g_mat[(1, 1)];
    if !(det.is_finite() && g_mat[(0, 0)] > 0.0 && diag_prod > 0.0 && det > 1e-12 * diag_prod) {
        return None;
    }
    let inv_g = Matrix2::new(g_mat[(1, 1)], -g_mat[(0, 1)], -g_mat[(1, 0)], g_mat[(0, 0)]) / det;

    let mut n_red = *n;
    let mut g_red = *g;
    let g_lin = nalgebra::Vector2::new(g[I_A], g[I_C]);
    let w_lin = inv_g * g_lin;

    for i in 0..NP {
        if i == I_A || i == I_C || !active[i] {
            continue;
        }
        let b_i = nalgebra::RowVector2::new(n[(i, I_A)], n[(i, I_C)]);
        g_red[i] -= (b_i * w_lin)[0];
        let bw = b_i * inv_g;
        for j in i..NP {
            if j == I_A || j == I_C || !active[j] {
                continue;
            }
            let b_j = nalgebra::Vector2::new(n[(I_A, j)], n[(I_C, j)]);
            let val = n_red[(i, j)] - (bw * b_j)[0];
            n_red[(i, j)] = val;
            n_red[(j, i)] = val;
        }
    }

    for lin in [I_A, I_C] {
        for k in 0..NP {
            n_red[(lin, k)] = 0.0;
            n_red[(k, lin)] = 0.0;
        }
        n_red[(lin, lin)] = 1.0;
        g_red[lin] = 0.0;
    }
    Some((n_red, g_red))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blur::model::{BoardPolygon, BoxModel, Want, I_H, I_SIGMA};
    use crate::blur::radiometric::RadiometricModel;

    fn square(x0: f64, y0: f64, w: f64) -> BoardPolygon {
        BoardPolygon {
            vertices: vec![
                [x0, y0],
                [x0 + 1.0, y0],
                [x0 + 1.0, y0 + 1.0],
                [x0, y0 + 1.0],
            ],
            weight: w,
        }
    }

    #[test]
    fn varpro_projection_and_schur_step_match_joint_system() {
        let polys = vec![square(-1.0, -1.0, 1.0), square(0.0, 0.0, 1.0)];
        let model = Model {
            polygons: &polys,
            c0: [16.0, 16.0],
            scale: 12.0,
            box_model: BoxModel::Gaussian,
            fixed: [false; NP],
            pixel_bounds: [10.0, 10.0, 22.0, 22.0],
            radiometric: RadiometricModel::Linear,
            fit_ink_bleed: false,
            blur_gradient: None,
        };
        let mut pixels = Vec::new();
        for y in 10..=22 {
            for x in 10..=22 {
                pixels.push(Pixel {
                    x,
                    y,
                    value: 50.0 + f64::from((x * 5 + y * 9) % 130),
                });
            }
        }
        let mut th = Vector::zeros();
        th[I_SIGMA] = 1.6;
        th[I_SIGMA + 2] = 1.6;
        th[I_A] = 10.0; // Deliberately far from optimum.
        th[I_C] = 200.0;
        th[I_H] = 1.0;
        th[I_H + 4] = 1.0;
        th[15] = 1.0; // I_GAMMA

        assert!(project_ac(&model, &mut th, &pixels));
        let eval = model
            .evaluate(
                &th,
                &pixels,
                Want {
                    jacobian: true,
                    ..Want::default()
                },
            )
            .expect("eval");
        assert!(eval.jtr[I_A].abs() < 1e-9);
        assert!(eval.jtr[I_C].abs() < 1e-9);

        // Compare Schur-reduced nonlinear step with full joint solve of JᵀJ δ = Jᵀr
        // (on the nonsingular active subset: sigma + photometry + h2, h5).
        let mut active = [false; NP];
        for idx in [0, 1, 2, I_A, I_C, 5, 6, I_H + 2, I_H + 5] {
            active[idx] = true;
        }
        let mut n = eval.jtj;
        let mut g = eval.jtr;
        for k in 0..NP {
            if !active[k] {
                for m in 0..NP {
                    n[(k, m)] = 0.0;
                    n[(m, k)] = 0.0;
                }
                n[(k, k)] = 1.0;
                g[k] = 0.0;
            }
        }
        let joint = n.cholesky().expect("SPD").solve(&g);
        let (n_red, g_red) = schur_reduce_ac(&n, &g, &active).expect("schur");
        let schur = n_red.cholesky().expect("SPD").solve(&g_red);
        assert!((schur[I_A]).abs() < 1e-15);
        assert!((schur[I_C]).abs() < 1e-15);
        for k in [0, 1, 2, 5, 6, I_H + 2, I_H + 5] {
            assert!(
                (joint[k] - schur[k]).abs() < 1e-9 * (1.0 + joint[k].abs()),
                "idx {k}: {} vs {}",
                joint[k],
                schur[k]
            );
        }
    }
}
