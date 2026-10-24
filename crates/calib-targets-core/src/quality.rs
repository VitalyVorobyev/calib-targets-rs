//! Per-frame detection quality: what the delivered corners' covariances say,
//! and how well they describe the corners' local scatter.

use std::collections::HashMap;

use projective_grid::Coord;
use serde::{Deserialize, Serialize};

use crate::{LabeledCorner, PositionCovariance};

/// The eight grid neighbours of a corner, as `(du, dv)` offsets.
const RING: [(i32, i32); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

/// A corner's neighbourhood must hold at least this many of its eight grid
/// neighbours for the leave-one-out prediction. Any six of the ring surround
/// the corner (only a whole missing side — three in a row — leaves five), so
/// the prediction is an interpolation, never an extrapolation, and the
/// 8-parameter fit has at least four redundant equations.
pub const MIN_LOCAL_NEIGHBOURS: usize = 6;

/// A summary of how well one detection's corners are localized, measured on
/// the corners it delivered.
///
/// Two kinds of number live here:
///
/// - **What the covariances say** — [`noise_sigma`](Self::noise_sigma),
///   [`covariance_coverage`](Self::covariance_coverage),
///   [`median_sigma_px`](Self::median_sigma_px),
///   [`p95_sigma_px`](Self::p95_sigma_px). Read straight off the corners'
///   [`PositionCovariance`]s.
/// - **How well they hold locally** —
///   [`local_residual_rms_px`](Self::local_residual_rms_px) and
///   [`variance_factor`](Self::variance_factor). Each interior corner is
///   predicted from its grid neighbours alone — the corner itself left out —
///   as the intersection of the image lines through its opposite neighbours
///   (row, column, both diagonals), which a local homography maps exactly;
///   the prediction residual is then compared against the covariance the
///   corner and the prediction carry.
///
/// # What the local check measures, and what it cannot
///
/// A position covariance describes **corner-local** error: noise propagated
/// through the localizer and its systematic error at that corner. Errors a
/// whole view shares — the lens model, the printed target, defocus — move
/// neighbouring corners together; a prediction from the 3 × 3 neighbourhood
/// absorbs them, so they are outside both the covariance and this check. That is the point: comparing
/// covariances with the residual of a global model fit would charge them with
/// the model's error, which is typically several times larger.
///
/// What stays in the residual is error that differs between a corner and its
/// neighbours: the localizer's own scatter, plus whatever lens distortion
/// bends a grid line over two cells (a few hundredths of a pixel at moderate
/// distortion — a floor on the residual for very precise corners).
/// The check presumes a planar target; a curved one (a PuzzlePole) reports
/// the covariance statistics only ([`DetectionQuality::from_covariances`]).
///
/// # Reading the variance factor
///
/// `variance_factor` (σ̂₀²) is the leave-one-out residual whitened by its own
/// covariance. **≈ 1** means the covariances describe the corners' local
/// scatter; **< 1** that they are conservative (they over-state the error);
/// **> 1** that they are overconfident, or that something the covariance does
/// not model is at work (e.g. illumination gradients, compression, a blurred
/// or partly occluded corner). On real images expect values above 1. Use it
/// to compare frames taken with the same setup, not as a pass/fail test —
/// nothing in the detector gates on it.
///
/// # Construction
///
/// Every detection result measures its own corners when it is built, so the
/// detectors fill this in. `#[non_exhaustive]`: build one with
/// [`DetectionQuality::from_corners`]; [`Default`] is the "nothing measured"
/// value.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DetectionQuality {
    /// The image noise level (gray levels, 1σ) the corner covariances were
    /// computed with: supplied as `chess.noise_sigma`, or estimated from the
    /// frame. `None` when the detector does not know it — no covariance was
    /// computed, the estimate declined, or the corners were passed in by the
    /// caller (`*_with_corners`, a chessboard detector run on a corner cloud).
    #[serde(default)]
    pub noise_sigma: Option<f32>,
    /// Fraction of the delivered corners that carry a position covariance,
    /// in `[0, 1]` (`0` for a detection with no corners).
    #[serde(default)]
    pub covariance_coverage: f32,
    /// Median of the corners' 1σ position uncertainty along its least certain
    /// direction ([`PositionCovariance::sigma_max_px`]), px. `None` when no
    /// corner carries a covariance.
    #[serde(default)]
    pub median_sigma_px: Option<f32>,
    /// 95th percentile (nearest rank) of the same per-corner σ, px.
    #[serde(default)]
    pub p95_sigma_px: Option<f32>,
    /// Number of corners with a complete enough neighbourhood for the
    /// leave-one-out prediction: a grid label and at least six of the eight
    /// grid neighbours delivered. `0` when the check was not run (a
    /// non-planar target).
    #[serde(default)]
    pub local_residual_count: usize,
    /// RMS length of the leave-one-out residual (observed minus predicted
    /// position) over those corners, px. It contains the prediction's own
    /// error as well as the corner's: with eight neighbours as precise as the
    /// corner, the prediction adds a quarter of the corner's variance, so the
    /// residual is about 12 % longer than the corner's error alone.
    /// `None` when no corner qualifies.
    #[serde(default)]
    pub local_residual_rms_px: Option<f32>,
    /// Number of corners in [`Self::variance_factor`]: those counted by
    /// [`Self::local_residual_count`] whose own covariance and whose
    /// neighbours' covariances are all present.
    #[serde(default)]
    pub variance_factor_count: usize,
    /// Robust variance factor σ̂₀² of the leave-one-out residuals.
    ///
    /// Each residual `r` is whitened with the covariance it should have,
    /// `C = Σ_corner + Σ_prediction`, where `Σ_prediction` propagates the
    /// neighbours' covariances to first order through the prediction:
    /// `z = rᵀ C⁻¹ r / 2`. Under the covariances' Gaussian model `z` is
    /// exponentially distributed with mean 1, so its median is `ln 2`; the
    /// reported factor is `median(z) / ln 2`. It equals the classical mean
    /// `Σ z / N` when the model holds, and a few gross corners cannot move
    /// it. See the type-level docs for how to read it. `None` when
    /// [`Self::variance_factor_count`] is zero.
    #[serde(default)]
    pub variance_factor: Option<f32>,
}

impl DetectionQuality {
    /// Measure a detection's corner set.
    ///
    /// Every corner counts towards the covariance statistics; only corners
    /// with a [`grid`](LabeledCorner::grid) label take part in the
    /// leave-one-out check, and a corner's neighbours are the corners whose
    /// labels differ by at most one in each coordinate. If two corners share a
    /// label, the later one is used. [`noise_sigma`](Self::noise_sigma) is left
    /// `None`; set it with [`Self::with_noise_sigma`].
    pub fn from_corners(corners: &[LabeledCorner]) -> Self {
        let local = local_check(corners);
        Self {
            local_residual_count: local.residual_count,
            local_residual_rms_px: local.residual_rms_px,
            variance_factor_count: local.whitened.len(),
            variance_factor: robust_variance_factor(local.whitened),
            ..Self::from_covariances(corners)
        }
    }

    /// The covariance statistics alone, without the leave-one-out check —
    /// for a target that is not planar.
    ///
    /// The local check predicts a corner from its 3 × 3 grid neighbourhood
    /// by collinearity, which presumes a flat target. On a curved one
    /// (a PuzzlePole) the surface's curvature dominates the residual: tens of
    /// times the corners' own scatter on a pole seen across a third of its
    /// circumference. Such detections report covariance statistics only; the
    /// local fields stay unset.
    pub fn from_covariances(corners: &[LabeledCorner]) -> Self {
        let sigmas: Vec<f32> = corners
            .iter()
            .filter_map(|c| c.position_covariance.map(|cov| cov.sigma_max_px()))
            .collect();
        let covariance_coverage = if corners.is_empty() {
            0.0
        } else {
            sigmas.len() as f32 / corners.len() as f32
        };
        let (median_sigma_px, p95_sigma_px) = sigma_quantiles(sigmas);
        Self {
            noise_sigma: None,
            covariance_coverage,
            median_sigma_px,
            p95_sigma_px,
            ..Self::default()
        }
    }

    /// Set [`noise_sigma`](Self::noise_sigma) (gray levels). Accepts an `f32`
    /// or an `Option` of one.
    #[must_use]
    pub fn with_noise_sigma(mut self, noise_sigma: impl Into<Option<f32>>) -> Self {
        self.noise_sigma = noise_sigma.into();
        self
    }
}

/// Median and 95th percentile (nearest rank) of `values`.
fn sigma_quantiles(mut values: Vec<f32>) -> (Option<f32>, Option<f32>) {
    if values.is_empty() {
        return (None, None);
    }
    values.sort_by(f32::total_cmp);
    (
        Some(median_sorted(&values)),
        Some(nearest_rank(&values, 0.95)),
    )
}

/// Median of an ascending slice (mean of the two middle values for an even
/// count). The slice must not be empty.
fn median_sorted(sorted: &[f32]) -> f32 {
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        0.5 * (sorted[n / 2 - 1] + sorted[n / 2])
    }
}

/// Nearest-rank `q`-quantile of an ascending, non-empty slice: the smallest
/// value with at least a fraction `q` of the values at or below it.
fn nearest_rank(sorted: &[f32], q: f32) -> f32 {
    let rank = (q * sorted.len() as f32).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

/// `median(z) / ln 2` — see [`DetectionQuality::variance_factor`].
fn robust_variance_factor(mut whitened: Vec<f32>) -> Option<f32> {
    if whitened.is_empty() {
        return None;
    }
    whitened.sort_by(f32::total_cmp);
    Some(median_sorted(&whitened) / std::f32::consts::LN_2)
}

/// What the leave-one-out pass over a corner set produced.
struct LocalCheck {
    residual_count: usize,
    residual_rms_px: Option<f32>,
    /// `z = rᵀ C⁻¹ r / 2` per corner that has every covariance it needs.
    whitened: Vec<f32>,
}

/// Label → corner-index lookup: a dense array over the labels' bounding box
/// (the usual case — a detection's labels are contiguous), or a hash map when
/// that box would be mostly empty. A label shared by two corners resolves to
/// the later one.
enum GridIndex {
    Dense {
        u0: i32,
        v0: i32,
        width: i32,
        height: i32,
        cells: Vec<u32>,
    },
    Sparse(HashMap<Coord, u32>),
}

const EMPTY: u32 = u32::MAX;

impl GridIndex {
    fn new(corners: &[LabeledCorner]) -> Self {
        let labelled = || {
            corners
                .iter()
                .enumerate()
                .filter_map(|(i, c)| Some((c.grid?, i as u32)))
        };
        let (mut lo, mut hi) = ((i32::MAX, i32::MAX), (i32::MIN, i32::MIN));
        let mut n = 0usize;
        for (g, _) in labelled() {
            lo = (lo.0.min(g.u), lo.1.min(g.v));
            hi = (hi.0.max(g.u), hi.1.max(g.v));
            n += 1;
        }
        if n == 0 {
            return Self::Sparse(HashMap::new());
        }
        let width = i64::from(hi.0) - i64::from(lo.0) + 1;
        let height = i64::from(hi.1) - i64::from(lo.1) + 1;
        let area = width * height;
        if area > 4 * n as i64 + 1024 {
            return Self::Sparse(labelled().collect());
        }
        let (width, height) = (width as i32, height as i32);
        let mut cells = vec![EMPTY; area as usize];
        for (g, i) in labelled() {
            cells[((g.v - lo.1) * width + (g.u - lo.0)) as usize] = i;
        }
        Self::Dense {
            u0: lo.0,
            v0: lo.1,
            width,
            height,
            cells,
        }
    }

    fn get(&self, u: i32, v: i32) -> Option<usize> {
        match self {
            Self::Dense {
                u0,
                v0,
                width,
                height,
                cells,
            } => {
                let (du, dv) = (u - u0, v - v0);
                if du < 0 || dv < 0 || du >= *width || dv >= *height {
                    return None;
                }
                let i = cells[(dv * width + du) as usize];
                (i != EMPTY).then_some(i as usize)
            }
            Self::Sparse(map) => map.get(&Coord::new(u, v)).map(|&i| i as usize),
        }
    }
}

fn local_check(corners: &[LabeledCorner]) -> LocalCheck {
    let index = GridIndex::new(corners);
    let mut residual_count = 0usize;
    let mut sum_sq = 0.0f64;
    let mut whitened = Vec::new();
    // Iterate the slice, so the result does not depend on any map order; a
    // label shared by two corners is checked once, for the corner the index
    // resolves it to.
    for (idx, corner) in corners.iter().enumerate() {
        let Some(grid) = corner.grid else { continue };
        if index.get(grid.u, grid.v) != Some(idx) {
            continue;
        }
        let mut ring = [None; 8];
        let mut present = 0usize;
        for (slot, &(du, dv)) in ring.iter_mut().zip(&RING) {
            if let Some(n) = index.get(grid.u + du, grid.v + dv) {
                *slot = Some(&corners[n]);
                present += 1;
            }
        }
        if present < MIN_LOCAL_NEIGHBOURS {
            continue;
        }
        let Some(prediction) = predict_from_ring(&ring) else {
            continue;
        };
        let rx = f64::from(corner.position.x) - prediction.position[0];
        let ry = f64::from(corner.position.y) - prediction.position[1];
        residual_count += 1;
        sum_sq += rx * rx + ry * ry;

        if let (Some(own), Some(pred)) = (corner.position_covariance, prediction.covariance) {
            let c = [
                f64::from(own.xx) + pred[0],
                f64::from(own.xy) + pred[1],
                f64::from(own.yy) + pred[2],
            ];
            if let Some(d2) = mahalanobis_sq(c, rx, ry) {
                whitened.push((0.5 * d2) as f32);
            }
        }
    }
    LocalCheck {
        residual_count,
        residual_rms_px: (residual_count > 0)
            .then(|| (sum_sq / residual_count as f64).sqrt() as f32),
        whitened,
    }
}

/// `dᵀ C⁻¹ d` for `C = [[xx, xy], [xy, yy]]`, or `None` when `C` is not
/// positive definite.
fn mahalanobis_sq([xx, xy, yy]: [f64; 3], dx: f64, dy: f64) -> Option<f64> {
    let det = xx * yy - xy * xy;
    if !(xx > 0.0 && det > 0.0 && det.is_finite()) {
        return None;
    }
    Some((yy * dx * dx - 2.0 * xy * dx * dy + xx * dy * dy) / det)
}

/// A corner's position predicted from its neighbours, with the covariance the
/// prediction inherits from theirs.
struct Prediction {
    /// Predicted image position, px.
    position: [f64; 2],
    /// `[xx, xy, yy]` of the prediction's covariance, px²; `None` when a
    /// neighbour has no covariance.
    covariance: Option<[f64; 3]>,
}

/// Predict a corner from its ring of grid neighbours (slots in [`RING`]
/// order, so slot `7 − k` is the neighbour opposite slot `k`).
///
/// A homography preserves collinearity, so the corner lies on the image line
/// through every pair of opposite neighbours — its row, its column and both
/// diagonals. The prediction is the least-squares intersection of those lines
/// (perpendicular distances): exact under any perspective, as a local
/// homography fit would be. Any six of the eight neighbours keep at least two
/// opposite pairs, two distinct directions, so the intersection is always
/// defined. An explicit 8-parameter homography fit predicts the same point on
/// exact data at about four times the cost (measured), for little precision:
/// with a full ring of equally precise neighbours its prediction variance is
/// 5/22 of a corner's, the intersection's 1/4.
///
/// The covariance is first order. With line `l` through `a` and `b`, unit
/// normal `n`, and the prediction a fraction `t` of the way from `a` to `b`,
/// the line moves at the prediction by `nᵀ((1 − t) δa + t δb)`, so
/// `δp̂ = A⁻¹ Σₗ n nᵀ ((1 − t) δa + t δb)` with `A = Σₗ n nᵀ`, and each point
/// contributes `s² (nᵀ Σ n) (A⁻¹ n)(A⁻¹ n)ᵀ` (`s` = `1 − t` or `t`).
fn predict_from_ring(ring: &[Option<&LabeledCorner>; 8]) -> Option<Prediction> {
    let pos = |c: &LabeledCorner| (f64::from(c.position.x), f64::from(c.position.y));
    // Up to four lines, one per opposite pair.
    let mut lines: [Option<LineFit>; 4] = [None; 4];
    let (mut a00, mut a01, mut a11, mut r0, mut r1) = (0.0f64, 0.0, 0.0, 0.0, 0.0);
    for (k, slot) in lines.iter_mut().enumerate() {
        let (Some(ca), Some(cb)) = (ring[k], ring[7 - k]) else {
            continue;
        };
        let (a, b) = (pos(ca), pos(cb));
        let (ex, ey) = (b.0 - a.0, b.1 - a.1);
        let len = ex.hypot(ey);
        if !(len.is_finite() && len > 0.0) {
            return None;
        }
        let n = (-ey / len, ex / len);
        let c = n.0 * a.0 + n.1 * a.1;
        a00 += n.0 * n.0;
        a01 += n.0 * n.1;
        a11 += n.1 * n.1;
        r0 += n.0 * c;
        r1 += n.1 * c;
        *slot = Some(LineFit {
            a,
            dir: (ex, ey),
            len2: len * len,
            n,
            cov: [ca.position_covariance, cb.position_covariance],
        });
    }
    let det = a00 * a11 - a01 * a01;
    // Two distinct directions through one point: det ≥ sin²(angle) ≫ 0.
    if !(det.is_finite() && det > 1e-6 * (a00 + a11).powi(2)) {
        return None;
    }
    let inv = [a11 / det, -a01 / det, a00 / det]; // A⁻¹ = [[0, 1], [1, 2]]
    let x = inv[0] * r0 + inv[1] * r1;
    let y = inv[1] * r0 + inv[2] * r1;
    if !(x.is_finite() && y.is_finite()) {
        return None;
    }

    let covariance = (|| {
        let mut out = [0.0f64; 3];
        for line in lines.iter().flatten() {
            let t = ((x - line.a.0) * line.dir.0 + (y - line.a.1) * line.dir.1) / line.len2;
            let g = (
                inv[0] * line.n.0 + inv[1] * line.n.1,
                inv[1] * line.n.0 + inv[2] * line.n.1,
            );
            for (s, cov) in [(1.0 - t, line.cov[0]), (t, line.cov[1])] {
                let cov = cov?;
                let (n0, n1) = line.n;
                let along = n0 * n0 * f64::from(cov.xx)
                    + 2.0 * n0 * n1 * f64::from(cov.xy)
                    + n1 * n1 * f64::from(cov.yy);
                let w = s * s * along;
                out[0] += w * g.0 * g.0;
                out[1] += w * g.0 * g.1;
                out[2] += w * g.1 * g.1;
            }
        }
        out.iter().all(|v| v.is_finite()).then_some(out)
    })();
    Some(Prediction {
        position: [x, y],
        covariance,
    })
}

/// One line through a pair of opposite neighbours.
#[derive(Clone, Copy)]
struct LineFit {
    a: (f64, f64),
    dir: (f64, f64),
    len2: f64,
    n: (f64, f64),
    cov: [Option<PositionCovariance>; 2],
}

#[cfg(test)]
mod tests {
    use super::*;
    use nalgebra::Point2;

    /// A tiny deterministic Gaussian source (Box–Muller over a 64-bit LCG),
    /// so the tests need no RNG dependency.
    struct Gauss(u64);
    impl Gauss {
        fn uniform(&mut self) -> f64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((self.0 >> 11) as f64 + 0.5) / (1u64 << 53) as f64
        }
        fn next(&mut self) -> f64 {
            let (u1, u2) = (self.uniform(), self.uniform());
            (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
        }
    }

    /// A `cols × rows` grid seen through a mild perspective, each corner
    /// displaced by Gaussian noise of `sigma` px per axis and carrying the
    /// covariance `claimed² · I`.
    fn noisy_grid(cols: i32, rows: i32, sigma: f64, claimed: f32, seed: u64) -> Vec<LabeledCorner> {
        let mut rng = Gauss(seed);
        let mut out = Vec::new();
        for v in 0..rows {
            for u in 0..cols {
                let (gu, gv) = (f64::from(u), f64::from(v));
                let w = 1.0 + 0.004 * gu - 0.003 * gv;
                let x = (40.0 + 21.0 * gu + 3.0 * gv) / w + sigma * rng.next();
                let y = (30.0 - 2.0 * gu + 19.0 * gv) / w + sigma * rng.next();
                let var = claimed * claimed;
                out.push(
                    LabeledCorner::new(Point2::new(x as f32, y as f32), 1.0)
                        .with_grid(Coord::new(u, v))
                        .with_position_covariance(PositionCovariance::new(var, 0.0, var)),
                );
            }
        }
        out
    }

    #[test]
    fn exact_projective_grid_has_no_residual() {
        let corners = noisy_grid(9, 7, 0.0, 0.05, 1);
        let q = DetectionQuality::from_corners(&corners);
        // Interior corners: 7 × 5. Edge corners have five neighbours.
        assert_eq!(q.local_residual_count, 35);
        assert_eq!(q.variance_factor_count, 35);
        assert!(q.local_residual_rms_px.unwrap() < 1e-3);
        assert!(q.variance_factor.unwrap() < 1e-3);
        assert_eq!(q.covariance_coverage, 1.0);
        assert!((q.median_sigma_px.unwrap() - 0.05).abs() < 1e-6);
        assert!((q.p95_sigma_px.unwrap() - 0.05).abs() < 1e-6);
        assert_eq!(q.noise_sigma, None);
    }

    /// With honest covariances the factor is ≈ 1; it scales with the square
    /// of how much the covariances under-state the scatter.
    #[test]
    fn variance_factor_tracks_the_true_scatter() {
        let mut honest = Vec::new();
        let mut half = Vec::new();
        for seed in 0..20 {
            let corners = noisy_grid(14, 12, 0.08, 0.08, seed);
            let q = DetectionQuality::from_corners(&corners);
            honest.push(q.variance_factor.unwrap());
            let corners = noisy_grid(14, 12, 0.08, 0.04, seed);
            half.push(
                DetectionQuality::from_corners(&corners)
                    .variance_factor
                    .unwrap(),
            );
        }
        let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len() as f32;
        let (h, o) = (mean(&honest), mean(&half));
        assert!((h - 1.0).abs() < 0.1, "honest covariances: σ̂₀² = {h}");
        assert!((o / h - 4.0).abs() < 0.2, "halved σ: ratio {}", o / h);
    }

    /// The prediction's own error is part of the residual: with a full ring of
    /// equally precise neighbours it adds a quarter of the corner's variance
    /// (four lines, each through two points at its midpoint).
    #[test]
    fn residual_rms_includes_the_prediction_error() {
        let mut rms = Vec::new();
        for seed in 0..20 {
            let corners = noisy_grid(14, 12, 0.1, 0.1, seed);
            rms.push(
                DetectionQuality::from_corners(&corners)
                    .local_residual_rms_px
                    .unwrap(),
            );
        }
        let mean = rms.iter().sum::<f32>() / rms.len() as f32;
        // Per axis 0.1 px, two axes: |e| RMS = 0.141, times √(1 + 1/4).
        let expected = 0.1 * 2f32.sqrt() * (1.0f32 + 0.25).sqrt();
        assert!(
            (mean / expected - 1.0).abs() < 0.04,
            "rms {mean} vs {expected}"
        );
    }

    #[test]
    fn missing_covariances_and_labels() {
        let mut corners = noisy_grid(5, 5, 0.0, 0.1, 3);
        // Strip the centre's covariance: it still has a residual, but no
        // whitened term; its neighbours lose theirs only if they use it.
        corners[12].position_covariance = None;
        // An unlabelled corner counts toward coverage only.
        corners.push(LabeledCorner::new(Point2::new(0.0, 0.0), 1.0));
        let q = DetectionQuality::from_corners(&corners);
        assert_eq!(q.local_residual_count, 9);
        // Every interior corner of a 5 × 5 grid neighbours the centre.
        assert_eq!(q.variance_factor_count, 0);
        assert_eq!(q.variance_factor, None);
        assert!((q.covariance_coverage - 24.0 / 26.0).abs() < 1e-6);
    }

    #[test]
    fn covariances_only_skips_the_local_check() {
        let corners = noisy_grid(6, 6, 0.05, 0.05, 2);
        let full = DetectionQuality::from_corners(&corners);
        let cov = DetectionQuality::from_covariances(&corners);
        assert!(full.local_residual_count > 0);
        assert_eq!(cov.local_residual_count, 0);
        assert_eq!(cov.local_residual_rms_px, None);
        assert_eq!(cov.variance_factor, None);
        assert_eq!(cov.covariance_coverage, full.covariance_coverage);
        assert_eq!(cov.median_sigma_px, full.median_sigma_px);
        assert_eq!(cov.p95_sigma_px, full.p95_sigma_px);
    }

    /// Two copies of a grid far apart in label space take the sparse index
    /// and measure exactly what each copy does alone.
    #[test]
    fn sparse_labels_match_dense() {
        let one = noisy_grid(7, 6, 0.05, 0.05, 4);
        let mut two = one.clone();
        for c in &one {
            let g = c.grid.unwrap();
            two.push(c.clone().with_grid(Coord::new(g.u + 100_000, g.v - 7)));
        }
        let q1 = DetectionQuality::from_corners(&one);
        let q2 = DetectionQuality::from_corners(&two);
        assert!(matches!(GridIndex::new(&two), GridIndex::Sparse(_)));
        assert_eq!(q2.local_residual_count, 2 * q1.local_residual_count);
        assert_eq!(q2.local_residual_rms_px, q1.local_residual_rms_px);
        assert_eq!(q2.variance_factor, q1.variance_factor);
    }

    #[test]
    fn empty_and_tiny_sets() {
        let q = DetectionQuality::from_corners(&[]);
        assert_eq!(q, DetectionQuality::default());
        let q = DetectionQuality::from_corners(&noisy_grid(2, 2, 0.0, 0.1, 0));
        assert_eq!(q.local_residual_count, 0);
        assert_eq!(q.local_residual_rms_px, None);
        assert_eq!(q.median_sigma_px, Some(0.1));
    }

    #[test]
    fn quantiles() {
        let v: Vec<f32> = (1..=20).map(|k| k as f32).collect();
        assert_eq!(median_sorted(&v), 10.5);
        assert_eq!(nearest_rank(&v, 0.95), 19.0);
        assert_eq!(nearest_rank(&[3.0], 0.95), 3.0);
    }

    #[test]
    fn serde_round_trip_and_defaults() {
        let q =
            DetectionQuality::from_corners(&noisy_grid(6, 6, 0.05, 0.05, 9)).with_noise_sigma(1.5);
        let v = serde_json::to_value(q).unwrap();
        assert_eq!(v["noise_sigma"], serde_json::json!(1.5));
        let back: DetectionQuality = serde_json::from_value(v).unwrap();
        assert_eq!(back, q);
        let empty: DetectionQuality = serde_json::from_str("{}").unwrap();
        assert_eq!(empty, DetectionQuality::default());
    }
}
