//! The probability mass of a polygon under a 2-D Gaussian, and its gradient
//! with respect to the polygon's vertices.
//!
//! Everything here works in *whitened* coordinates: a Gaussian `N(p, Σ)` with
//! `Σ = L Lᵀ` becomes the standard normal once a point `x` is mapped to
//! `z = L⁻¹ (x − p)`, and the mass of a polygon is unchanged by the map (the
//! density and the area element scale together). The caller maps vertices and
//! chains the gradients back (see `super::model`).
//!
//! # The mass as a sum over edges
//!
//! Fan the polygon from the origin: its mass is the sum over edges `a → b` of
//! the signed mass of the triangle `(0, z_a, z_b)`. With `t` the unit edge
//! direction, `h = |z_a × t|` the distance from the origin to the edge's line
//! and `y_a = z_a · t`, `y_b = y_a + ℓ` the endpoints' positions along it, a
//! triangle's mass is
//!
//! ```text
//! (θ_b − θ_a) / 2π − [T(h, y_b/h) − T(h, y_a/h)],     θ = atan(y / h),
//! ```
//!
//! with `T` Owen's T function. The angle terms of all edges add up to the
//! winding number of the polygon about the origin, so
//!
//! ```text
//! mass = inside(0) − o · Σ_edges sgn(z_a × t) · [F(h, y_b) − F(h, y_a)],
//! F(h, y) = T(h, y / h),
//! ```
//!
//! `o = ±1` the polygon's orientation. `F` vanishes (below `½ Q(CUT)`,
//! 1.4·10⁻⁷) once the edge's line is [`CUT`] or more away, and the difference
//! `F(h, y_b) − F(h, y_a)` vanishes once the whole segment lies beyond
//! [`CUT`] on one side of the foot of the perpendicular, so an edge costs
//! nothing unless it passes within [`CUT`] standard deviations of the sample.
//!
//! # The gradient
//!
//! Moving the boundary changes the mass by the boundary integral of the
//! density times the outward normal displacement. Along an edge the standard
//! normal density is `φ(h) φ(y)`; with linear (hat) weights for the two
//! endpoints, the gradients with respect to `z_a` and `z_b` are both along the
//! outward normal `n`, with coefficients
//!
//! ```text
//! c_a = φ(h) (I₀ − I₁ / ℓ),   c_b = φ(h) I₁ / ℓ,
//! I₀ = Φ(y_b) − Φ(y_a),       I₁ = φ(y_a) − φ(y_b) − y_a I₀.
//! ```
//!
//! # Owen's T
//!
//! `T(h, a)` is needed for `0 ≤ h < CUT` and `0 ≤ a ≤ 1` only: for `a > 1`
//! the identity `T(h, a) = ½ [Φ(h) Q(ah) + Φ(ah) Q(h)] − T(ah, 1/a)` maps the
//! argument back, and `T(ah, 1/a)` is negligible once `ah ≥ CUT`. On that
//! rectangle `T` is analytic, so it is tabulated once (bicubic Hermite with
//! the analytic partial derivatives, absolute error below 10⁻⁹) instead of
//! integrated per call.

use std::f64::consts::FRAC_1_SQRT_2;
use std::sync::OnceLock;

/// Standard deviations beyond which an edge's contribution is dropped: the
/// neglected mass is below `½ Q(5) ≈ 1.4·10⁻⁷` per edge.
pub(super) const CUT: f64 = 5.0;

const INV_SQRT_TAU: f64 = 0.398_942_280_401_432_7;
const INV_TAU: f64 = 0.159_154_943_091_895_35;

/// Standard normal density.
#[inline]
pub(super) fn pdf(x: f64) -> f64 {
    INV_SQRT_TAU * (-0.5 * x * x).exp()
}

/// Standard normal CDF `Φ`.
#[inline]
pub(super) fn cdf(x: f64) -> f64 {
    0.5 * libm::erfc(-x * FRAC_1_SQRT_2)
}

// ---------------------------------------------------------------------------
// Φ and φ on [−CUT, CUT], tabulated: the hot loop needs both at every edge
// endpoint, and the library `erfc` dominates the cost otherwise.

/// Table steps per unit of `x`.
const N_PER_UNIT: f64 = 64.0;
const N_STEPS: usize = (2.0 * CUT * N_PER_UNIT) as usize;

/// Per node `[Φ, φ, φ' = −x φ]`, from `x = −CUT`.
fn normal_table() -> &'static [[f64; 3]] {
    static TABLE: OnceLock<Vec<[f64; 3]>> = OnceLock::new();
    TABLE.get_or_init(|| {
        (0..=N_STEPS)
            .map(|i| {
                let x = -CUT + i as f64 / N_PER_UNIT;
                [cdf(x), pdf(x), -x * pdf(x)]
            })
            .collect()
    })
}

/// `(Φ(x), φ(x))` by cubic Hermite interpolation for `|x| < CUT`
/// (absolute error below 10⁻¹⁰); `(0 or 1, 0)` beyond — the same neglect
/// bound as [`CUT`] itself.
#[inline]
pub(super) fn normal(x: f64) -> (f64, f64) {
    if x <= -CUT {
        return (0.0, 0.0);
    }
    if x >= CUT {
        return (1.0, 0.0);
    }
    let table = normal_table();
    let f = (x + CUT) * N_PER_UNIT;
    let i = (f as usize).min(N_STEPS - 1);
    let (v, d) = hermite(f - i as f64);
    let (a, b) = (&table[i], &table[i + 1]);
    let step = 1.0 / N_PER_UNIT;
    (
        v[0] * a[0] + v[1] * b[0] + step * (d[0] * a[1] + d[1] * b[1]),
        v[0] * a[1] + v[1] * b[1] + step * (d[0] * a[2] + d[1] * b[2]),
    )
}

// ---------------------------------------------------------------------------
// Owen's T.

/// 16-point Gauss–Legendre on `[-1, 1]`: the positive nodes and their weights.
const GL_X: [f64; 8] = [
    0.095_012_509_837_637_44,
    0.281_603_550_779_258_9,
    0.458_016_777_657_227_4,
    0.617_876_244_402_643_7,
    0.755_404_408_355_003,
    0.865_631_202_387_831_7,
    0.944_575_023_073_232_6,
    0.989_400_934_991_649_9,
];
const GL_W: [f64; 8] = [
    0.189_450_610_455_068_5,
    0.182_603_415_044_923_6,
    0.169_156_519_395_002_5,
    0.149_595_988_816_576_7,
    0.124_628_971_255_533_9,
    0.095_158_511_682_492_8,
    0.062_253_523_938_647_9,
    0.027_152_459_411_754_1,
];

/// `T(h, a)` for `0 ≤ a ≤ 1` by two 16-point Gauss–Legendre panels — the
/// reference the table is built from (the integrand is analytic and slowly
/// varying on `[0, 1]`; the panels agree with a 10⁶-point midpoint rule to
/// 10⁻¹⁴).
fn owen_t_quadrature(h: f64, a: f64) -> f64 {
    let hh = -0.5 * h * h;
    let quarter = 0.25 * a;
    let mut acc = 0.0;
    for centre in [quarter, 3.0 * quarter] {
        for k in 0..8 {
            for s in [-1.0, 1.0] {
                let x = centre + s * quarter * GL_X[k];
                let q = 1.0 + x * x;
                acc += GL_W[k] * (hh * q).exp() / q;
            }
        }
    }
    acc * quarter * INV_TAU
}

/// Table steps: `h` over `[0, CUT]`, `a` over `[0, 1]`.
const T_H_STEPS: usize = 160;
const T_A_STEPS: usize = 32;
const T_DH: f64 = CUT / T_H_STEPS as f64;
const T_DA: f64 = 1.0 / T_A_STEPS as f64;

/// Bicubic Hermite table of `T` on `[0, CUT] × [0, 1]`: per node `T`,
/// `Δh ∂T/∂h`, `Δa ∂T/∂a` and `Δh Δa ∂²T/∂h∂a`, row-major in `h`.
struct OwenTable {
    nodes: Vec<[f64; 4]>,
}

impl OwenTable {
    fn build() -> Self {
        let mut nodes = Vec::with_capacity((T_H_STEPS + 1) * (T_A_STEPS + 1));
        for ih in 0..=T_H_STEPS {
            let h = ih as f64 * T_DH;
            for ia in 0..=T_A_STEPS {
                let a = ia as f64 * T_DA;
                let q = 1.0 + a * a;
                // ∂T/∂h = −φ(h)(Φ(ah) − ½); ∂T/∂a = e^{−h²q/2}/(2π q);
                // ∂²T/∂h∂a = −h φ(h) φ(ah).
                let t_h = -pdf(h) * (cdf(a * h) - 0.5);
                let t_a = (-0.5 * h * h * q).exp() * INV_TAU / q;
                let t_ha = -h * pdf(h) * pdf(a * h);
                nodes.push([
                    owen_t_quadrature(h, a),
                    t_h * T_DH,
                    t_a * T_DA,
                    t_ha * T_DH * T_DA,
                ]);
            }
        }
        Self { nodes }
    }
}

fn owen_table() -> &'static OwenTable {
    static TABLE: OnceLock<OwenTable> = OnceLock::new();
    TABLE.get_or_init(OwenTable::build)
}

/// Cubic Hermite basis at `t ∈ [0, 1]`: value weights of the two nodes and
/// the (step-scaled) derivative weights.
#[inline]
fn hermite(t: f64) -> ([f64; 2], [f64; 2]) {
    let t2 = t * t;
    let t3 = t2 * t;
    (
        [2.0 * t3 - 3.0 * t2 + 1.0, -2.0 * t3 + 3.0 * t2],
        [t3 - 2.0 * t2 + t, t3 - t2],
    )
}

/// Owen's `T(h, a)` for `0 ≤ h ≤ CUT`, `0 ≤ a ≤ 1`, from the table.
#[inline]
pub(super) fn owen_t(h: f64, a: f64) -> f64 {
    let table = owen_table();
    let fh = (h / T_DH).clamp(0.0, T_H_STEPS as f64);
    let fa = (a / T_DA).clamp(0.0, T_A_STEPS as f64);
    let ih = (fh as usize).min(T_H_STEPS - 1);
    let ia = (fa as usize).min(T_A_STEPS - 1);
    let (vh, dh) = hermite(fh - ih as f64);
    let (va, da) = hermite(fa - ia as f64);
    let mut acc = 0.0;
    for (i, (wv_h, wd_h)) in vh.iter().zip(dh.iter()).enumerate() {
        let row = (ih + i) * (T_A_STEPS + 1);
        for (j, (wv_a, wd_a)) in va.iter().zip(da.iter()).enumerate() {
            let n = &table.nodes[row + ia + j];
            acc +=
                wv_h * wv_a * n[0] + wd_h * wv_a * n[1] + wv_h * wd_a * n[2] + wd_h * wd_a * n[3];
        }
    }
    acc
}

/// `F(h, y) = T(h, y / h)` for `0 ≤ h < CUT` (odd in `y`), given `Φ(h)`,
/// `Q(h) = 1 − Φ(h)` and `Φ(y)`.
#[inline]
fn f_edge(h: f64, cdf_h: f64, q_h: f64, y: f64, cdf_y: f64) -> f64 {
    let ay = y.abs();
    let v = if ay <= h {
        if h <= 0.0 {
            return 0.0;
        }
        owen_t(h, ay / h)
    } else {
        // Φ(|y|) and Q(|y|) from Φ(y).
        let (cdf_ay, q_ay) = if y >= 0.0 {
            (cdf_y, 1.0 - cdf_y)
        } else {
            (1.0 - cdf_y, cdf_y)
        };
        let swap = if ay < CUT { owen_t(ay, h / ay) } else { 0.0 };
        0.5 * (cdf_h * q_ay + cdf_ay * q_h) - swap
    };
    v.copysign(y)
}

/// One edge's share of a polygon's mass about a sample point, and the
/// coefficients of its gradient.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct EdgeTerm {
    /// The edge's term of `mass − inside`, before the orientation sign:
    /// `−sgn(z_a × t) [F(h, y_b) − F(h, y_a)]`.
    pub value: f64,
    /// `∂mass/∂z_a = o · c_a · n_right` (see the module docs).
    pub c_a: f64,
    /// `∂mass/∂z_b = o · c_b · n_right`.
    pub c_b: f64,
}

/// Evaluate one edge of a polygon about the origin of whitened space.
///
/// `hs = z_a × t` (signed distance of the line), `ya = z_a · t`, `len = ℓ`.
/// Returns `None` when the segment does not pass within [`CUT`]: its value
/// and gradient are then below the neglect bound.
#[inline]
pub(super) fn edge_term(hs: f64, ya: f64, len: f64, gradient: bool) -> Option<EdgeTerm> {
    let yb = ya + len;
    let h = hs.abs();
    if h >= CUT || ya >= CUT || yb <= -CUT {
        return None;
    }
    let (cdf_h, pdf_h) = normal(h);
    let q_h = 1.0 - cdf_h;
    let (cdf_a, pdf_a) = normal(ya);
    let (cdf_b, pdf_b) = normal(yb);
    let diff = f_edge(h, cdf_h, q_h, yb, cdf_b) - f_edge(h, cdf_h, q_h, ya, cdf_a);
    // sgn(hs) with a fixed side for a sample exactly on the line.
    let side = if hs < 0.0 { -1.0 } else { 1.0 };
    let mut term = EdgeTerm {
        value: -side * diff,
        ..EdgeTerm::default()
    };
    if gradient {
        let i0 = cdf_b - cdf_a;
        let i1 = pdf_a - pdf_b - ya * i0;
        term.c_a = pdf_h * (i0 - i1 / len);
        term.c_b = pdf_h * i1 / len;
    }
    Some(term)
}

/// First three raw Gaussian edge moments along `t` (`φ(h) ∫_{ya}^{yb} y^k φ(y) dy`
/// for `k = 0, 1, 2`), used by the first-order 2-D scatter-PSF expansion.
#[inline]
pub(super) fn edge_moments(hs: f64, ya: f64, len: f64) -> Option<[f64; 3]> {
    let yb = ya + len;
    let h = hs.abs();
    if h >= CUT || ya >= CUT || yb <= -CUT {
        return None;
    }
    let (_, pdf_h) = normal(h);
    let (cdf_a, pdf_a) = normal(ya);
    let (cdf_b, pdf_b) = normal(yb);
    let i0 = cdf_b - cdf_a;
    let i1 = pdf_a - pdf_b;
    let i2 = ya * pdf_a - yb * pdf_b + i0;
    Some([pdf_h * i0, pdf_h * i1, pdf_h * i2])
}

/// Whitened distance below which a sample counts as lying on an edge.
const ON_EDGE: f64 = 1e-9;

/// Whether the origin lies on the segment (`hs`, `ya`, `len` as in
/// [`edge_term`]). Its triangle is then degenerate (zero mass), and the
/// inside test is undefined: the caller drops the edge's value term and
/// takes the winding from [`boundary_winding`] instead.
#[inline]
pub(super) fn on_segment(hs: f64, ya: f64, len: f64) -> bool {
    hs.abs() < ON_EDGE && ya <= ON_EDGE && ya + len >= -ON_EDGE
}

/// `o ·` (sum of the angles the non-degenerate edges subtend at the origin)
/// `/ 2π`: the fraction of the Gaussian's centre the polygon covers when the
/// origin lies on its boundary (½ on an edge, the interior angle over 2π at
/// a vertex). `vertex(i)` is `z_i`; `orient` the polygon's orientation.
pub(super) fn boundary_winding(n: usize, orient: f64, vertex: impl Fn(usize) -> [f64; 2]) -> f64 {
    let mut acc = 0.0;
    for i in 0..n {
        let (a, b) = (vertex(i), vertex((i + 1) % n));
        let e = [b[0] - a[0], b[1] - a[1]];
        let len = e[0].hypot(e[1]);
        if len <= 0.0 {
            continue;
        }
        let t = [e[0] / len, e[1] / len];
        let hs = a[0] * t[1] - a[1] * t[0];
        let ya = a[0] * t[0] + a[1] * t[1];
        if on_segment(hs, ya, len) {
            continue;
        }
        acc += (a[0] * b[1] - a[1] * b[0]).atan2(a[0] * b[0] + a[1] * b[1]);
    }
    orient * acc * INV_TAU
}

/// Whether the origin lies inside the closed polygon `z` (even–odd rule).
#[cfg(test)]
fn contains_origin(z: &[[f64; 2]]) -> bool {
    let n = z.len();
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (z[i], z[j]);
        if (a[1] > 0.0) != (b[1] > 0.0) {
            let x = a[0] + (0.0 - a[1]) * (b[0] - a[0]) / (b[1] - a[1]);
            if x > 0.0 {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

/// Signed area (shoelace); positive for counter-clockwise in a y-up frame.
pub(super) fn signed_area(v: &[[f64; 2]]) -> f64 {
    let n = v.len();
    let mut acc = 0.0;
    for i in 0..n {
        let (a, b) = (v[i], v[(i + 1) % n]);
        acc += a[0] * b[1] - a[1] * b[0];
    }
    0.5 * acc
}

/// Standard-normal mass of the polygon `z` (whitened, relative to the
/// sample) and, when `grad` is given, its gradient with respect to every
/// vertex accumulated into `grad` (same length as `z`).
///
/// The direct form of what the model's hot loop inlines; the reference the
/// tests check.
#[cfg(test)]
fn polygon_mass(z: &[[f64; 2]], mut grad: Option<&mut [[f64; 2]]>) -> f64 {
    let n = z.len();
    let o = signed_area(z).signum();
    let mut m = 0.0;
    let mut on_boundary = false;
    for i in 0..n {
        let (a, b) = (z[i], z[(i + 1) % n]);
        let e = [b[0] - a[0], b[1] - a[1]];
        let len = e[0].hypot(e[1]);
        if len <= 0.0 {
            continue;
        }
        let t = [e[0] / len, e[1] / len];
        let hs = a[0] * t[1] - a[1] * t[0];
        let ya = a[0] * t[0] + a[1] * t[1];
        if let Some(term) = edge_term(hs, ya, len, grad.is_some()) {
            if on_segment(hs, ya, len) {
                on_boundary = true;
            } else {
                m += o * term.value;
            }
            if let Some(g) = grad.as_deref_mut() {
                let nr = [o * t[1], -o * t[0]];
                g[i][0] += term.c_a * nr[0];
                g[i][1] += term.c_a * nr[1];
                let k = (i + 1) % n;
                g[k][0] += term.c_b * nr[0];
                g[k][1] += term.c_b * nr[1];
            }
        }
    }
    m + if on_boundary {
        boundary_winding(n, o, |i| z[i])
    } else if contains_origin(z) {
        1.0
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Standard-normal mass of polygon `z` by rows: exact along `x` (the
    /// even–odd crossings, `Φ` differences); along `y`, Gauss–Legendre
    /// panels between consecutive vertex heights, where the row integral is
    /// smooth.
    fn brute_mass(z: &[[f64; 2]]) -> f64 {
        let row = |y: f64| -> f64 {
            let mut xs = Vec::new();
            let k = z.len();
            for i in 0..k {
                let (a, b) = (z[i], z[(i + 1) % k]);
                if (a[1] > y) != (b[1] > y) {
                    xs.push(a[0] + (y - a[1]) * (b[0] - a[0]) / (b[1] - a[1]));
                }
            }
            xs.sort_by(f64::total_cmp);
            xs.chunks(2).map(|c| cdf(c[1]) - cdf(c[0])).sum::<f64>() * pdf(y)
        };
        let mut breaks: Vec<f64> = z.iter().map(|v| v[1].clamp(-9.0, 9.0)).collect();
        breaks.push(-9.0);
        breaks.push(9.0);
        breaks.sort_by(f64::total_cmp);
        breaks.dedup();
        let mut acc = 0.0;
        for w in breaks.windows(2) {
            let panels = 64;
            let step = (w[1] - w[0]) / panels as f64;
            for p in 0..panels {
                let mid = w[0] + (p as f64 + 0.5) * step;
                for k in 0..8 {
                    for sgn in [-1.0, 1.0] {
                        acc += 0.5 * step * GL_W[k] * row(mid + sgn * 0.5 * step * GL_X[k]);
                    }
                }
            }
        }
        acc
    }

    #[test]
    fn normal_table_matches_the_library() {
        let mut worst: f64 = 0.0;
        for i in 1..20_011 {
            let x = -CUT + 2.0 * CUT * i as f64 / 20_011.0;
            let (c, p) = normal(x);
            worst = worst.max((c - cdf(x)).abs()).max((p - pdf(x)).abs());
        }
        assert!(worst < 1e-9, "table error {worst:e}");
    }

    #[test]
    fn owen_table_matches_quadrature() {
        let mut worst: f64 = 0.0;
        for i in 0..=173 {
            let h = CUT * i as f64 / 173.0;
            for j in 0..=67 {
                let a = j as f64 / 67.0;
                worst = worst.max((owen_t(h, a) - owen_t_quadrature(h, a)).abs());
            }
        }
        assert!(worst < 3e-9, "table error {worst:e}");
    }

    #[test]
    fn owen_quadrature_matches_midpoint_rule() {
        for h in [0.0, 0.3, 1.0, 2.5, 4.9] {
            for a in [0.05, 0.5, 1.0] {
                let n = 200_000;
                let mut acc = 0.0;
                for i in 0..n {
                    let x = a * (i as f64 + 0.5) / n as f64;
                    let q = 1.0 + x * x;
                    acc += (-0.5 * h * h * q).exp() / q;
                }
                let reference = acc * a / n as f64 * INV_TAU;
                let got = owen_t_quadrature(h, a);
                assert!(
                    (got - reference).abs() < 1e-11,
                    "h {h} a {a}: {got} vs {reference}"
                );
            }
        }
    }

    fn shifted(poly: &[[f64; 2]], p: [f64; 2], scale: f64) -> Vec<[f64; 2]> {
        poly.iter()
            .map(|v| [(v[0] - p[0]) / scale, (v[1] - p[1]) / scale])
            .collect()
    }

    #[test]
    fn mass_matches_numeric_integration() {
        let quad = [[0.0, 0.0], [1.3, 0.2], [1.1, 1.6], [-0.3, 1.2]];
        // Clockwise copy: the orientation must not matter.
        let quad_cw: Vec<[f64; 2]> = quad.iter().rev().copied().collect();
        let tri = [[-2.0, -1.0], [2.5, -0.5], [0.2, 2.2]];
        let mut half_disk = vec![[1.0, 0.0]];
        for k in 0..=8 {
            let a = std::f64::consts::PI * k as f64 / 8.0;
            half_disk.push([a.cos() * 1.5, a.sin() * 1.5 - 0.2]);
        }
        half_disk.remove(0);
        // (polygon, sample point, isotropic σ)
        type Case<'a> = (&'a [[f64; 2]], [f64; 2], f64);
        let cases: Vec<Case<'_>> = vec![
            (&quad, [0.4, 0.5], 0.7),
            (&quad, [1.6, 1.9], 0.9),
            (&quad, [0.0, 0.0], 0.3),
            (&quad_cw, [0.4, 0.5], 0.7),
            (&tri, [0.0, 0.0], 1.0),
            (&tri, [3.0, -0.5], 0.8),
            (&half_disk, [0.0, 0.0], 0.6),
            (&half_disk, [1.4, 0.1], 0.5),
        ];
        for (poly, p, s) in cases {
            let z = shifted(poly, p, s);
            let got = polygon_mass(&z, None);
            let want = brute_mass(&z);
            assert!((got - want).abs() < 1e-7, "p {p:?} s {s}: {got} vs {want}");
        }
    }

    #[test]
    fn gradient_matches_finite_differences() {
        let quad = [[0.0, 0.0], [1.3, 0.2], [1.1, 1.6], [-0.3, 1.2]];
        for (p, s) in [([0.4, 0.5], 0.7), ([1.5, 1.0], 0.4), ([-0.2, 1.3], 0.9)] {
            let z = shifted(&quad, p, s);
            let mut grad = vec![[0.0; 2]; z.len()];
            polygon_mass(&z, Some(&mut grad));
            // Wide enough that the table's 10⁻⁹ interpolation error stays
            // negligible against the step; the truncation error is O(eps²).
            let eps = 1e-3;
            for k in 0..z.len() {
                for c in 0..2 {
                    let mut plus = z.clone();
                    plus[k][c] += eps;
                    let mut minus = z.clone();
                    minus[k][c] -= eps;
                    let fd = (polygon_mass(&plus, None) - polygon_mass(&minus, None)) / (2.0 * eps);
                    assert!(
                        (grad[k][c] - fd).abs() < 1e-6 + 1e-5 * fd.abs(),
                        "vertex {k} axis {c}: {} vs {fd}",
                        grad[k][c]
                    );
                }
            }
        }
    }
}
