//! The solver: the fit's parameterisation over the observable directions,
//! Levenberg–Marquardt, the photometric variance inflation, and the
//! heteroskedasticity-and-autocorrelation-consistent covariance.

use super::model::{
    Eval, Matrix, Model, Pixel, Vector, Want, I_A, I_ALPHA, I_BETA, I_C, I_GAMMA, I_H, I_SIGMA, NP,
};
use super::varpro::{project_ac, schur_reduce_ac};
use super::BlurDecline;

/// Levenberg–Marquardt iteration cap per window.
const MAX_ITER: usize = 60;

/// Convergence: the last accepted step lowered the cost by less than this
/// fraction of the residual variance `s²`. A cost decrease `Δ` from a move of
/// `k` standard errors is about `k² s²`, so this stops `k ≈ 0.01` short.
const STOP_FRACTION: f64 = 1e-4;

/// The fit's parameterisation: `θ = θ₀ + T φ`.
///
/// Blur and photometry map one to one. The homography moves only along the
/// directions the window constrains: its 8 × 8 block of the normal matrix is
/// expressed in pixels of displacement at the window's corners and
/// eigen-decomposed, and a direction informed less than [`GEOMETRY_RANK_TOL`]
/// times the best-informed one keeps its value from the labelled neighbours.
/// On a plain chessboard at half a cell, for instance, the patch shows only
/// the two lines through the corner: the scales along them and the
/// perspective do not move the model, carry no information about the blur,
/// and would otherwise drift without bound.
pub(super) struct Reparam {
    pub(super) t: Matrix,
    /// Coordinates of `φ` that are free.
    active: [bool; NP],
}

/// Relative information below which a homography direction is held: its
/// standard error from the patch would be a thousand times that of the
/// best-determined direction (the corner position). The blur is insensitive
/// to this tolerance over orders of magnitude (such directions barely move
/// the model); it exists to keep the solve well-posed.
const GEOMETRY_RANK_TOL: f64 = 1e-6;

impl Reparam {
    pub(super) fn new(model: &Model<'_>, theta: &Vector, jtj: &Matrix, rho: f64) -> Self {
        let mut t = Matrix::zeros();
        let mut active = [false; NP];
        for k in (0..I_H).chain((I_H + 8)..NP) {
            if !model.fixed[k] {
                t[(k, k)] = 1.0;
                active[k] = true;
            }
        }
        // Pixel displacement per unit of each homography entry, RMS over the
        // window's corners.
        let hom = super::model::LocalHomography {
            h: &theta.as_slice()[I_H..I_H + 8],
        };
        let mut c = [0.0f64; 8];
        for q in [[-rho, -rho], [rho, -rho], [rho, rho], [-rho, rho]] {
            let (_, d) = hom.apply(q);
            for k in 0..8 {
                c[k] += 0.25 * model.scale * model.scale * (d[0][k].powi(2) + d[1][k].powi(2));
            }
        }
        let c = c.map(|v| v.sqrt().max(1e-12));
        let mut g = nalgebra::SMatrix::<f64, 8, 8>::zeros();
        for r in 0..8 {
            for k in 0..8 {
                if !model.fixed[I_H + r] && !model.fixed[I_H + k] {
                    g[(r, k)] = jtj[(I_H + r, I_H + k)] / (c[r] * c[k]);
                }
            }
        }
        let eig = g.symmetric_eigen();
        let top = eig.eigenvalues.max();
        for j in 0..8 {
            if top > 0.0 && eig.eigenvalues[j] > GEOMETRY_RANK_TOL * top {
                for k in 0..8 {
                    t[(I_H + k, I_H + j)] = eig.eigenvectors[(k, j)] / c[k];
                }
                active[I_H + j] = true;
            }
        }
        Self { t, active }
    }

    /// Normal matrix and gradient in `φ`, inactive coordinates inert.
    pub(super) fn project(&self, jtj: &Matrix, jtr: &Vector) -> (Matrix, Vector) {
        let mut a = self.t.transpose() * jtj * self.t;
        let mut g = self.t.transpose() * jtr;
        for k in 0..NP {
            if !self.active[k] {
                for m in 0..NP {
                    a[(k, m)] = 0.0;
                    a[(m, k)] = 0.0;
                }
                a[(k, k)] = 1.0;
                g[k] = 0.0;
            }
        }
        (a, g)
    }

    /// `T (Tᵀ JᵀJ T)⁻¹ Tᵀ` over the coordinates in `keep` (a subset of the
    /// active ones; the others treated as known): the generalised inverse
    /// of the normal matrix in `θ`.
    pub(super) fn inverse(&self, jtj: &Matrix, keep: &[bool; NP]) -> Option<Matrix> {
        let (a, _) = self.project(jtj, &Vector::zeros());
        let idx: Vec<usize> = (0..NP).filter(|&k| self.active[k] && keep[k]).collect();
        let n = idx.len();
        let mut sub = nalgebra::DMatrix::<f64>::zeros(n, n);
        for (r, &i) in idx.iter().enumerate() {
            for (c, &j) in idx.iter().enumerate() {
                sub[(r, c)] = a[(i, j)];
            }
        }
        let inv = sub.cholesky()?.inverse();
        let mut full = Matrix::zeros();
        for (r, &i) in idx.iter().enumerate() {
            for (c, &j) in idx.iter().enumerate() {
                full[(i, j)] = inv[(r, c)];
            }
        }
        Some(self.t * full * self.t.transpose())
    }

    pub(super) fn free_count(&self) -> usize {
        self.active.iter().filter(|a| **a).count()
    }
}

/// Levenberg–Marquardt from `theta` in the parameterisation `re`. Returns
/// the optimum and the evaluation (with Jacobian) there. When `use_varpro` is
/// `true` and contrast/black level `(A, C)` enter linearly, eliminates `(A, C)`
/// via Variable Projection (closed-form projection + Schur complement, i.e.
/// Kaufman's approximation of the Golub–Pereyra Jacobian).
pub(super) fn levenberg_marquardt(
    model: &Model<'_>,
    theta: Vector,
    pixels: &[Pixel],
    first: Eval,
    re: &Reparam,
    use_varpro: bool,
) -> Result<(Vector, Eval), BlurDecline> {
    let want = Want {
        jacobian: true,
        ..Want::default()
    };
    let varpro = use_varpro
        && model.radiometric.is_linear()
        && !model.fixed[I_A]
        && !model.fixed[I_C]
        && re.active[I_A]
        && re.active[I_C];
    let mut theta = theta;
    let mut cur = first;
    let mut lambda = 1e-3;
    for _ in 0..MAX_ITER {
        let (n_raw, g_raw) = re.project(&cur.jtj, &cur.jtr);
        let (n, g) = if varpro {
            schur_reduce_ac(&n_raw, &g_raw, &re.active).unwrap_or((n_raw, g_raw))
        } else {
            (n_raw, g_raw)
        };
        let mut a = n;
        for i in 0..NP {
            a[(i, i)] += lambda * n[(i, i)].max(1e-12);
        }
        let Some(ch) = a.cholesky() else {
            lambda *= 10.0;
            if lambda > 1e10 {
                return Ok((theta, cur));
            }
            continue;
        };
        let step = ch.solve(&g);
        // The linearised model's decrease, `2 gᵀδ − δᵀ N δ` (cost Σ r²):
        // below the stopping threshold, no step can matter.
        let predicted = 2.0 * g.dot(&step) - (step.transpose() * n * step)[(0, 0)];
        if predicted <= STOP_FRACTION * cur.cost / pixels.len() as f64 {
            return Ok((theta, cur));
        }
        let mut trial = theta + re.t * step;
        if varpro && model.admissible(&trial) {
            let _ = project_ac(model, &mut trial, pixels);
        }
        let next = if model.admissible(&trial) {
            model.evaluate(&trial, pixels, want)
        } else {
            None
        };
        match next {
            Some(e) if e.cost < cur.cost => {
                let decrease = cur.cost - e.cost;
                theta = trial;
                cur = e;
                lambda = (lambda * 0.2).max(1e-12);
                // Stop once a step improves the cost by less than 10⁻⁴ of
                // the residual variance: what is left moves the estimate by
                // about a hundredth of its standard error.
                if decrease <= STOP_FRACTION * cur.cost / pixels.len() as f64 {
                    return Ok((theta, cur));
                }
            }
            _ => {
                lambda *= 10.0;
                if lambda > 1e10 {
                    // No admissible descent left: at the optimum (or at the
                    // domain boundary, which the caller checks).
                    return Ok((theta, cur));
                }
            }
        }
    }
    Err(BlurDecline::NotConverged)
}

/// `∂σ_px/∂θ` for `σ_px = √(tr Σ / 2)`.
pub(super) fn sigma_px_gradient(sigma: [f64; 3]) -> Vector {
    let s = (0.5 * (sigma[0] + sigma[2])).sqrt();
    let mut g = Vector::zeros();
    g[I_SIGMA] = 0.25 / s;
    g[I_SIGMA + 2] = 0.25 / s;
    g
}

/// Variance inflation of `σ_px` by the photometric parameters, from the
/// normal matrix: `Var(σ | photometry free) / Var(σ | photometry known)`.
pub(super) fn photometric_vif(jtj: &Matrix, re: &Reparam, sigma: [f64; 3]) -> Option<f64> {
    let g = sigma_px_gradient(sigma);
    let all = [true; NP];
    let mut cond = all;
    for k in [I_A, I_C, I_ALPHA, I_BETA, I_GAMMA] {
        cond[k] = false;
    }
    let v_full = (g.transpose() * re.inverse(jtj, &all)? * g)[(0, 0)];
    let v_cond = (g.transpose() * re.inverse(jtj, &cond)? * g)[(0, 0)];
    (v_full > 0.0 && v_cond > 0.0).then(|| v_full / v_cond)
}

/// Heteroskedasticity-and-autocorrelation-consistent covariance of `θ`:
/// `B⁻¹ M B⁻¹ · n/(n − k)` with `B = JᵀJ` and the meat `M` the per-pixel
/// scores `u_p = J_p r_p` cross-weighted by a Bartlett product kernel of
/// half-width `lag` px over the pixel grid.
pub(super) fn hac_covariance(
    bread_inv: &Matrix,
    pixels: &[Pixel],
    scores: &[Vector],
    lag: usize,
    free_params: usize,
) -> Matrix {
    let x0 = pixels.iter().map(|p| p.x).min().unwrap_or(0);
    let y0 = pixels.iter().map(|p| p.y).min().unwrap_or(0);
    let x1 = pixels.iter().map(|p| p.x).max().unwrap_or(0);
    let y1 = pixels.iter().map(|p| p.y).max().unwrap_or(0);
    let w = (x1 - x0 + 1) as usize;
    let h = (y1 - y0 + 1) as usize;
    let mut grid = vec![Vector::zeros(); w * h];
    for (p, u) in pixels.iter().zip(scores) {
        grid[(p.y - y0) as usize * w + (p.x - x0) as usize] = *u;
    }
    // Bartlett weights 1 − |d|/(lag + 1), |d| ≤ lag: a separable triangle,
    // applied as a running window sum per axis.
    let tri = |line: &[Vector]| -> Vec<Vector> {
        let n = line.len();
        let mut out = vec![Vector::zeros(); n];
        let norm = 1.0 / (lag + 1) as f64;
        for (i, o) in out.iter_mut().enumerate() {
            let lo = i.saturating_sub(lag);
            let hi = (i + lag).min(n - 1);
            for (j, v) in line.iter().enumerate().take(hi + 1).skip(lo) {
                let d = i.abs_diff(j) as f64;
                *o += v * (1.0 - d * norm);
            }
        }
        out
    };
    let mut rows = vec![Vector::zeros(); w * h];
    for y in 0..h {
        let r = tri(&grid[y * w..(y + 1) * w]);
        rows[y * w..(y + 1) * w].copy_from_slice(&r);
    }
    let mut both = vec![Vector::zeros(); w * h];
    for x in 0..w {
        let col: Vec<Vector> = (0..h).map(|y| rows[y * w + x]).collect();
        for (y, v) in tri(&col).into_iter().enumerate() {
            both[y * w + x] = v;
        }
    }
    let mut meat = Matrix::zeros();
    for (p, u) in pixels.iter().zip(scores) {
        let v = both[(p.y - y0) as usize * w + (p.x - x0) as usize];
        meat += u * v.transpose();
    }
    let n = pixels.len() as f64;
    let dof = (n - free_params as f64).max(1.0);
    bread_inv * meat * bread_inv * (n / dof)
}

/// The Bartlett kernel's half-width for the HAC covariance, px: Andrews'
/// (1991) automatic bandwidth for an AR(1) disturbance,
/// `1.1447 (α T)^{1/3}` with `α = 4ρ² / ((1 − ρ)² (1 + ρ)²)`, from the
/// residuals' own lag-1 autocorrelation `ρ` (negative values, which would
/// shrink the variance, count as 0) and `T = √n` pixels per axis; at least 1,
/// at most half the patch.
///
/// White residuals (an exact model) get a narrow kernel; residuals dominated
/// by smooth misfit — a PSF that is not Gaussian, or a fit the pattern does
/// not support — get a wide one, and a standard error that says so.
pub(super) fn bartlett_bandwidth(autocorrelation: f64, n: usize) -> usize {
    let rho = autocorrelation.clamp(0.0, 0.99);
    let alpha = 4.0 * rho * rho / ((1.0 - rho).powi(2) * (1.0 + rho).powi(2));
    let t = (n as f64).sqrt();
    let lag = 1.1447 * (alpha * t).cbrt();
    lag.ceil().clamp(1.0, (0.5 * t).max(1.0)) as usize
}

/// Lag-1 autocorrelation of the residuals over horizontal and vertical
/// neighbour pairs.
pub(super) fn residual_autocorrelation(pixels: &[Pixel], residuals: &[f64]) -> f64 {
    let x0 = pixels.iter().map(|p| p.x).min().unwrap_or(0);
    let y0 = pixels.iter().map(|p| p.y).min().unwrap_or(0);
    let x1 = pixels.iter().map(|p| p.x).max().unwrap_or(0);
    let y1 = pixels.iter().map(|p| p.y).max().unwrap_or(0);
    let w = (x1 - x0 + 1) as usize;
    let h = (y1 - y0 + 1) as usize;
    let mut grid = vec![f64::NAN; w * h];
    for (p, r) in pixels.iter().zip(residuals) {
        grid[(p.y - y0) as usize * w + (p.x - x0) as usize] = *r;
    }
    let (mut num, mut den_a, mut den_b) = (0.0, 0.0, 0.0);
    for y in 0..h {
        for x in 0..w {
            let a = grid[y * w + x];
            if a.is_nan() {
                continue;
            }
            for (nx, ny) in [(x + 1, y), (x, y + 1)] {
                if nx < w && ny < h {
                    let b = grid[ny * w + nx];
                    if !b.is_nan() {
                        num += a * b;
                        den_a += a * a;
                        den_b += b * b;
                    }
                }
            }
        }
    }
    if den_a > 0.0 && den_b > 0.0 {
        num / (den_a * den_b).sqrt()
    } else {
        0.0
    }
}
