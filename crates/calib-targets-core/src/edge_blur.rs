//! Per-corner edge-blur estimate — an **unstable study API**.
//!
//! Nothing here is part of the crate's semver contract: the items are
//! `#[doc(hidden)]` and exist so the accuracy suite can measure the estimator
//! before any corner type carries its result.
//!
//! # What is estimated
//!
//! `σ` is the Gaussian-equivalent **total** edge blur in image pixels: lens
//! PSF and pixel aperture together (a sharp edge still has the pixel's own
//! `√(1/12)` px). One `σ` per labelled corner, with its standard error. It is
//! the width of the best-fitting error-function edge: for a near-Gaussian PSF
//! that is the PSF's standard deviation; for a PSF of another shape (a
//! sharpening filter's overshoot, a defocus disk, veiling glare) it is a
//! width of the edge's core, not the PSF's second moment.
//!
//! # Model
//!
//! Every grid neighbour `(i ± 1, j)`, `(i, j ± 1)` of a corner defines a
//! *half-edge*: the checker edge from the corner toward that neighbour, its
//! direction the corner → neighbour line (no ChESS axes are used). Its samples
//! are the **raw** pixels (no resampling, which would add blur of its own) at
//! integer pixel centres inside a strip along the half-edge, each with its
//! signed distance `d` from that line; a pixel at 0 or 255 is censored and left
//! out. Half-edge `k` is modelled as an error-function step
//!
//! ```text
//! I = a_k + b_k · Φ((d − d0_k) / σ)
//! ```
//!
//! with its own levels `a_k`, `b_k` and offset `d0_k` and **one `σ` shared by
//! every half-edge of the corner**, fitted by Levenberg–Marquardt. The standard
//! error is `√[s² (JᵀJ)⁻¹]_σσ` with `s²` the residual variance.
//!
//! The line angle is not fitted: a free angle per half-edge was measured on
//! the accuracy suite (radial distortion k₁ = 0.15, 30° perspective, both
//! corner refiners) and changed neither the bias nor the scatter of `σ`.
//!
//! # Window
//!
//! The strip must hold the whole profile and nothing else. In units of the
//! estimated `σ` (iterated: window from `σ`, fit, window from the new `σ`):
//!
//! - along the edge it starts [`REACH`]`·σ` from the crossing grid line (that
//!   edge's own blur is out) and ends at half the edge, or [`REACH`]`·σ` before
//!   the next printed structure along it, whichever comes first;
//! - across, each side spans `2·`[`REACH`]`·σ` — the transition plus a plateau
//!   as wide — but stops [`REACH`]`·σ` before the next printed structure on
//!   that side and at half a cell. Both sides must keep the transition
//!   ([`REACH`]`·σ`) and at least one side the plateau too.
//!
//! Where that structure sits is the target family's ([`EdgeBlurWindow`]).
//!
//! # When there is no estimate
//!
//! A miss is acceptable, a `σ` from a truncated or contaminated profile is
//! not. A corner gets `None` when its window cannot hold the profile at the
//! fitted `σ` **plus [`REACH`] standard errors** (an underestimate shrinks its
//! own margins and can admit the next structure's blur while staying
//! self-consistent; its large standard error is what exposes it), when the fit
//! does not converge or is singular, when the standard error reaches `σ`, or
//! when `σ` falls below the pixel aperture (no edge profile is that sharp).
//! Small cells under strong blur are out of range by construction: a plain
//! board holds `σ` up to about a twelfth of a cell, a ChArUco board about a
//! 48th, a PuzzleBoard a 27th.

use std::collections::HashMap;

use nalgebra::{DMatrix, DVector, Point2, Vector2};
use projective_grid::Coord;

use crate::GrayImageView;

/// Reach of a Gaussian blur in `σ`: an edge `3σ` away leaks `Φ(−3) = 0.13 %`
/// of its contrast, under half an 8-bit gray level on a full-range step.
pub const REACH: f64 = 3.0;

/// Locality: a corner's strips stay within half a cell of it, so each corner
/// measures its own neighbourhood and no two corners share an edge half.
const LOCAL_CELLS: f64 = 0.5;

/// The smallest total blur a pixel can show: the unit pixel aperture alone,
/// `√(1/12)` px. Windows are never built narrower than this.
const PIXEL_APERTURE_SIGMA: f64 = 0.288_675_134_594_812_9;

/// A strip shorter than one pixel pitch along its edge holds no full row of
/// samples.
const MIN_BAND_PX: f64 = 1.0;

/// Window iteration stops when `σ` moves less than its standard error or this
/// fraction, whichever is larger — a numerical convergence tolerance.
const WINDOW_TOLERANCE: f64 = 0.01;
const MAX_WINDOW_ITERATIONS: usize = 12;
const MAX_LM_ITERATIONS: usize = 200;

/// Where a target family's printed structure next to a checker edge sits, in
/// cells (the local corner pitch). The estimator keeps [`REACH`]`·σ` clear of
/// each.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeBlurWindow {
    /// Distance from the corner, along its edge, to the next structure on or
    /// next to that edge.
    pub along_clear_cells: f64,
    /// Distance across the edge, into the light square, to the next structure.
    pub light_clear_cells: f64,
    /// Distance across the edge, into the dark square, to the next structure.
    pub dark_clear_cells: f64,
}

impl EdgeBlurWindow {
    /// A window from its three clearances, in cells.
    pub fn new(along_clear_cells: f64, light_clear_cells: f64, dark_clear_cells: f64) -> Self {
        Self {
            along_clear_cells,
            light_clear_cells,
            dark_clear_cells,
        }
    }

    /// Plain chessboard: the next structures are the far corner along the
    /// edge and the parallel edges a cell across.
    pub fn chessboard() -> Self {
        Self::new(1.0, 1.0, 1.0)
    }

    /// ChArUco: a marker of side `marker_size_rel` cells sits centred in every
    /// light square, so the light side is clear for `(1 − marker_size_rel)/2`
    /// cells; the dark square is plain.
    pub fn charuco(marker_size_rel: f64) -> Self {
        Self::new(1.0, 0.5 * (1.0 - marker_size_rel), 1.0)
    }

    /// PuzzleBoard: a dot of diameter `dot_diameter_rel` cells sits on every
    /// edge midpoint. Along the edge the corner's own dot starts at
    /// `(1 − dot_diameter_rel)/2`; across, the crossing edge's dots, half a cell
    /// away, leave the same clearance on either side.
    pub fn puzzleboard(dot_diameter_rel: f64) -> Self {
        let clear = 0.5 * (1.0 - dot_diameter_rel);
        Self::new(clear, clear, clear)
    }

    /// Checkerboard with circle markers of diameter `circle_diameter_rel`
    /// cells centred in some squares: across, either side may hold a circle,
    /// clear for `(1 − circle_diameter_rel)/2`; along, nothing.
    pub fn marker_board(circle_diameter_rel: f64) -> Self {
        let clear = 0.5 * (1.0 - circle_diameter_rel);
        Self::new(1.0, clear, clear)
    }
}

/// Estimator settings.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeBlurParams {
    /// The target family's structure around an edge.
    pub window: EdgeBlurWindow,
    /// Period of the grid labels, when they wrap (a PuzzleBoard's master
    /// coordinates wrap at 501): neighbours are then found modulo the period.
    pub label_period: Option<i32>,
}

impl EdgeBlurParams {
    /// Settings for `window`, labels not wrapping.
    pub fn new(window: EdgeBlurWindow) -> Self {
        Self {
            window,
            label_period: None,
        }
    }

    /// With wrapping labels of the given period.
    pub fn with_label_period(mut self, period: i32) -> Self {
        self.label_period = Some(period);
        self
    }
}

/// One labelled corner: its image position (pixel centres at integers) and
/// grid label.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeBlurCorner {
    /// Image position, px.
    pub position: Point2<f32>,
    /// Grid label.
    pub grid: Coord,
}

impl EdgeBlurCorner {
    /// A corner at `position` labelled `grid`.
    pub fn new(position: Point2<f32>, grid: Coord) -> Self {
        Self { position, grid }
    }
}

/// One corner's blur estimate.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeBlurEstimate {
    /// Gaussian-equivalent total edge blur, px.
    pub sigma_px: f64,
    /// Standard error of [`Self::sigma_px`], px.
    pub std_error_px: f64,
    /// Half-edges that entered the fit (1–4).
    pub half_edges: usize,
    /// Pixel samples that entered the fit.
    pub samples: usize,
}

/// Estimate the edge blur at every corner of `corners` in `image`; `None` where
/// the window cannot hold the profile or the fit fails (see the module docs).
pub fn estimate_edge_blur(
    image: &GrayImageView<'_>,
    corners: &[EdgeBlurCorner],
    params: &EdgeBlurParams,
) -> Vec<Option<EdgeBlurEstimate>> {
    let key = |c: Coord| match params.label_period {
        Some(p) if p > 0 => (c.u.rem_euclid(p), c.v.rem_euclid(p)),
        _ => (c.u, c.v),
    };
    // Label → corner index; a label claimed twice names no neighbour.
    let mut by_label: HashMap<(i32, i32), Option<usize>> = HashMap::with_capacity(corners.len());
    for (idx, c) in corners.iter().enumerate() {
        by_label
            .entry(key(c.grid))
            .and_modify(|slot| *slot = None)
            .or_insert(Some(idx));
    }
    let at = |p: Point2<f32>| Vector2::new(f64::from(p.x), f64::from(p.y));
    corners
        .iter()
        .map(|c| {
            let nbr = |du: i32, dv: i32| {
                by_label
                    .get(&key(Coord::new(c.grid.u + du, c.grid.v + dv)))
                    .copied()
                    .flatten()
                    .map(|i| at(corners[i].position))
            };
            // +u, −u, +v, −v.
            let nbrs = [nbr(1, 0), nbr(-1, 0), nbr(0, 1), nbr(0, -1)];
            CornerFit::new(image, at(c.position), nbrs, params).and_then(|f| f.estimate())
        })
        .collect()
}

/// The geometry of one half-edge.
#[derive(Clone, Copy, Debug)]
struct HalfEdge {
    /// Unit direction corner → neighbour.
    u: Vector2<f64>,
    /// Unit normal (`u` turned +90°): the sign of `d`.
    n: Vector2<f64>,
    /// Edge length, px.
    length: f64,
    /// Unit normal of the crossing grid line, pointing along `u`.
    m: Vector2<f64>,
}

/// One pixel sample of a half-edge strip.
#[derive(Clone, Copy, Debug)]
struct Sample {
    edge: usize,
    /// Signed distance from the corner → neighbour line, px.
    d: f64,
    value: f64,
}

/// A strip's extent at a given `σ`: along up to `t_end` (and [`REACH`]`·σ`
/// past the crossing line), across up to `w_light` into the light square and
/// `w_dark` into the dark one.
#[derive(Clone, Copy, Debug)]
struct Extent {
    t_end: f64,
    w_light: f64,
    w_dark: f64,
}

struct CornerFit<'a> {
    image: &'a GrayImageView<'a>,
    p: Vector2<f64>,
    edges: Vec<HalfEdge>,
    /// Local pitch across the edges (smallest neighbour distance), px.
    pitch: f64,
    params: &'a EdgeBlurParams,
}

impl<'a> CornerFit<'a> {
    fn new(
        image: &'a GrayImageView<'a>,
        p: Vector2<f64>,
        nbrs: [Option<Vector2<f64>>; 4],
        params: &'a EdgeBlurParams,
    ) -> Option<Self> {
        let pitch = nbrs
            .iter()
            .flatten()
            .map(|q| (q - p).norm())
            .fold(f64::INFINITY, f64::min);
        if !pitch.is_finite() || pitch <= 0.0 {
            return None;
        }
        let mut edges = Vec::with_capacity(4);
        for (k, q) in nbrs.iter().enumerate() {
            let Some(q) = q else { continue };
            let e = q - p;
            let length = e.norm();
            if length <= 0.0 {
                continue;
            }
            let u = e / length;
            let n = Vector2::new(-u.y, u.x);
            // The crossing grid line: through the corner along the other axis.
            let (a, b) = if k < 2 {
                (nbrs[2], nbrs[3])
            } else {
                (nbrs[0], nbrs[1])
            };
            let c = match (a, b) {
                (Some(a), Some(b)) => (a - b).try_normalize(0.0),
                (Some(a), None) => (a - p).try_normalize(0.0),
                (None, Some(b)) => (p - b).try_normalize(0.0),
                (None, None) => Some(n),
            };
            let Some(c) = c else { continue };
            let mut m = Vector2::new(-c.y, c.x);
            if m.dot(&u) < 0.0 {
                m = -m;
            }
            // Grid lines that are nearly parallel leave no strip between them.
            if m.dot(&u) <= f64::EPSILON {
                continue;
            }
            edges.push(HalfEdge { u, n, length, m });
        }
        (!edges.is_empty()).then_some(Self {
            image,
            p,
            edges,
            pitch,
            params,
        })
    }

    /// The strip extent of `edge` at blur `sigma`, or `None` if it cannot hold
    /// the profile.
    fn extent(&self, edge: &HalfEdge, sigma: f64) -> Option<Extent> {
        let w = &self.params.window;
        let reach = REACH * sigma;
        let side = |clear_cells: f64| {
            (2.0 * reach)
                .min(LOCAL_CELLS * self.pitch)
                .min(clear_cells * self.pitch - reach)
        };
        let (w_light, w_dark) = (side(w.light_clear_cells), side(w.dark_clear_cells));
        let t_end = (LOCAL_CELLS * edge.length).min(w.along_clear_cells * edge.length - reach);
        // On the edge line the crossing line's margin is reached at
        // t = reach / (m · u).
        let t_start = reach / edge.m.dot(&edge.u);
        // Both sides hold the transition; at least one also holds a plateau
        // as wide. Without an observed plateau a profile wider than the window
        // fits as a narrower, lower-contrast one and can look self-consistent.
        let holds = w_light.min(w_dark) >= reach && w_light.max(w_dark) >= 2.0 * reach;
        (holds && t_end - t_start >= MIN_BAND_PX).then_some(Extent {
            t_end,
            w_light,
            w_dark,
        })
    }

    fn feasible(&self, sigma: f64) -> bool {
        self.edges.iter().any(|e| self.extent(e, sigma).is_some())
    }

    /// The largest `σ` the window can hold at this corner (bisection on the
    /// monotone feasibility predicate), or `None` if not even the pixel
    /// aperture fits.
    fn sigma_max(&self) -> Option<f64> {
        if !self.feasible(PIXEL_APERTURE_SIGMA) {
            return None;
        }
        let (mut lo, mut hi) = (PIXEL_APERTURE_SIGMA, self.pitch);
        if self.feasible(hi) {
            return Some(hi);
        }
        for _ in 0..50 {
            let mid = 0.5 * (lo + hi);
            if self.feasible(mid) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        Some(lo)
    }

    fn pixel(&self, x: i64, y: i64) -> f64 {
        f64::from(self.image.data[y as usize * self.image.width + x as usize])
    }

    /// Every half-edge's samples at window blur `sigma`. Half-edges that cannot
    /// hold the profile, run off the image or show no contrast are left out.
    fn samples(&self, sigma: f64) -> (Vec<Sample>, usize) {
        let reach = REACH * sigma;
        let mut out = Vec::new();
        let mut used = 0usize;
        for edge in &self.edges {
            let Some(ext) = self.extent(edge, sigma) else {
                continue;
            };
            let w_max = ext.w_light.max(ext.w_dark);
            // Pixels of the strip's bounding box, before polarity is known.
            let corners = [
                self.p + edge.n * w_max,
                self.p - edge.n * w_max,
                self.p + edge.u * ext.t_end + edge.n * w_max,
                self.p + edge.u * ext.t_end - edge.n * w_max,
            ];
            let lo = corners
                .iter()
                .fold(Vector2::repeat(f64::INFINITY), |a, c| a.inf(c));
            let hi = corners
                .iter()
                .fold(Vector2::repeat(f64::NEG_INFINITY), |a, c| a.sup(c));
            let (x0, y0) = (lo.x.ceil() as i64, lo.y.ceil() as i64);
            let (x1, y1) = (hi.x.floor() as i64, hi.y.floor() as i64);
            if x0 < 0 || y0 < 0 || x1 >= self.image.width as i64 || y1 >= self.image.height as i64 {
                continue;
            }
            let mut strip = Vec::new();
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let r = Vector2::new(x as f64, y as f64) - self.p;
                    let t = r.dot(&edge.u);
                    let d = r.dot(&edge.n);
                    if t > ext.t_end || d.abs() > w_max || r.dot(&edge.m) < reach {
                        continue;
                    }
                    let value = self.pixel(x, y);
                    // A pixel at the 8-bit limit is censored: the profile
                    // there is unknown, only bounded.
                    if value <= 0.0 || value >= 255.0 {
                        continue;
                    }
                    strip.push((d, value));
                }
            }
            // Polarity from the symmetric core |d| ≤ REACH σ, which every
            // feasible strip holds on both sides.
            let mean = |pos: bool| {
                let (s, n) = strip
                    .iter()
                    .filter(|(d, _)| d.abs() <= reach && *d != 0.0 && (*d > 0.0) == pos)
                    .fold((0.0, 0usize), |(s, n), (_, v)| (s + v, n + 1));
                (n > 0).then(|| s / n as f64)
            };
            let (Some(plus), Some(minus)) = (mean(true), mean(false)) else {
                continue;
            };
            if plus == minus {
                continue;
            }
            let (w_pos, w_neg) = if plus > minus {
                (ext.w_light, ext.w_dark)
            } else {
                (ext.w_dark, ext.w_light)
            };
            let before = out.len();
            out.extend(
                strip
                    .into_iter()
                    .filter(|(d, _)| *d <= w_pos && *d >= -w_neg)
                    .map(|(d, value)| Sample {
                        edge: used,
                        d,
                        value,
                    }),
            );
            if out.len() == before {
                continue;
            }
            used += 1;
        }
        (out, used)
    }

    fn estimate(&self) -> Option<EdgeBlurEstimate> {
        let sigma_max = self.sigma_max()?;
        // Start from the widest margins the window allows: a profile at most
        // that blurred is then clean, and the window shrinks onto it. Those
        // margins can leave a strip too short to fit; a failed fit is then
        // no evidence, and the search halves the window σ (down to the pixel
        // aperture) before giving up. Only a self-consistent fit is returned.
        let mut sigma_w = sigma_max;
        let mut fitted_once = false;
        for _ in 0..MAX_WINDOW_ITERATIONS {
            let (samples, edges) = self.samples(sigma_w);
            let fit = (edges > 0)
                .then(|| fit_profile(&samples, edges, sigma_w))
                .flatten();
            let Some(fit) = fit else {
                if fitted_once || sigma_w <= PIXEL_APERTURE_SIGMA {
                    return None;
                }
                sigma_w = (0.5 * sigma_w).max(PIXEL_APERTURE_SIGMA);
                continue;
            };
            fitted_once = true;
            if fit.sigma > sigma_max {
                return None;
            }
            // A fit below the pixel aperture keeps the aperture's window: no
            // narrower one is ever built.
            let next = fit.sigma.max(PIXEL_APERTURE_SIGMA);
            // Self-consistent once the window's σ is within the fit's own
            // standard error (a closer match is not resolvable) or within
            // the numerical tolerance.
            if (next - sigma_w).abs() <= (WINDOW_TOLERANCE * sigma_w).max(fit.std_error) {
                // The window must hold the profile for every σ the data cannot
                // exclude, up to REACH standard errors above the estimate: an
                // uncertain σ needs the room of its upper bound. An
                // underestimate shrinks its own margins and lets the next
                // structure's blur in, which can keep it self-consistent; its
                // large standard error is what gives it away. A total blur
                // below the pixel aperture is not an edge profile at all.
                if fit.sigma + REACH * fit.std_error > sigma_max || fit.sigma < PIXEL_APERTURE_SIGMA
                {
                    return None;
                }
                return Some(EdgeBlurEstimate {
                    sigma_px: fit.sigma,
                    std_error_px: fit.std_error,
                    half_edges: edges,
                    samples: samples.len(),
                });
            }
            sigma_w = next;
        }
        None
    }
}

/// A converged profile fit.
#[derive(Clone, Copy, Debug)]
struct ProfileFit {
    sigma: f64,
    std_error: f64,
}

/// Standard normal CDF.
fn phi_cdf(x: f64) -> f64 {
    0.5 * (1.0 + libm::erf(x * std::f64::consts::FRAC_1_SQRT_2))
}

/// Standard normal density.
fn phi_pdf(x: f64) -> f64 {
    const INV_SQRT_TAU: f64 = 0.398_942_280_401_432_7;
    INV_SQRT_TAU * (-0.5 * x * x).exp()
}

/// Parameters per half-edge: `a`, `b`, `d0`. The vector is `[σ, a_0, b_0,
/// d0_0, a_1, …]`.
const PER_EDGE: usize = 3;

fn base(edge: usize) -> usize {
    1 + PER_EDGE * edge
}

/// Model value and its non-zero Jacobian entries `(index, ∂I/∂θ)` for one
/// sample.
fn model(theta: &DVector<f64>, s: &Sample, jac: &mut [(usize, f64); 4]) -> f64 {
    let sigma = theta[0];
    let k = base(s.edge);
    let (a, b, d0) = (theta[k], theta[k + 1], theta[k + 2]);
    let u = (s.d - d0) / sigma;
    let cdf = phi_cdf(u);
    let g = b * phi_pdf(u) / sigma;
    jac[0] = (0, -g * u);
    jac[1] = (k, 1.0);
    jac[2] = (k + 1, cdf);
    jac[3] = (k + 2, -g);
    a + b * cdf
}

/// `JᵀJ`, `Jᵀr` and `Σ r²` at `theta`.
fn normal_equations(theta: &DVector<f64>, samples: &[Sample]) -> (DMatrix<f64>, DVector<f64>, f64) {
    let n = theta.len();
    let mut jtj = DMatrix::<f64>::zeros(n, n);
    let mut jtr = DVector::<f64>::zeros(n);
    let mut cost = 0.0;
    let mut jac = [(0usize, 0.0f64); 4];
    for s in samples {
        let r = s.value - model(theta, s, &mut jac);
        cost += r * r;
        for &(i, ji) in &jac {
            jtr[i] += ji * r;
            for &(k, jk) in &jac {
                jtj[(i, k)] += ji * jk;
            }
        }
    }
    (jtj, jtr, cost)
}

fn cost_at(theta: &DVector<f64>, samples: &[Sample]) -> f64 {
    let mut jac = [(0usize, 0.0f64); 4];
    samples
        .iter()
        .map(|s| {
            let r = s.value - model(theta, s, &mut jac);
            r * r
        })
        .sum()
}

/// Fit the shared-`σ` error-function model to `samples` (Levenberg–Marquardt),
/// starting from `sigma0`. `None` if it does not converge, is singular, or
/// cannot tell `σ` from zero (standard error ≥ `σ`).
fn fit_profile(samples: &[Sample], edges: usize, sigma0: f64) -> Option<ProfileFit> {
    let n = 1 + PER_EDGE * edges;
    if samples.len() <= n {
        return None;
    }
    let mut theta = DVector::<f64>::zeros(n);
    theta[0] = sigma0;
    for k in 0..edges {
        // Initial levels: the mean of each side's outer half.
        let mine = || samples.iter().filter(move |s| s.edge == k);
        let w_neg = mine().map(|s| -s.d).fold(0.0, f64::max);
        let w_pos = mine().map(|s| s.d).fold(0.0, f64::max);
        let mean = |f: &dyn Fn(&Sample) -> bool| {
            let (sum, cnt) = mine()
                .filter(|s| f(s))
                .fold((0.0, 0usize), |(a, c), s| (a + s.value, c + 1));
            (cnt > 0).then(|| sum / cnt as f64)
        };
        let lo = mean(&|s| s.d <= -0.5 * w_neg).or_else(|| mean(&|s| s.d < 0.0))?;
        let hi = mean(&|s| s.d >= 0.5 * w_pos).or_else(|| mean(&|s| s.d > 0.0))?;
        theta[base(k)] = lo;
        theta[base(k) + 1] = hi - lo;
    }

    let (mut jtj, mut jtr, mut cost) = normal_equations(&theta, samples);
    let mut lambda = 1e-3;
    let mut converged = false;
    for _ in 0..MAX_LM_ITERATIONS {
        let mut a = jtj.clone();
        for i in 0..n {
            a[(i, i)] += lambda * jtj[(i, i)].max(f64::MIN_POSITIVE);
        }
        let Some(chol) = a.cholesky() else {
            lambda *= 10.0;
            if lambda > 1e12 {
                break;
            }
            continue;
        };
        let step = chol.solve(&jtr);
        let trial = &theta + &step;
        let trial_cost = if trial[0] > 0.0 {
            cost_at(&trial, samples)
        } else {
            f64::INFINITY
        };
        if trial_cost < cost {
            let decrease = cost - trial_cost;
            theta = trial;
            (jtj, jtr, cost) = normal_equations(&theta, samples);
            lambda = (lambda * 0.1).max(1e-12);
            if decrease <= 1e-12 * cost.max(f64::MIN_POSITIVE) || step.amax() <= 1e-10 * theta[0] {
                converged = true;
                break;
            }
        } else {
            lambda *= 10.0;
            if lambda > 1e12 {
                // No step improves the cost: a minimum to machine precision.
                converged = true;
                break;
            }
        }
    }
    if !converged {
        return None;
    }
    let sigma = theta[0];
    let dof = (samples.len() - n) as f64;
    let s2 = cost / dof;
    let inv = jtj.cholesky()?.inverse();
    let var = s2 * inv[(0, 0)];
    if !(sigma.is_finite() && sigma > 0.0 && var.is_finite() && var >= 0.0) {
        return None;
    }
    let std_error = var.sqrt();
    (std_error < sigma).then_some(ProfileFit { sigma, std_error })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SplitMix64 + Box–Muller: seeded test noise.
    struct Rng(u64);

    impl Rng {
        fn unit(&mut self) -> f64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^= z >> 31;
            ((z >> 11) as f64 + 0.5) / (1u64 << 53) as f64
        }

        fn gauss(&mut self) -> f64 {
            let (u1, u2) = (self.unit(), self.unit());
            (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
        }
    }

    /// The ±1 square wave `(−1)^⌊v⌋` blurred by a Gaussian of `s` (same units).
    fn blurred_square_wave(v: f64, s: f64) -> f64 {
        let k = v.floor() as i64;
        let mut acc = 0.0;
        for j in (k - 6)..=(k + 6) {
            let sign = if j.rem_euclid(2) == 0 { 1.0 } else { -1.0 };
            let j = j as f64;
            acc += sign * (phi_cdf((v - j) / s) - phi_cdf((v - j - 1.0) / s));
        }
        acc
    }

    /// A checker of `cell` px squares rotated by `angle`, blurred by an
    /// isotropic Gaussian of `sigma` px and point-sampled at pixel centres, so
    /// the truth is `sigma` itself. The checker is a product of two square
    /// waves and the Gaussian is separable in the board frame, so the blurred
    /// checker is the product of the blurred waves — exact.
    fn checker(
        size: usize,
        cell: f64,
        angle: f64,
        sigma: f64,
        noise: f64,
        seed: u64,
    ) -> (Vec<u8>, Vec<EdgeBlurCorner>) {
        let (s, c) = angle.sin_cos();
        let centre = Vector2::new(size as f64 * 0.5 + 0.3, size as f64 * 0.5 - 0.2);
        let mut rng = Rng(seed);
        let mut data = vec![0u8; size * size];
        for y in 0..size {
            for x in 0..size {
                let r = Vector2::new(x as f64, y as f64) - centre;
                let bx = (c * r.x + s * r.y) / cell;
                let by = (-s * r.x + c * r.y) / cell;
                let w =
                    blurred_square_wave(bx, sigma / cell) * blurred_square_wave(by, sigma / cell);
                let v = 40.0 + 85.0 * (1.0 + w) + noise * rng.gauss();
                data[y * size + x] = v.round().clamp(0.0, 255.0) as u8;
            }
        }
        let mut corners = Vec::new();
        for j in -2..=2 {
            for i in -2..=2 {
                let (bx, by) = (f64::from(i) * cell, f64::from(j) * cell);
                let p = centre + Vector2::new(c * bx - s * by, s * bx + c * by);
                corners.push(EdgeBlurCorner::new(
                    Point2::new(p.x as f32, p.y as f32),
                    Coord::new(i, j),
                ));
            }
        }
        (data, corners)
    }

    /// Index of label (0, 0) in [`checker`]'s corner list.
    const CENTRE: usize = 12;

    fn centre_estimate(
        cell: f64,
        sigma: f64,
        noise: f64,
        seed: u64,
        window: EdgeBlurWindow,
    ) -> Option<EdgeBlurEstimate> {
        let size = (5.0 * cell) as usize;
        let (data, corners) = checker(size, cell, 0.3, sigma, noise, seed);
        let view = GrayImageView {
            width: size,
            height: size,
            data: &data,
        };
        estimate_edge_blur(&view, &corners, &EdgeBlurParams::new(window))[CENTRE]
    }

    #[test]
    fn recovers_a_point_sampled_gaussian_edge() {
        // A 32 px cell holds σ up to 32 / 12 px.
        for sigma in [0.7, 1.0, 2.0, 2.6] {
            let est = centre_estimate(32.0, sigma, 0.0, 1, EdgeBlurWindow::chessboard())
                .unwrap_or_else(|| panic!("σ {sigma}: no estimate"));
            // Noise-free but 8-bit: only the quantisation remains.
            let rel = est.sigma_px / sigma - 1.0;
            assert!(rel.abs() < 0.01, "σ {sigma}: got {}", est.sigma_px);
            assert_eq!(est.half_edges, 4);
        }
    }

    #[test]
    fn standard_error_matches_the_scatter_under_noise() {
        let sigma = 1.2;
        let est: Vec<EdgeBlurEstimate> = (0..40)
            .map(|seed| {
                centre_estimate(32.0, sigma, 4.0, seed, EdgeBlurWindow::chessboard())
                    .expect("estimate")
            })
            .collect();
        let n = est.len() as f64;
        let mean = est.iter().map(|e| e.sigma_px).sum::<f64>() / n;
        let sd = (est.iter().map(|e| (e.sigma_px - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt();
        let se = est.iter().map(|e| e.std_error_px).sum::<f64>() / n;
        assert!((mean / sigma - 1.0).abs() < 0.02, "mean {mean}");
        // A 40-draw standard deviation is itself uncertain by ~11 %; allow
        // about three of those.
        let ratio = sd / se;
        assert!((0.7..1.35).contains(&ratio), "scatter {sd} vs SE {se}");
    }

    #[test]
    fn declines_when_the_window_cannot_hold_the_profile() {
        // At a 24 px cell a ChArUco light side is clear for 3 px; a 1 px blur
        // needs 6 px (3σ of profile, 3σ off the marker border).
        assert!(centre_estimate(24.0, 1.0, 1.0, 3, EdgeBlurWindow::charuco(0.75)).is_none());
        // The same profile on a plain board of the same cell fits.
        assert!(centre_estimate(24.0, 1.0, 1.0, 3, EdgeBlurWindow::chessboard()).is_some());
        // A plain board holds σ up to cell / 12: 2 px here.
        assert!(centre_estimate(24.0, 1.9, 1.0, 3, EdgeBlurWindow::chessboard()).is_some());
        assert!(centre_estimate(24.0, 2.4, 1.0, 3, EdgeBlurWindow::chessboard()).is_none());
    }

    #[test]
    fn wrapping_labels_find_their_neighbours() {
        let (data, mut corners) = checker(160, 32.0, 0.3, 1.0, 0.0, 1);
        // Labels −2..=2 become 499, 500, 0, 1, 2 under a period of 501.
        for c in &mut corners {
            c.grid = Coord::new(c.grid.u.rem_euclid(501), c.grid.v);
        }
        let view = GrayImageView {
            width: 160,
            height: 160,
            data: &data,
        };
        let window = EdgeBlurWindow::chessboard();
        let plain = estimate_edge_blur(&view, &corners, &EdgeBlurParams::new(window));
        let wrapped = estimate_edge_blur(
            &view,
            &corners,
            &EdgeBlurParams::new(window).with_label_period(501),
        );
        // Label (0, 0)'s −u neighbour is (500, 0): found only modulo 501.
        assert_eq!(plain[CENTRE].map(|e| e.half_edges), Some(3));
        assert_eq!(wrapped[CENTRE].map(|e| e.half_edges), Some(4));
    }

    #[test]
    fn duplicate_labels_name_no_neighbour() {
        let (data, mut corners) = checker(160, 32.0, 0.3, 1.0, 0.0, 1);
        // Corner (1, 0) relabelled as a second (−1, 0).
        corners[CENTRE + 1].grid = Coord::new(-1, 0);
        let view = GrayImageView {
            width: 160,
            height: 160,
            data: &data,
        };
        let out = estimate_edge_blur(
            &view,
            &corners,
            &EdgeBlurParams::new(EdgeBlurWindow::chessboard()),
        );
        assert_eq!(out[CENTRE].map(|e| e.half_edges), Some(2));
    }
}
