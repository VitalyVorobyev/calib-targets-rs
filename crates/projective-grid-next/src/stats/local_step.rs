//! Generic per-corner local grid-step estimation.
//!
//! For each input point this module returns an estimate of the spatial step
//! `|offset|` along the point's two local axes, plus a confidence score based
//! on how many neighbours contributed. Pattern-agnostic: chessboards,
//! ChArUco lattices, PuzzleBoards — any consumer with per-point two-axis
//! angles — can feed it.
//!
//! Float-generic over `F` (closes Gap 2 from `docs/algorithmic_gaps.md`).
//! The Phase 2 BFS engine consumes this as the *prediction fallback* when
//! the global homography isn't fit yet — that wiring closes Gap 5.
//!
//! Algorithm (per point):
//!
//! 1. Query up to `k_neighbors` nearest neighbours via a KD-tree.
//! 2. Drop neighbours farther than `max_step_factor × median(|offset|)`.
//! 3. Classify each surviving neighbour into the axis-u or axis-v sector,
//!    using the point's own two axes folded to undirected lines (mod π).
//!    Neighbours outside `sector_half_width_rad` of either axis are
//!    discarded.
//! 4. Per sector, run 1-D mean-shift on the collected `|offset|` values
//!    with bandwidth `bandwidth_rel × median(|offset|_sector)`. Fall back
//!    to the median on non-convergence.
//! 5. `confidence = min(1, supporters / confidence_denominator)`.

use kiddo::{KdTree, SquaredEuclidean};
use nalgebra::{Point2, RealField, Vector2};

use crate::feature::AxisEstimate;
use crate::float::{lit, Float};

/// Estimated per-point local grid-step along axis u and axis v.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct LocalStep<F: Float> {
    /// Estimated step length along axis u, in pixels. `0.0` when there were
    /// no supporters in this sector.
    pub step_u: F,
    /// Estimated step length along axis v.
    pub step_v: F,
    /// Confidence in `[0, 1]`: `min(1, supporters / confidence_denominator)`
    /// where supporters = (u-sector supporters + v-sector supporters).
    pub confidence: F,
    /// How many neighbours fed the u-sector mode (for diagnostics).
    pub supporters_u: usize,
    /// How many neighbours fed the v-sector mode.
    pub supporters_v: usize,
}

impl<F: Float> Default for LocalStep<F> {
    fn default() -> Self {
        Self {
            step_u: F::zero(),
            step_v: F::zero(),
            confidence: F::zero(),
            supporters_u: 0_usize,
            supporters_v: 0_usize,
        }
    }
}

/// Per-point data consumed by [`estimate_local_steps`].
///
/// `axes[0]` and `axes[1]` are the point's two local grid-axis directions.
/// The estimator uses only the `angle` field; `sigma` is carried through
/// but not consulted. Angles need not be orthogonal — the routine treats
/// them as undirected lines and folds every angle to `[0, π)` before
/// sector classification.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct LocalStepPointData<F: Float> {
    /// Point location in image pixels.
    pub position: Point2<F>,
    /// Two grid-axis hints. The `angle` field is used for sector binning;
    /// `sigma` is carried through but not inspected.
    pub axes: [AxisEstimate<F>; 2],
}

impl<F: Float> LocalStepPointData<F> {
    /// Bundle a position with two axis estimates.
    pub fn new(position: Point2<F>, axes: [AxisEstimate<F>; 2]) -> Self {
        Self { position, axes }
    }
}

/// Tuning knobs for [`estimate_local_steps`].
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct LocalStepParams<F: Float> {
    /// Nearest-neighbour count fed to the KD-tree per point. Defaults to 8.
    pub k_neighbors: usize,
    /// Clamp neighbours whose `|offset|` exceeds this factor times the
    /// local median distance. Defaults to `3.0`.
    pub max_step_factor: F,
    /// Half-width (radians) of the u and v sectors. Defaults to `π/6`
    /// (30°) so grid diagonals (45° on an orthogonal chessboard) are
    /// excluded from both sectors.
    pub sector_half_width_rad: F,
    /// Bandwidth for the 1-D mean-shift mode finder, as a fraction of the
    /// sector median. Defaults to `0.15`.
    pub bandwidth_rel: F,
    /// Maximum mean-shift iterations before falling back to the sector
    /// median. Defaults to `20`.
    pub mean_shift_max_iters: u32,
    /// Mean-shift converges when the update magnitude drops below
    /// `bandwidth × convergence_rel`. Defaults to `1e-3`.
    pub mean_shift_convergence_rel: F,
    /// Denominator used when converting supporter count to confidence.
    /// Defaults to `4.0` (an interior corner with 4 cardinal supporters
    /// hits confidence = 1.0).
    pub confidence_denominator: F,
}

impl<F: Float> Default for LocalStepParams<F> {
    fn default() -> Self {
        Self {
            k_neighbors: 8,
            max_step_factor: lit::<F>(3.0_f32),
            sector_half_width_rad: F::pi() / lit::<F>(6.0_f32),
            bandwidth_rel: lit::<F>(0.15_f32),
            mean_shift_max_iters: 20,
            mean_shift_convergence_rel: lit::<F>(1e-3_f32),
            confidence_denominator: lit::<F>(4.0_f32),
        }
    }
}

/// Compute a per-point local grid step along each point's two local axes.
///
/// Returns a vector whose length matches `points`. Points that end up with
/// no usable neighbours receive [`LocalStep::default`] (zero step + zero
/// confidence), letting downstream validators fall back to a global step.
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "debug",
        skip_all,
        fields(num_points = points.len()),
    )
)]
pub fn estimate_local_steps<F: Float + kiddo::float::kdtree::Axis>(
    points: &[LocalStepPointData<F>],
    params: &LocalStepParams<F>,
) -> Vec<LocalStep<F>> {
    if points.is_empty() {
        return Vec::new();
    }
    let coords: Vec<[F; 2]> = points
        .iter()
        .map(|p| [p.position.x, p.position.y])
        .collect();
    let tree: KdTree<F, 2> = (&coords).into();
    let mut out = Vec::with_capacity(points.len());
    for (i, p) in points.iter().enumerate() {
        out.push(estimate_one(i, p, &tree, points, params));
    }
    out
}

fn estimate_one<F: Float + kiddo::float::kdtree::Axis>(
    source_index: usize,
    source: &LocalStepPointData<F>,
    tree: &KdTree<F, 2>,
    points: &[LocalStepPointData<F>],
    params: &LocalStepParams<F>,
) -> LocalStep<F> {
    let k = params.k_neighbors.saturating_add(1).max(2);
    let results = tree.nearest_n::<SquaredEuclidean>(&[source.position.x, source.position.y], k);

    let mut offsets: Vec<Vector2<F>> = Vec::with_capacity(k);
    for nn in results {
        let j = nn.item as usize;
        if j == source_index {
            continue;
        }
        let other = &points[j];
        let offset = other.position - source.position;
        if offset.norm_squared().is_zero() {
            continue;
        }
        offsets.push(offset);
    }
    if offsets.is_empty() {
        return LocalStep::default();
    }

    let distances: Vec<F> = offsets.iter().map(|o| o.norm()).collect();
    let median_dist = median_f(&mut distances.clone());
    let cutoff = median_dist * params.max_step_factor;
    let mut kept: Vec<Vector2<F>> = offsets
        .into_iter()
        .zip(distances.iter())
        .filter_map(|(o, d)| if *d <= cutoff { Some(o) } else { None })
        .collect();
    if kept.is_empty() {
        return LocalStep::default();
    }

    let line_u = fold_to_line(source.axes[0].angle);
    let line_v = fold_to_line(source.axes[1].angle);
    let mut u_steps: Vec<F> = Vec::new();
    let mut v_steps: Vec<F> = Vec::new();

    while let Some(offset) = kept.pop() {
        let edge_line = fold_to_line(offset.y.atan2(offset.x));
        let diff_u = line_diff(edge_line, line_u);
        let diff_v = line_diff(edge_line, line_v);
        if RealField::min(diff_u, diff_v) > params.sector_half_width_rad {
            continue;
        }
        let d = offset.norm();
        if diff_u <= diff_v {
            u_steps.push(d);
        } else {
            v_steps.push(d);
        }
    }

    let (step_u, sup_u) = sector_mode(&mut u_steps, params);
    let (step_v, sup_v) = sector_mode(&mut v_steps, params);

    let total_sup = lit::<F>((sup_u + sup_v) as f32);
    let confidence = RealField::max(
        RealField::min(total_sup / params.confidence_denominator, F::one()),
        F::zero(),
    );

    LocalStep {
        step_u,
        step_v,
        confidence,
        supporters_u: sup_u as usize,
        supporters_v: sup_v as usize,
    }
}

/// 1-D mode via mean-shift on the collected `|offset|` samples. Returns
/// `(mode_value, supporter_count)`; `(0, 0)` when the sector is empty.
fn sector_mode<F: Float>(values: &mut [F], params: &LocalStepParams<F>) -> (F, u32) {
    if values.is_empty() {
        return (F::zero(), 0);
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let med = median_sorted(values);
    let sup = values.len() as u32;
    if values.len() < 2 {
        return (med, sup);
    }
    let bandwidth = med * params.bandwidth_rel;
    if bandwidth.is_zero() {
        return (med, sup);
    }
    let mut center = med;
    let convergence = bandwidth * params.mean_shift_convergence_rel;
    for _ in 0..params.mean_shift_max_iters {
        let mut sum = F::zero();
        let mut weight = F::zero();
        for &v in values.iter() {
            let diff = v - center;
            if diff.abs() > bandwidth {
                continue;
            }
            let t = diff / bandwidth;
            let w = F::one() - t * t;
            let w = if w < F::zero() { F::zero() } else { w };
            sum += v * w;
            weight += w;
        }
        if weight.is_zero() {
            return (med, sup);
        }
        let next = sum / weight;
        if (next - center).abs() <= convergence {
            return (next, sup);
        }
        center = next;
    }
    (med, sup)
}

/// Fold an angle into the undirected-line range `[0, π)`.
#[inline]
fn fold_to_line<F: Float>(theta: F) -> F {
    let pi = F::pi();
    let two_pi = pi + pi;
    let mut t = theta - two_pi * (theta / two_pi).floor();
    if t >= pi {
        t -= pi;
    }
    if t < F::zero() {
        t += pi;
    }
    t
}

/// Absolute angular difference between two undirected lines (both in
/// `[0, π)`). Result in `[0, π/2]`.
#[inline]
fn line_diff<F: Float>(a: F, b: F) -> F {
    let pi = F::pi();
    let frac_pi_2 = F::frac_pi_2();
    let mut diff = (a - b).abs();
    if diff > frac_pi_2 {
        diff = pi - diff;
    }
    diff
}

fn median_f<F: Float>(values: &mut [F]) -> F {
    if values.is_empty() {
        return F::zero();
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    median_sorted(values)
}

fn median_sorted<F: Float>(sorted: &[F]) -> F {
    let n = sorted.len();
    if n == 0 {
        return F::zero();
    }
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) * lit::<F>(0.5_f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::float::abs;

    fn lspd<F: Float>(x: F, y: F, axis_u: F) -> LocalStepPointData<F> {
        LocalStepPointData::new(
            Point2::new(x, y),
            [
                AxisEstimate::from_angle(axis_u),
                AxisEstimate::from_angle(axis_u + F::frac_pi_2()),
            ],
        )
    }

    fn regular_grid<F: Float>(
        rows: u32,
        cols: u32,
        spacing: F,
        angle: F,
    ) -> Vec<LocalStepPointData<F>> {
        let cx = angle.cos();
        let sx = angle.sin();
        let mut out = Vec::new();
        for j in 0..rows {
            for i in 0..cols {
                let i_f = lit::<F>(i as f32) * spacing;
                let j_f = lit::<F>(j as f32) * spacing;
                let x = i_f * cx - j_f * sx;
                let y = i_f * sx + j_f * cx;
                out.push(lspd(x, y, angle));
            }
        }
        out
    }

    fn assert_recovers_spacing_at_multiple_scales<F: Float + kiddo::float::kdtree::Axis>() {
        let params = LocalStepParams::<F>::default();
        for spacing_f32 in [10.0_f32, 20.0, 40.0] {
            let spacing = lit::<F>(spacing_f32);
            let pts = regular_grid::<F>(5, 5, spacing, F::zero());
            let steps = estimate_local_steps(&pts, &params);
            let s = &steps[12];
            let rel_u = abs::<F>(s.step_u - spacing) / spacing;
            let rel_v = abs::<F>(s.step_v - spacing) / spacing;
            assert!(rel_u < lit::<F>(0.05_f32));
            assert!(rel_v < lit::<F>(0.05_f32));
            assert!(s.supporters_u >= 2 && s.supporters_v >= 2);
            assert!(s.confidence > lit::<F>(0.8_f32));
        }
    }

    fn assert_rotated_grid_invariant<F: Float + kiddo::float::kdtree::Axis>() {
        let params = LocalStepParams::<F>::default();
        let spacing = lit::<F>(20.0_f32);
        for deg in [0.0_f32, 15.0, 30.0, 45.0] {
            let angle = lit::<F>(deg) * F::pi() / lit::<F>(180.0_f32);
            let pts = regular_grid::<F>(5, 5, spacing, angle);
            let steps = estimate_local_steps(&pts, &params);
            let s = &steps[12];
            assert!(abs::<F>(s.step_u - spacing) < lit::<F>(1.0_f32));
            assert!(abs::<F>(s.step_v - spacing) < lit::<F>(1.0_f32));
        }
    }

    fn assert_mild_barrel_distortion_tolerated<F: Float + kiddo::float::kdtree::Axis>() {
        let spacing = lit::<F>(25.0_f32);
        let mut pts = regular_grid::<F>(7, 7, spacing, F::zero());
        let cx = lit::<F>(3.0_f32) * spacing;
        let cy = lit::<F>(3.0_f32) * spacing;
        let radial_coeff = lit::<F>(1e-5_f32);
        for p in &mut pts {
            let dx = p.position.x - cx;
            let dy = p.position.y - cy;
            let r2 = dx * dx + dy * dy;
            let scale = F::one() + radial_coeff * r2;
            p.position = Point2::new(cx + dx * scale, cy + dy * scale);
        }
        let steps = estimate_local_steps::<F>(&pts, &LocalStepParams::default());
        let interior = 24usize;
        let s = &steps[interior];
        let rel = abs::<F>(s.step_u - spacing) / spacing;
        assert!(rel < lit::<F>(0.1_f32));
    }

    fn assert_dual_scale_picks_dominant_mode<F: Float + kiddo::float::kdtree::Axis>() {
        let spacing = lit::<F>(20.0_f32);
        let mut pts = regular_grid::<F>(5, 5, spacing, F::zero());
        let marker_angle = lit::<F>(20.0_f32) * F::pi() / lit::<F>(180.0_f32);
        let interior_pts: Vec<usize> = (1..4)
            .flat_map(|j| (1..4).map(move |i| j * 5 + i))
            .collect();
        let off = lit::<F>(3.0_f32);
        for &idx in &interior_pts {
            let c = pts[idx].position;
            pts.push(LocalStepPointData::new(
                Point2::new(c.x + off, c.y + off),
                [
                    AxisEstimate::from_angle(marker_angle),
                    AxisEstimate::from_angle(marker_angle + F::frac_pi_2()),
                ],
            ));
        }
        let steps = estimate_local_steps::<F>(&pts, &LocalStepParams::default());
        let s = &steps[12];
        assert!(abs::<F>(s.step_u - spacing) < lit::<F>(2.0_f32));
        assert!(abs::<F>(s.step_v - spacing) < lit::<F>(2.0_f32));
    }

    fn assert_isolated_point_zero_confidence<F: Float + kiddo::float::kdtree::Axis>() {
        let pts = vec![lspd::<F>(F::zero(), F::zero(), F::zero())];
        let steps = estimate_local_steps::<F>(&pts, &LocalStepParams::default());
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].confidence, F::zero());
        assert_eq!(steps[0].step_u, F::zero());
        assert_eq!(steps[0].step_v, F::zero());
    }

    fn assert_fold_and_line_diff<F: Float>() {
        let pi = F::pi();
        for theta in [
            -pi,
            lit::<F>(-0.5_f32),
            F::zero(),
            lit::<F>(0.5_f32),
            pi - lit::<F>(1e-3_f32),
            pi,
            lit::<F>(1.5_f32) * pi,
            lit::<F>(2.5_f32) * pi,
        ] {
            let folded = fold_to_line(theta);
            assert!(folded >= F::zero() && folded < pi);
        }
        let frac_pi_2 = F::frac_pi_2();
        assert!(abs::<F>(line_diff(F::zero(), frac_pi_2) - frac_pi_2) < lit::<F>(1e-5_f32));
        assert!(line_diff(F::zero(), pi - lit::<F>(1e-3_f32)) < lit::<F>(1e-2_f32));
    }

    #[test]
    fn recovers_spacing_f32() {
        assert_recovers_spacing_at_multiple_scales::<f32>();
    }
    #[test]
    fn recovers_spacing_f64() {
        assert_recovers_spacing_at_multiple_scales::<f64>();
    }
    #[test]
    fn rotated_invariant_f32() {
        assert_rotated_grid_invariant::<f32>();
    }
    #[test]
    fn rotated_invariant_f64() {
        assert_rotated_grid_invariant::<f64>();
    }
    #[test]
    fn barrel_distortion_f32() {
        assert_mild_barrel_distortion_tolerated::<f32>();
    }
    #[test]
    fn barrel_distortion_f64() {
        assert_mild_barrel_distortion_tolerated::<f64>();
    }
    #[test]
    fn dual_scale_f32() {
        assert_dual_scale_picks_dominant_mode::<f32>();
    }
    #[test]
    fn dual_scale_f64() {
        assert_dual_scale_picks_dominant_mode::<f64>();
    }
    #[test]
    fn isolated_zero_confidence_f32() {
        assert_isolated_point_zero_confidence::<f32>();
    }
    #[test]
    fn isolated_zero_confidence_f64() {
        assert_isolated_point_zero_confidence::<f64>();
    }
    #[test]
    fn fold_line_diff_f32() {
        assert_fold_and_line_diff::<f32>();
    }
    #[test]
    fn fold_line_diff_f64() {
        assert_fold_and_line_diff::<f64>();
    }
}
