//! Generic per-corner local grid-step estimation.
//!
//! For each input point this module returns an estimate of the spatial step
//! `|offset|` along the point's two local axes, plus a confidence score based
//! on how many neighbors contributed to the estimate. It is pattern-agnostic;
//! chessboards, ChArUco lattices, PuzzleBoards — any consumer with per-point
//! two-axis angles — can feed it.
//!
//! Algorithm (per point):
//! 1. Query up to `k_neighbors` nearest neighbors via a KD-tree.
//! 2. Drop neighbors farther than `max_step_factor × median(|offset|)` — a
//!    coarse outlier reject that avoids bleed-through from distant marker
//!    cells or second-order lattice copies.
//! 3. Classify each surviving neighbor into the axis-u or axis-v sector,
//!    using the point's own `(axis_u, axis_v)` folded to undirected lines
//!    (mod π). Neighbors outside `sector_half_width_rad` of either axis are
//!    discarded as ambiguous.
//! 4. Per sector, run 1-D mean-shift on the collected `|offset|` values with
//!    bandwidth `bandwidth_rel × median(|offset|_sector)` to recover the
//!    dominant step. Fall back to the median when mean-shift fails to
//!    converge in a small fixed number of iterations.
//! 5. Confidence = `min(1, supporters / confidence_denominator)`.
//!
//! Dual-scale awareness (ChArUco marker-internal corners sit at ~0.2× the
//! board step). Because sector binning uses each point's own axes — which
//! typically deviate from the marker's rotated axes — marker-internal
//! neighbors fall outside the sector and never reach the step-mode stage.
//! Any that do reach it are a minority per neighborhood, so the dominant
//! mode corresponds to the board scale.
//!
//! See `docs/grid_plan.md` Phase 2 and the plan file stored under
//! `.claude/plans/we-need-to-plan-breezy-pixel.md` for the full context.

use crate::Float;
use kiddo::{KdTree, SquaredEuclidean};
use nalgebra::{Point2, RealField, Vector2};

/// Estimated per-point local grid-step along axis u and axis v.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalStep<F: Float = f32> {
    /// Estimated step length along axis u, in pixels. `0.0` when there were no
    /// supporters in this sector.
    pub step_u: F,
    /// Estimated step length along axis v.
    pub step_v: F,
    /// Confidence in `[0, 1]`: `min(1, supporters / confidence_denominator)`
    /// where supporters = (u-sector supporters + v-sector supporters).
    pub confidence: F,
    /// How many neighbors fed the u-sector mode (for diagnostics).
    pub supporters_u: u32,
    /// How many neighbors fed the v-sector mode.
    pub supporters_v: u32,
}

impl<F: Float> Default for LocalStep<F> {
    fn default() -> Self {
        Self {
            step_u: F::zero(),
            step_v: F::zero(),
            confidence: F::zero(),
            supporters_u: 0,
            supporters_v: 0,
        }
    }
}

/// Per-point data consumed by [`estimate_local_steps`].
///
/// `axis_u` and `axis_v` are the point's two local grid-axis directions in
/// radians. They need not be orthogonal — the routine treats them as
/// undirected lines and folds every angle to `[0, π)` before sector
/// classification, so perspective-warped corners whose axes deviate from 90°
/// are handled naturally.
#[derive(Clone, Copy, Debug)]
pub struct LocalStepPointData<F: Float = f32> {
    pub position: Point2<F>,
    pub axis_u: F,
    pub axis_v: F,
}

/// Tuning knobs for [`estimate_local_steps`].
#[derive(Clone, Copy, Debug)]
pub struct LocalStepParams<F: Float = f32> {
    /// Nearest-neighbor count fed to the KD-tree per point. Defaults to 8 —
    /// enough for a 4-connected grid even when a handful of neighbors are
    /// missing.
    pub k_neighbors: usize,
    /// Clamp neighbors whose `|offset|` exceeds this factor times the local
    /// median distance. Defaults to `3.0`.
    pub max_step_factor: F,
    /// Half-width (radians) of the u and v sectors. Defaults to `π/6` (30°)
    /// so that grid diagonals — which sit exactly at 45° on an orthogonal
    /// chessboard — are excluded from both sectors rather than polluting one
    /// of them. Widen this if the detector emits heavily-warped grids whose
    /// on-axis neighbors rotate more than 30° away from the lattice axes.
    pub sector_half_width_rad: F,
    /// Bandwidth for the 1-D mean-shift mode finder, expressed as a fraction
    /// of each sector's median `|offset|`. Defaults to `0.15`.
    pub bandwidth_rel: F,
    /// Maximum mean-shift iterations before falling back to the sector
    /// median. Defaults to `20`.
    pub mean_shift_max_iters: u32,
    /// Mean-shift converges when the update magnitude drops below
    /// `bandwidth × convergence_rel`. Defaults to `1e-3`.
    pub mean_shift_convergence_rel: F,
    /// Denominator used when converting supporter count to confidence. A
    /// well-connected interior corner has up to 4 supporters in each axis
    /// (2 left/right, 2 up/down), so the default of `4.0` keeps well-supported
    /// corners at confidence ≥ 1.0.
    pub confidence_denominator: F,
}

impl<F: Float> Default for LocalStepParams<F> {
    fn default() -> Self {
        Self {
            k_neighbors: 8,
            max_step_factor: F::from_subset(&3.0),
            sector_half_width_rad: F::pi() / F::from_subset(&6.0),
            bandwidth_rel: F::from_subset(&0.15),
            mean_shift_max_iters: 20,
            mean_shift_convergence_rel: F::from_subset(&1e-3),
            confidence_denominator: F::from_subset(&4.0),
        }
    }
}

/// Compute a per-point local grid step along each point's two local axes.
///
/// Returns a vector whose length matches `points`. Points that end up with no
/// usable neighbors receive [`LocalStep::default`] (all zeros + zero
/// confidence), letting downstream validators fall back to a global step.
pub fn estimate_local_steps<F: Float + kiddo::float::kdtree::Axis>(
    points: &[LocalStepPointData<F>],
    params: &LocalStepParams<F>,
) -> Vec<LocalStep<F>> {
    if points.is_empty() {
        return Vec::new();
    }

    // Build the KD-tree once, reuse for every query.
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
    let k = params.k_neighbors.saturating_add(1); // +1 because the source itself will come back
    let results =
        tree.nearest_n::<SquaredEuclidean>(&[source.position.x, source.position.y], k.max(2));

    // Collect (distance, offset) for real neighbors.
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

    // Coarse outlier reject by distance.
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

    // Bin into u/v sectors via each axis folded to [0, π).
    let line_u = fold_to_line(source.axis_u);
    let line_v = fold_to_line(source.axis_v);
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

    let total_sup = F::from_subset(&((sup_u + sup_v) as f64));
    let confidence = RealField::max(
        RealField::min(total_sup / params.confidence_denominator, F::one()),
        F::zero(),
    );

    LocalStep {
        step_u,
        step_v,
        confidence,
        supporters_u: sup_u,
        supporters_v: sup_v,
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
            // Epanechnikov-style weight: 1 - (diff/bandwidth)^2, clamped to 0.
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
    // Mean-shift did not converge; fall back to the median.
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

/// Absolute angular difference between two undirected lines (both in `[0, π)`).
/// Result is in `[0, π/2]`.
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
        (sorted[n / 2 - 1] + sorted[n / 2]) * F::from_subset(&0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nalgebra::Point2;

    fn lspd(x: f32, y: f32, axis_u: f32) -> LocalStepPointData<f32> {
        LocalStepPointData {
            position: Point2::new(x, y),
            axis_u,
            axis_v: axis_u + std::f32::consts::FRAC_PI_2,
        }
    }

    fn regular_grid(
        rows: u32,
        cols: u32,
        spacing: f32,
        angle: f32,
    ) -> Vec<LocalStepPointData<f32>> {
        let (cx, sx) = (angle.cos(), angle.sin());
        let mut out = Vec::new();
        for j in 0..rows {
            for i in 0..cols {
                let i_f = i as f32 * spacing;
                let j_f = j as f32 * spacing;
                let x = i_f * cx - j_f * sx;
                let y = i_f * sx + j_f * cx;
                out.push(lspd(x, y, angle));
            }
        }
        out
    }

    #[test]
    fn regular_grid_recovers_spacing_at_multiple_scales() {
        let params = LocalStepParams::<f32>::default();
        for &spacing in &[10.0_f32, 20.0, 40.0] {
            let pts = regular_grid(5, 5, spacing, 0.0);
            let steps = estimate_local_steps(&pts, &params);
            // Interior point (center of the 5×5 grid, index 12).
            let s = &steps[12];
            assert!(
                (s.step_u - spacing).abs() / spacing < 0.05,
                "spacing {spacing}: step_u {} off >5%",
                s.step_u
            );
            assert!((s.step_v - spacing).abs() / spacing < 0.05);
            assert!(s.supporters_u >= 2 && s.supporters_v >= 2);
            assert!(s.confidence > 0.8);
        }
    }

    #[test]
    fn rotated_grid_is_sector_invariant() {
        let params = LocalStepParams::<f32>::default();
        for &deg in &[0.0_f32, 15.0, 30.0, 45.0] {
            let angle = deg.to_radians();
            let pts = regular_grid(5, 5, 20.0, angle);
            let steps = estimate_local_steps(&pts, &params);
            let s = &steps[12];
            assert!(
                (s.step_u - 20.0).abs() < 1.0,
                "angle {deg}°: step_u {} deviates",
                s.step_u
            );
            assert!((s.step_v - 20.0).abs() < 1.0);
        }
    }

    #[test]
    fn mild_barrel_distortion_is_tolerated() {
        // Apply a mild pincushion/barrel-like radial perturbation and check
        // that the estimator still recovers step ~ spacing at interior points
        // to within ~10 %.
        let spacing = 25.0;
        let mut pts = regular_grid(7, 7, spacing, 0.0);
        for p in &mut pts {
            let cx = 3.0 * spacing;
            let cy = 3.0 * spacing;
            let dx = p.position.x - cx;
            let dy = p.position.y - cy;
            let r2 = dx * dx + dy * dy;
            let scale = 1.0 + 1e-5 * r2;
            p.position = Point2::new(cx + dx * scale, cy + dy * scale);
        }
        let steps = estimate_local_steps(&pts, &LocalStepParams::<f32>::default());
        let interior = 24usize; // center of 7×7.
        let s = &steps[interior];
        assert!(
            (s.step_u - spacing).abs() / spacing < 0.1,
            "step_u {} far from spacing {spacing}",
            s.step_u
        );
    }

    #[test]
    fn dual_scale_grid_picks_dominant_mode() {
        // Board-scale 5×5 lattice at spacing=20.
        let mut pts = regular_grid(5, 5, 20.0, 0.0);
        // Inject a minority of "marker-internal" neighbors at ~0.2× spacing
        // around each interior cell. The markers sit OFF the board axes
        // (at a 45° diagonal inside the cell) and carry their own rotated
        // axes, so the default sector filter should reject them. Even if one
        // sneaks into the k-NN window it is outnumbered by the 4 cardinal
        // board neighbors.
        let marker_angle = 20.0_f32.to_radians();
        let interior_pts: Vec<usize> = (1..4)
            .flat_map(|j| (1..4).map(move |i| j * 5 + i))
            .collect();
        for &idx in &interior_pts {
            let c = pts[idx].position;
            pts.push(LocalStepPointData {
                position: Point2::new(c.x + 3.0, c.y + 3.0),
                axis_u: marker_angle,
                axis_v: marker_angle + std::f32::consts::FRAC_PI_2,
            });
        }
        let steps = estimate_local_steps(&pts, &LocalStepParams::<f32>::default());
        let s = &steps[12]; // center of the board-scale grid.
                            // Expect the board-scale ~20 px step, not the marker-scale ~4 px.
        assert!(
            (s.step_u - 20.0).abs() < 2.0,
            "expected board step ~20 for u, got {}",
            s.step_u
        );
        assert!(
            (s.step_v - 20.0).abs() < 2.0,
            "expected board step ~20 for v, got {}",
            s.step_v
        );
    }

    #[test]
    fn isolated_point_reports_zero_confidence() {
        let pts = vec![lspd(0.0, 0.0, 0.0)];
        let steps = estimate_local_steps(&pts, &LocalStepParams::<f32>::default());
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].confidence, 0.0);
        assert_eq!(steps[0].step_u, 0.0);
        assert_eq!(steps[0].step_v, 0.0);
    }

    #[test]
    fn fold_and_line_diff_roundtrip() {
        let pi = std::f32::consts::PI;
        for &theta in &[-pi, -0.5, 0.0, 0.5, pi - 1e-3, pi, 1.5 * pi, 2.5 * pi] {
            let folded = fold_to_line(theta);
            assert!(
                (0.0..pi).contains(&folded),
                "fold({theta}) = {folded} escaped [0, π)"
            );
        }
        // Axes 0 and π/2 are orthogonal → line_diff = π/2.
        assert!(
            (line_diff(0.0, std::f32::consts::FRAC_PI_2) - std::f32::consts::FRAC_PI_2).abs()
                < 1e-5
        );
        // Axes 0 and π-ε are nearly parallel.
        assert!(line_diff(0.0, pi - 1e-3) < 1e-2);
    }
}
