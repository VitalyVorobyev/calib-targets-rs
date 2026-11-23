//! The generative pixel model of a corner patch and its analytic Jacobian.
//!
//! ```text
//! I(p) = S(p) · (C + A · (1 − B(p))),       S(p) = 1 + α d_x + β d_y,
//! B(p) = Σ_k w_k · ⟨mass of H(P_k) under N(p + u, Σ_c)⟩_u
//! ```
//!
//! `P_k` are the printed polygons in board units, `w_k` their signed
//! blackness, `H` the local board → image homography, `d = (p − c₀)/s` the
//! pixel's offset from the corner in cells. `A` is the contrast (its sign
//! free: a plain chessboard's parity is unknown), `C` the black level, `α`,
//! `β` a linear illumination gradient.
//!
//! # The pixel aperture
//!
//! The parameter is the *total* blur covariance `Σ` (px²): lens and the
//! pixel's unit box together, the box contributing `I/12`. The box is not a
//! Gaussian, and near `Σ ≈ I/12` the difference biases a pure-Gaussian fit
//! by about a per cent. It is modelled by a four-point mixture: the box is
//! replaced by Gaussians of covariance `s² I` centred at `u = (±a, ±a)`,
//! with `a⁴ = 1/240` and `a² + s² = 1/12`, which matches the box's second
//! and fourth moments on both axes exactly (its sixth to 10 %). Each
//! component then carries `Σ_c = Σ − a² I`. When the blur is wide the box's
//! excess kurtosis no longer matters and one sample with `Σ_c = Σ` is used
//! ([`BoxModel`]).

use nalgebra::{SMatrix, SVector};

use super::gauss_polygon::{boundary_winding, edge_moments, edge_term, on_segment, CUT};
use super::radiometric::{bleed_eval, RadiometricModel, MAX_BLEED_KAPPA, MAX_GAMMA, MIN_GAMMA};
use super::BlurGradient;

/// Number of model parameters.
pub(super) const NP: usize = 17;
/// `Σxx, Σxy, Σyy` (px²).
pub(super) const I_SIGMA: usize = 0;
/// Contrast `A` (linearised gray levels).
pub(super) const I_A: usize = 3;
/// Black level `C` (linearised gray levels).
pub(super) const I_C: usize = 4;
/// Illumination gradient `α`, `β` (per cell).
pub(super) const I_ALPHA: usize = 5;
pub(super) const I_BETA: usize = 6;
/// The eight free homography entries.
pub(super) const I_H: usize = 7;
/// Encoding gamma `γ` (`FreeGamma` only).
pub(super) const I_GAMMA: usize = 15;
/// Probit ink / microcapsule bleed `κ` (standard deviations).
pub(super) const I_BLEED: usize = 16;

pub(super) type Vector = SVector<f64, NP>;
pub(super) type Matrix = SMatrix<f64, NP, NP>;

/// The mixture's shift `a = 240^(−1/4)` px.
pub(super) const BOX_SHIFT: f64 = 0.254_066_013_276_212_6;
/// `a²`: the part of the box's `1/12` variance carried by the shifts.
pub(super) const BOX_SHIFT_VAR: f64 = BOX_SHIFT * BOX_SHIFT;

/// How the pixel aperture enters the model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BoxModel {
    /// Four-point mixture matching the unit box's moments to fourth order.
    Mixture,
    /// One Gaussian carrying the whole `Σ`.
    Gaussian,
}

impl BoxModel {
    fn samples(self) -> &'static [[f64; 2]] {
        const MIX: [[f64; 2]; 4] = [
            [-BOX_SHIFT, -BOX_SHIFT],
            [BOX_SHIFT, -BOX_SHIFT],
            [-BOX_SHIFT, BOX_SHIFT],
            [BOX_SHIFT, BOX_SHIFT],
        ];
        const ONE: [[f64; 2]; 1] = [[0.0, 0.0]];
        match self {
            Self::Mixture => &MIX,
            Self::Gaussian => &ONE,
        }
    }

    /// The variance the samples' spread carries, removed from `Σ`.
    pub(super) fn shift_var(self) -> f64 {
        match self {
            Self::Mixture => BOX_SHIFT_VAR,
            Self::Gaussian => 0.0,
        }
    }

    /// Smallest eigenvalue `Σ` may take for the components to stay a
    /// Gaussian (with a small floor against a degenerate Cholesky).
    pub(super) fn min_eigenvalue(self) -> f64 {
        self.shift_var() + 1e-4
    }
}

/// One pixel of the patch.
#[derive(Clone, Copy, Debug)]
pub(super) struct Pixel {
    /// Pixel-centre coordinates (integer pixel indices).
    pub x: i32,
    pub y: i32,
    /// Gray value.
    pub value: f64,
}

/// A signed polygon of the printed pattern, in board units (cells, corner at
/// the origin).
#[derive(Clone, Debug)]
pub(super) struct BoardPolygon {
    pub vertices: Vec<[f64; 2]>,
    pub weight: f64,
}

/// The local homography in normalised form: board units → `c₀ + s·H(q)`.
#[derive(Clone, Copy, Debug)]
pub(super) struct LocalHomography<'a> {
    pub h: &'a [f64],
}

impl LocalHomography<'_> {
    /// `H(q)` (in units of `s`, relative to `c₀`) and `∂H/∂h` (2 × 8).
    #[inline]
    pub(super) fn apply(&self, q: [f64; 2]) -> ([f64; 2], [[f64; 8]; 2]) {
        let h = self.h;
        let w = h[6] * q[0] + h[7] * q[1] + 1.0;
        let x = (h[0] * q[0] + h[1] * q[1] + h[2]) / w;
        let y = (h[3] * q[0] + h[4] * q[1] + h[5]) / w;
        let (qx, qy) = (q[0] / w, q[1] / w);
        let iw = 1.0 / w;
        (
            [x, y],
            [
                [qx, qy, iw, 0.0, 0.0, 0.0, -x * qx, -x * qy],
                [0.0, 0.0, 0.0, qx, qy, iw, -y * qx, -y * qy],
            ],
        )
    }
}

/// A polygon mapped to the image and whitened for one evaluation pass.
pub(super) struct PassPolygon {
    /// Whitened vertices `L⁻¹ v`.
    w: Vec<[f64; 2]>,
    edges: Vec<PassEdge>,
    weight: f64,
    /// Orientation sign.
    orient: f64,
    /// Whitened bounding box `[x0, y0, x1, y1]`.
    bbox: [f64; 4],
}

struct PassEdge {
    a: usize,
    b: usize,
    t: [f64; 2],
    len: f64,
    /// `w_a × t` and `w_a · t`.
    ch: f64,
    cy: f64,
    /// `L⁻ᵀ` of the outward normal (`o · n_right`), image space.
    m: [f64; 2],
    /// `(∂v_a/∂h)ᵀ m` and `(∂v_b/∂h)ᵀ m`.
    pa: [f64; 8],
    pb: [f64; 8],
}

/// The fit's view of one corner: pixels, pattern and the fixed frame.
pub(super) struct Model<'a> {
    pub polygons: &'a [BoardPolygon],
    /// Corner image position at the start, px.
    pub c0: [f64; 2],
    /// Local scale, px per cell.
    pub scale: f64,
    pub box_model: BoxModel,
    /// Parameters held fixed (zero Jacobian column, unit diagonal).
    pub fixed: [bool; NP],
    /// Bounding box of the patch's pixel centres `[x0, y0, x1, y1]`: a pass
    /// keeps only the polygons within the kernel's reach of it.
    pub pixel_bounds: [f64; 4],
    /// Camera tone-response curve applied in the forward model.
    pub radiometric: RadiometricModel,
    /// Whether probit ink/microcapsule bleed `κ` (`theta[I_BLEED]`) is enabled.
    pub fit_ink_bleed: bool,
    /// Optional known spatial blur gradient `(∂Σ/∂x, ∂Σ/∂y)` for first-order
    /// 2-D scatter-PSF modeling.
    pub blur_gradient: Option<BlurGradient>,
}

pub(super) struct ScatterPass {
    g1: [f64; 3],
    g2: [f64; 3],
    div: [f64; 2],
    l_rows: [[f64; 2]; 2],
    linv: [f64; 3],
    dls: [[f64; 3]; 3],
}

impl ScatterPass {
    pub(super) fn new(grad: BlurGradient, l: [f64; 3], linv: [f64; 3], dls: [[f64; 3]; 3]) -> Self {
        let g1 = grad.d_sigma_dx.map(f64::from);
        let g2 = grad.d_sigma_dy.map(f64::from);
        Self {
            g1,
            g2,
            div: [g1[0] + g2[1], g1[1] + g2[2]],
            l_rows: [[l[0], 0.0], [l[1], l[2]]],
            linv,
            dls,
        }
    }
}

struct SampleGrad<'a> {
    dl: &'a mut [f64; 3],
    dh: Option<&'a mut [f64; 8]>,
}

/// What an evaluation pass returns.
pub(super) struct Eval {
    pub cost: f64,
    pub jtj: Matrix,
    pub jtr: Vector,
    /// `J_p r_p` per pixel, when asked for.
    pub scores: Vec<Vector>,
    /// Residual per pixel, when asked for.
    pub residuals: Vec<f64>,
}

/// What to compute in a pass.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Want {
    pub jacobian: bool,
    pub scores: bool,
    pub residuals: bool,
}

/// `Σ` from the parameter vector.
#[inline]
pub(super) fn sigma_of(theta: &Vector) -> [f64; 3] {
    [theta[I_SIGMA], theta[I_SIGMA + 1], theta[I_SIGMA + 2]]
}

/// Eigenvalues `(λ_min, λ_max)` of a symmetric 2 × 2 `[a, b, c]`.
pub(super) fn eig2(s: [f64; 3]) -> (f64, f64) {
    crate::sym2::Sym2x2::new(s[0], s[1], s[2]).eigenvalues()
}

/// Lower Cholesky factor `(l11, l21, l22)` of `[[a, b], [b, c]]`, or `None`
/// when it is not positive definite.
#[inline]
pub(super) fn cholesky2(s: [f64; 3]) -> Option<[f64; 3]> {
    crate::sym2::Sym2x2::new(s[0], s[1], s[2]).cholesky()
}

impl Model<'_> {
    /// Build the per-pass polygon data at `theta`, or `None` when `Σ_c` is
    /// not positive definite.
    pub(super) fn pass_polygons(&self, theta: &Vector) -> Option<(Vec<PassPolygon>, [f64; 3])> {
        let sv = self.box_model.shift_var();
        let s = sigma_of(theta);
        let l = cholesky2([s[0] - sv, s[1], s[2] - sv])?;
        // L⁻¹ = [[i11, 0], [i21, i22]].
        let (i11, i22) = (1.0 / l[0], 1.0 / l[2]);
        let i21 = -l[1] * i11 * i22;
        let whiten = |v: [f64; 2]| [i11 * v[0], i21 * v[0] + i22 * v[1]];
        let hom = LocalHomography {
            h: &theta.as_slice()[I_H..I_H + 8],
        };
        // The patch in whitened space, grown by the kernel's reach.
        let pb = self.pixel_bounds;
        let mut reach = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        for (x, y) in [
            (pb[0], pb[1]),
            (pb[2], pb[1]),
            (pb[2], pb[3]),
            (pb[0], pb[3]),
        ] {
            for (dx, dy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let w = whiten([x + dx * BOX_SHIFT, y + dy * BOX_SHIFT]);
                reach[0] = reach[0].min(w[0] - CUT);
                reach[1] = reach[1].min(w[1] - CUT);
                reach[2] = reach[2].max(w[0] + CUT);
                reach[3] = reach[3].max(w[1] + CUT);
            }
        }
        let mut out = Vec::with_capacity(self.polygons.len());
        for poly in self.polygons {
            let n = poly.vertices.len();
            let mut img = Vec::with_capacity(n);
            let mut dv = Vec::with_capacity(n);
            for &q in &poly.vertices {
                let (r, d) = hom.apply(q);
                img.push([
                    self.c0[0] + self.scale * r[0],
                    self.c0[1] + self.scale * r[1],
                ]);
                dv.push(d);
            }
            let w: Vec<[f64; 2]> = img.iter().map(|&v| whiten(v)).collect();
            let mut bbox = [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ];
            for v in &w {
                bbox[0] = bbox[0].min(v[0]);
                bbox[1] = bbox[1].min(v[1]);
                bbox[2] = bbox[2].max(v[0]);
                bbox[3] = bbox[3].max(v[1]);
            }
            if bbox[2] < reach[0] || bbox[0] > reach[2] || bbox[3] < reach[1] || bbox[1] > reach[3]
            {
                continue;
            }
            let orient = super::gauss_polygon::signed_area(&w).signum();
            let mut edges = Vec::with_capacity(n);
            for a in 0..n {
                let b = (a + 1) % n;
                let e = [w[b][0] - w[a][0], w[b][1] - w[a][1]];
                let len = e[0].hypot(e[1]);
                if len <= 0.0 {
                    continue;
                }
                let t = [e[0] / len, e[1] / len];
                let nr = [orient * t[1], -orient * t[0]];
                // L⁻ᵀ = [[i11, i21], [0, i22]].
                let m = [i11 * nr[0] + i21 * nr[1], i22 * nr[1]];
                let mut pa = [0.0; 8];
                let mut pb = [0.0; 8];
                for k in 0..8 {
                    pa[k] = self.scale * (dv[a][0][k] * m[0] + dv[a][1][k] * m[1]);
                    pb[k] = self.scale * (dv[b][0][k] * m[0] + dv[b][1][k] * m[1]);
                }
                edges.push(PassEdge {
                    a,
                    b,
                    t,
                    len,
                    ch: w[a][0] * t[1] - w[a][1] * t[0],
                    cy: w[a][0] * t[0] + w[a][1] * t[1],
                    m,
                    pa,
                    pb,
                });
            }
            out.push(PassPolygon {
                w,
                edges,
                weight: poly.weight,
                orient,
                bbox,
            });
        }
        Some((out, [i11, i21, i22]))
    }

    /// Blackness at one sample point `q` (whitened), accumulating the
    /// gradient with respect to `(l11, l21, l22)` and (optionally) the
    /// homography when `grad` is given, plus `(T1 + T2)` first-order scatter
    /// edge corrections when `scatter` is given.
    #[inline]
    fn sample(
        polys: &[PassPolygon],
        q: [f64; 2],
        mut grad: Option<SampleGrad<'_>>,
        scatter: Option<&ScatterPass>,
    ) -> (f64, f64) {
        let mut b = 0.0;
        let mut dt12 = 0.0;
        let need_term_grad = grad.is_some();
        for poly in polys {
            if q[0] < poly.bbox[0] - CUT
                || q[0] > poly.bbox[2] + CUT
                || q[1] < poly.bbox[1] - CUT
                || q[1] > poly.bbox[3] + CUT
            {
                continue;
            }
            let mut m = 0.0;
            let mut m_inside = false;
            if q[0] >= poly.bbox[0]
                && q[0] <= poly.bbox[2]
                && q[1] >= poly.bbox[1]
                && q[1] <= poly.bbox[3]
            {
                // Even–odd rule about q.
                let w = &poly.w;
                let n = w.len();
                let mut inside = false;
                let mut j = n - 1;
                for i in 0..n {
                    let (ya, yb) = (w[i][1] - q[1], w[j][1] - q[1]);
                    if (ya > 0.0) != (yb > 0.0) {
                        let xa = w[i][0] - q[0];
                        let xb = w[j][0] - q[0];
                        if xa - ya * (xb - xa) / (yb - ya) > 0.0 {
                            inside = !inside;
                        }
                    }
                    j = i;
                }
                if inside {
                    m = 1.0;
                    m_inside = true;
                }
            }
            let mut on_boundary = false;
            for e in &poly.edges {
                let hs = e.ch - (q[0] * e.t[1] - q[1] * e.t[0]);
                let ya = e.cy - (q[0] * e.t[0] + q[1] * e.t[1]);
                let Some(term) = edge_term(hs, ya, e.len, need_term_grad) else {
                    continue;
                };
                if on_segment(hs, ya, e.len) {
                    on_boundary = true;
                } else {
                    m += poly.orient * term.value;
                }
                let wk = poly.weight;
                if let Some(g) = grad.as_mut() {
                    if let Some(dh) = g.dh.as_deref_mut() {
                        for k in 0..8 {
                            dh[k] += wk * (term.c_a * e.pa[k] + term.c_b * e.pb[k]);
                        }
                    }
                    let za = [poly.w[e.a][0] - q[0], poly.w[e.a][1] - q[1]];
                    let zb = [poly.w[e.b][0] - q[0], poly.w[e.b][1] - q[1]];
                    let sx = term.c_a * za[0] + term.c_b * zb[0];
                    let sy = term.c_a * za[1] + term.c_b * zb[1];
                    g.dl[0] -= wk * e.m[0] * sx;
                    g.dl[1] -= wk * e.m[1] * sx;
                    g.dl[2] -= wk * e.m[1] * sy;
                }
                if let Some(sp) = scatter {
                    if let Some([m0, m1, m2]) = edge_moments(hs, ya, e.len) {
                        dt12 -= 0.5 * wk * (sp.div[0] * e.m[0] + sp.div[1] * e.m[1]) * m0;
                        let za = [poly.w[e.a][0] - q[0], poly.w[e.a][1] - q[1]];
                        let zp = [za[0] - ya * e.t[0], za[1] - ya * e.t[1]];
                        let z00 =
                            m0 * zp[0] * zp[0] + 2.0 * m1 * zp[0] * e.t[0] + m2 * e.t[0] * e.t[0];
                        let z01 = m0 * zp[0] * zp[1]
                            + m1 * (zp[0] * e.t[1] + zp[1] * e.t[0])
                            + m2 * e.t[0] * e.t[1];
                        let z11 =
                            m0 * zp[1] * zp[1] + 2.0 * m1 * zp[1] * e.t[1] + m2 * e.t[1] * e.t[1];
                        for (k, g) in [sp.g1, sp.g2].iter().enumerate() {
                            let gm = [g[0] * e.m[0] + g[1] * e.m[1], g[1] * e.m[0] + g[2] * e.m[1]];
                            let ak = [sp.linv[0] * gm[0], sp.linv[1] * gm[0] + sp.linv[2] * gm[1]];
                            let rk = sp.l_rows[k];
                            let quad = ak[0] * (z00 * rk[0] + z01 * rk[1])
                                + ak[1] * (z01 * rk[0] + z11 * rk[1]);
                            dt12 -= 0.5 * wk * quad;
                        }
                    }
                }
            }
            if on_boundary {
                // The even–odd test is undefined on the boundary.
                let inside = if m_inside { 1.0 } else { 0.0 };
                m += boundary_winding(poly.w.len(), poly.orient, |i| {
                    [poly.w[i][0] - q[0], poly.w[i][1] - q[1]]
                }) - inside;
            }
            b += poly.weight * m;
        }
        (b, dt12)
    }

    pub(super) fn sample_pixel(
        &self,
        polys: &[PassPolygon],
        scatter: Option<&ScatterPass>,
        px: Pixel,
        linv: [f64; 3],
        want_jac: bool,
    ) -> (f64, [f64; 3], [f64; 8]) {
        let [i11, i21, i22] = linv;
        let samples = self.box_model.samples();
        let inv_n = 1.0 / samples.len() as f64;
        let p = [f64::from(px.x), f64::from(px.y)];
        let mut b_raw = 0.0;
        let mut dl = [0.0; 3];
        let mut dh = [0.0; 8];
        for u in samples {
            let x = [p[0] + u[0], p[1] + u[1]];
            let q = [i11 * x[0], i21 * x[0] + i22 * x[1]];
            let mut dl_u = [0.0; 3];
            let grad = if want_jac {
                Some(SampleGrad {
                    dl: &mut dl_u,
                    dh: Some(&mut dh),
                })
            } else if scatter.is_some() {
                Some(SampleGrad {
                    dl: &mut dl_u,
                    dh: None,
                })
            } else {
                None
            };
            let (mut bu, dt12) = Self::sample(polys, q, grad, scatter);
            if let Some(sp) = scatter {
                let db_ds = [
                    dl_u[0] * sp.dls[0][0] + dl_u[1] * sp.dls[1][0] + dl_u[2] * sp.dls[2][0],
                    dl_u[0] * sp.dls[0][1] + dl_u[1] * sp.dls[1][1] + dl_u[2] * sp.dls[2][1],
                    dl_u[0] * sp.dls[0][2] + dl_u[1] * sp.dls[1][2] + dl_u[2] * sp.dls[2][2],
                ];
                let dx_c = x[0] - self.c0[0];
                let dy_c = x[1] - self.c0[1];
                let t0 = dx_c * (sp.g1[0] * db_ds[0] + sp.g1[1] * db_ds[1] + sp.g1[2] * db_ds[2])
                    + dy_c * (sp.g2[0] * db_ds[0] + sp.g2[1] * db_ds[1] + sp.g2[2] * db_ds[2]);
                bu += t0 + dt12;
            }
            if want_jac {
                for k in 0..3 {
                    dl[k] += dl_u[k];
                }
            }
            b_raw += bu;
        }
        b_raw *= inv_n;
        if scatter.is_some() {
            b_raw = b_raw.clamp(0.0, 1.0);
        }
        (b_raw, dl, dh)
    }

    /// `∂(l11, l21, l22)/∂(Σxx, Σxy, Σyy)` at `Σ_c`.
    pub(super) fn dl_dsigma(l: [f64; 3]) -> [[f64; 3]; 3] {
        let [l11, l21, l22] = l;
        [
            [0.5 / l11, 0.0, 0.0],
            [-0.5 * l21 / (l11 * l11), 1.0 / l11, 0.0],
            [
                0.5 * l21 * l21 / (l11 * l11 * l22),
                -l21 / (l11 * l22),
                0.5 / l22,
            ],
        ]
    }

    /// One pass over `pixels` at `theta`. `None` when `θ` is outside the
    /// model's domain (`Σ_c` not positive definite).
    pub(super) fn evaluate(&self, theta: &Vector, pixels: &[Pixel], want: Want) -> Option<Eval> {
        let (polys, linv) = self.pass_polygons(theta)?;
        let sv = self.box_model.shift_var();
        let s = sigma_of(theta);
        let l = cholesky2([s[0] - sv, s[1], s[2] - sv])?;
        let dls = Self::dl_dsigma(l);
        let scatter = self
            .blur_gradient
            .map(|grad| ScatterPass::new(grad, l, linv, dls));
        let inv_n = 1.0 / self.box_model.samples().len() as f64;
        let (a, c, alpha, beta) = (theta[I_A], theta[I_C], theta[I_ALPHA], theta[I_BETA]);
        let mut out = Eval {
            cost: 0.0,
            jtj: Matrix::zeros(),
            jtr: Vector::zeros(),
            scores: Vec::new(),
            residuals: Vec::new(),
        };
        if want.scores {
            out.scores.reserve(pixels.len());
        }
        if want.residuals {
            out.residuals.reserve(pixels.len());
        }
        for px in pixels {
            let p = [f64::from(px.x), f64::from(px.y)];
            let (b_raw, dl, dh) =
                self.sample_pixel(&polys, scatter.as_ref(), *px, linv, want.jacobian);
            let (b, db_db, db_dkappa) = bleed_eval(b_raw, theta[I_BLEED], self.fit_ink_bleed);
            let d = [
                (p[0] - self.c0[0]) / self.scale,
                (p[1] - self.c0[1]) / self.scale,
            ];
            let shade = 1.0 + alpha * d[0] + beta * d[1];
            let inner = c + a * (1.0 - b);
            let lin = shade * inner;
            let (obs, d_obs_d_lin, d_obs_d_gamma) = self.radiometric.forward(lin, theta[I_GAMMA]);
            let r = px.value - obs;
            out.cost += r * r;
            if want.residuals {
                out.residuals.push(r);
            }
            if want.jacobian {
                let mut j = Vector::zeros();
                let k_b = d_obs_d_lin * (-shade * a * inv_n * db_db);
                for (si, js) in j.iter_mut().skip(I_SIGMA).take(3).enumerate() {
                    *js = k_b * (dl[0] * dls[0][si] + dl[1] * dls[1][si] + dl[2] * dls[2][si]);
                }
                j[I_A] = d_obs_d_lin * shade * (1.0 - b);
                j[I_C] = d_obs_d_lin * shade;
                j[I_ALPHA] = d_obs_d_lin * d[0] * inner;
                j[I_BETA] = d_obs_d_lin * d[1] * inner;
                for hk in 0..8 {
                    j[I_H + hk] = k_b * dh[hk];
                }
                j[I_GAMMA] = d_obs_d_gamma;
                j[I_BLEED] = if self.fit_ink_bleed {
                    d_obs_d_lin * (-shade * a * db_dkappa)
                } else {
                    0.0
                };
                for (jk, fixed) in j.iter_mut().zip(self.fixed.iter()) {
                    if *fixed {
                        *jk = 0.0;
                    }
                }
                // Symmetric rank-1 update, upper triangle.
                for col in 0..NP {
                    let jc = j[col];
                    if jc == 0.0 {
                        continue;
                    }
                    for row in 0..=col {
                        out.jtj[(row, col)] += j[row] * jc;
                    }
                }
                out.jtr += j * r;
                if want.scores {
                    out.scores.push(j * r);
                }
            }
        }
        if want.jacobian {
            for col in 0..NP {
                for row in 0..col {
                    out.jtj[(col, row)] = out.jtj[(row, col)];
                }
            }
            for (k, fixed) in self.fixed.iter().enumerate() {
                if *fixed {
                    out.jtj[(k, k)] = 1.0;
                }
            }
        }
        Some(out)
    }

    /// Model blackness with optional probit ink bleed and scatter-PSF expansion
    /// at every pixel, used by VarPro to solve closed-form `(A, C)` without
    /// Jacobian assembly.
    pub(super) fn blackness_with_bleed(
        &self,
        theta: &Vector,
        pixels: &[Pixel],
    ) -> Option<Vec<f64>> {
        let (polys, linv) = self.pass_polygons(theta)?;
        let scatter = if let Some(grad) = self.blur_gradient {
            let sv = self.box_model.shift_var();
            let s = sigma_of(theta);
            let l = cholesky2([s[0] - sv, s[1], s[2] - sv])?;
            let dls = Self::dl_dsigma(l);
            Some(ScatterPass::new(grad, l, linv, dls))
        } else {
            None
        };
        Some(
            pixels
                .iter()
                .map(|px| {
                    let (b_raw, _, _) =
                        self.sample_pixel(&polys, scatter.as_ref(), *px, linv, false);
                    bleed_eval(b_raw, theta[I_BLEED], self.fit_ink_bleed).0
                })
                .collect(),
        )
    }

    /// Model blackness at every pixel for an isotropic component
    /// covariance `Σ_c = σ² I`, with the homography of `theta` — the σ
    /// profile's evaluator.
    pub(super) fn blackness_iso(&self, theta: &Vector, sigma: f64, pixels: &[Pixel]) -> Vec<f64> {
        let mut th = *theta;
        let sv = self.box_model.shift_var();
        th[I_SIGMA] = sigma * sigma + sv;
        th[I_SIGMA + 1] = 0.0;
        th[I_SIGMA + 2] = sigma * sigma + sv;
        let Some((polys, linv)) = self.pass_polygons(&th) else {
            return vec![0.0; pixels.len()];
        };
        let [i11, i21, i22] = linv;
        let samples = self.box_model.samples();
        let inv_n = 1.0 / samples.len() as f64;
        pixels
            .iter()
            .map(|px| {
                samples
                    .iter()
                    .map(|u| {
                        let x = [f64::from(px.x) + u[0], f64::from(px.y) + u[1]];
                        Self::sample(&polys, [i11 * x[0], i21 * x[0] + i22 * x[1]], None, None).0
                    })
                    .sum::<f64>()
                    * inv_n
            })
            .collect()
    }
}

impl Model<'_> {
    /// Whether `θ` is inside the fit's domain.
    pub(super) fn admissible(&self, th: &Vector) -> bool {
        if th.iter().any(|v| !v.is_finite()) {
            return false;
        }
        if !self.fixed[I_GAMMA] && !(MIN_GAMMA..=MAX_GAMMA).contains(&th[I_GAMMA]) {
            return false;
        }
        if !self.fixed[I_BLEED] && th[I_BLEED].abs() > MAX_BLEED_KAPPA {
            return false;
        }
        let (lo, hi) = eig2(sigma_of(th));
        lo >= self.box_model.min_eigenvalue() && hi <= self.scale * self.scale
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn model(polys: &[BoardPolygon], box_model: BoxModel) -> Model<'_> {
        Model {
            polygons: polys,
            c0: [20.3, 19.6],
            scale: 9.0,
            box_model,
            fixed: [false; NP],
            pixel_bounds: [9.0, 8.0, 32.0, 31.0],
            radiometric: RadiometricModel::FreeGamma { initial_gamma: 2.1 },
            fit_ink_bleed: true,
            blur_gradient: None,
        }
    }

    fn theta() -> Vector {
        let mut t = Vector::zeros();
        t[0] = 1.3;
        t[1] = 0.25;
        t[2] = 0.9;
        t[I_A] = 150.0;
        t[I_C] = 35.0;
        t[I_ALPHA] = 0.03;
        t[I_BETA] = -0.02;
        let h = [1.02, 0.11, 0.03, -0.08, 0.97, -0.02, 0.015, -0.01];
        for (k, v) in h.iter().enumerate() {
            t[I_H + k] = *v;
        }
        t[I_GAMMA] = 2.1;
        t[I_BLEED] = 0.15;
        t
    }

    fn pixels() -> Vec<Pixel> {
        let mut v = Vec::new();
        for y in 8..32 {
            for x in 9..33 {
                v.push(Pixel {
                    x,
                    y,
                    value: 100.0 + f64::from((x * 7 + y * 3) % 11),
                });
            }
        }
        v
    }

    #[test]
    fn jacobian_matches_finite_differences() {
        let polys = vec![
            square(-1.0, -1.0, 1.0),
            square(0.0, 0.0, 1.0),
            BoardPolygon {
                vertices: vec![[0.3, 0.0], [0.5, -0.2], [0.7, 0.0]],
                weight: -1.0,
            },
        ];
        for box_model in [BoxModel::Gaussian, BoxModel::Mixture] {
            let m = model(&polys, box_model);
            let px = pixels();
            let th = theta();
            let want = Want {
                jacobian: true,
                scores: true,
                residuals: true,
            };
            let e = m.evaluate(&th, &px, want).expect("in domain");
            for k in 0..NP {
                let eps = if k < 3 {
                    1e-4
                } else if k >= I_H {
                    1e-5
                } else {
                    1e-4
                };
                let mut tp = th;
                tp[k] += eps;
                let mut tm = th;
                tm[k] -= eps;
                let rp = m.evaluate(&tp, &px, want).expect("in domain").residuals;
                let rm = m.evaluate(&tm, &px, want).expect("in domain").residuals;
                for (i, (score, r)) in e.scores.iter().zip(&e.residuals).enumerate() {
                    // score = J r with J = ∂model/∂θ = −∂r/∂θ.
                    let fd = -(rp[i] - rm[i]) / (2.0 * eps);
                    let an = score[k] / r;
                    assert!(
                        (an - fd).abs() < 2e-3 * (1.0 + fd.abs()),
                        "{box_model:?} param {k} pixel {i}: {an} vs {fd}"
                    );
                }
            }
        }
    }

    #[test]
    fn mixture_reproduces_a_box_blurred_edge() {
        // A vertical half-plane x > 20.3 seen through a unit box and an
        // isotropic Gaussian: the exact pixel value is the box average of
        // Φ((x − 20.3)/σ_g), which the mixture must match far better than one
        // Gaussian of the total variance.
        let polys = vec![BoardPolygon {
            vertices: vec![[0.0, -50.0], [50.0, -50.0], [50.0, 50.0], [0.0, 50.0]],
            weight: 1.0,
        }];
        let px: Vec<Pixel> = (14..27)
            .map(|x| Pixel {
                x,
                y: 20,
                value: 0.0,
            })
            .collect();
        for sigma_g in [0.15f64, 0.3, 0.5] {
            let total = sigma_g * sigma_g + 1.0 / 12.0;
            let exact: Vec<f64> = px
                .iter()
                .map(|p| {
                    let n = 4000;
                    (0..n)
                        .map(|i| {
                            let u = -0.5 + (i as f64 + 0.5) / n as f64;
                            super::super::gauss_polygon::cdf((f64::from(p.x) + u - 20.3) / sigma_g)
                        })
                        .sum::<f64>()
                        / n as f64
                })
                .collect();
            let mut th = Vector::zeros();
            th[I_H] = 1.0;
            th[I_H + 4] = 1.0;
            let mut err = [0.0f64; 2];
            for (slot, box_model) in [BoxModel::Mixture, BoxModel::Gaussian].iter().enumerate() {
                let m = Model {
                    polygons: &polys,
                    c0: [20.3, 20.0],
                    scale: 1.0,
                    box_model: *box_model,
                    fixed: [false; NP],
                    pixel_bounds: [14.0, 20.0, 26.0, 20.0],
                    radiometric: RadiometricModel::Linear,
                    fit_ink_bleed: false,
                    blur_gradient: None,
                };
                let sv = box_model.shift_var();
                let comp = (total - sv).sqrt();
                let got = m.blackness_iso(&th, comp, &px);
                err[slot] = got
                    .iter()
                    .zip(&exact)
                    .map(|(g, e)| (g - e).abs())
                    .fold(0.0, f64::max);
            }
            assert!(
                err[0] < 0.25 * err[1],
                "σ_g {sigma_g}: mixture {} vs gaussian {}",
                err[0],
                err[1]
            );
        }
    }
}
