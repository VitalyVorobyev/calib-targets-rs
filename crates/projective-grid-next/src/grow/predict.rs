//! Shared prediction helper used by both [`grow::engine`](crate::grow::engine)
//! and [`refine::fill`](crate::refine::fill) (Phase 4).
//!
//! ## Closes Gap 5 + Gap 6
//!
//! Legacy `square::grow::predict_from_neighbours` and
//! `calib-targets-chessboard::boosters::predict_from_neighbors` carried two
//! near-identical copies of the same algorithm; the new design has **one**
//! prediction function shared by both BFS and post-grow fill. The local-step
//! fallback also closes Gap 5: the `estimate_local_steps` routine from
//! [`crate::stats`] finally has a production consumer.
//!
//! ## Algorithm
//!
//! For each labelled neighbour `N_k` at coord `(i_k, j_k)`:
//!
//! 1. Compute the integer offset `Δ = target - N_k`.
//! 2. Estimate the local per-cell step at `N_k`:
//!    - If the neighbour carries its own `local_step_u` / `local_step_v`
//!      (typically from a previous central-difference at the BFS engine),
//!      use those.
//!    - Otherwise fall back to the global axes scaled by `global_cell_size`.
//! 3. The predicted position is `N_k.position + Δ.i · step_u + Δ.j · step_v`.
//!
//! Predictions are combined with **inverse squared grid-distance weighting**:
//! `w_k = 1 / (Δi² + Δj²)`. Cardinal neighbours (grid distance 1) carry
//! weight 1.0; diagonal neighbours (grid distance √2) carry weight 0.5 —
//! variance addition per grid step. A neighbour at the target cell itself
//! (`Δ = 0`) is treated as `w = 1` to avoid `NaN` (in practice the BFS engine
//! never enqueues such a neighbour because it is already labelled).
//!
//! See `docs/algorithmic_gaps.md` Gap 3 → 5 → 6 for the historical context.

use nalgebra::{Point2, Vector2};

use crate::float::{lit, Float};
use crate::lattice::Coord;

/// Inputs to [`predict_from_neighbours`].
///
/// Bundled to stay under the workspace's `too_many_arguments = "deny"` lint
/// and to let the same function serve both the BFS engine and the post-grow
/// fill (the fill caller may pass identical `global_axes` / `global_cell_size`
/// but a richer `neighbours` slice with central-difference local steps).
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct PredictCtx<'a, F: Float> {
    /// Coord the engine is trying to label.
    pub target_coord: Coord,
    /// Labelled neighbours contributing to the prediction.
    pub neighbours: &'a [LabelledNeighbour<F>],
    /// Unit-length grid axes `(u, v)` — fallback per-step direction when no
    /// neighbour carries a local step estimate.
    pub global_axes: [Vector2<F>; 2],
    /// Cell size in pixels used with `global_axes` for the fallback step.
    pub global_cell_size: F,
    /// Whether to consume `estimate_local_steps` results passed in via the
    /// `LabelledNeighbour.local_step_*` fields. Always `true` for the BFS
    /// engine in v1; reserved for the post-grow fill which may opt out when
    /// the labelled set is sparse enough that local-step noise dominates.
    pub local_step_fallback: bool,
}

/// A labelled neighbour contributing to a prediction.
///
/// `local_step_u` / `local_step_v` are pre-computed per-neighbour local step
/// vectors (typically central differences from the BFS engine's labelled
/// map). When `None`, [`predict_from_neighbours`] falls back to the global
/// axes scaled by `global_cell_size`.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct LabelledNeighbour<F: Float> {
    /// The neighbour's `(i, j)` cell.
    pub coord: Coord,
    /// The neighbour's pixel position.
    pub position: Point2<F>,
    /// Optional pre-computed local step along the `+i` direction (one cell).
    pub local_step_u: Option<Vector2<F>>,
    /// Optional pre-computed local step along the `+j` direction (one cell).
    pub local_step_v: Option<Vector2<F>>,
}

impl<F: Float> LabelledNeighbour<F> {
    /// Construct a labelled neighbour with no local step estimates.
    pub fn new(coord: Coord, position: Point2<F>) -> Self {
        Self {
            coord,
            position,
            local_step_u: None,
            local_step_v: None,
        }
    }

    /// Attach a local `+i` step.
    #[must_use]
    pub fn with_local_step_u(mut self, step: Vector2<F>) -> Self {
        self.local_step_u = Some(step);
        self
    }

    /// Attach a local `+j` step.
    #[must_use]
    pub fn with_local_step_v(mut self, step: Vector2<F>) -> Self {
        self.local_step_v = Some(step);
        self
    }
}

/// Outcome of [`predict_from_neighbours`].
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct PredictedPosition<F: Float> {
    /// Weighted average prediction in pixels.
    pub position: Point2<F>,
    /// Sum of per-neighbour weights, useful for downstream diagnostics
    /// (low total weight ⇒ thin support, treat with care).
    pub weight_sum: F,
    /// `true` when at least one contributing neighbour carried a non-`None`
    /// local-step estimate. Diagnostic only; the prediction itself is the
    /// same combined position regardless.
    pub from_local_step: bool,
}

/// Combine labelled-neighbour predictions into a single position estimate.
///
/// Returns `None` when `neighbours` is empty — the caller is expected to
/// handle that by emitting a `GrowRejected { reason: NoCandidate }` (or the
/// analogous fill-pass event).
pub fn predict_from_neighbours<F: Float>(ctx: PredictCtx<'_, F>) -> Option<PredictedPosition<F>> {
    if ctx.neighbours.is_empty() {
        return None;
    }
    let global_i_step = ctx.global_axes[0] * ctx.global_cell_size;
    let global_j_step = ctx.global_axes[1] * ctx.global_cell_size;

    let mut sum_x = F::zero();
    let mut sum_y = F::zero();
    let mut weight_sum = F::zero();
    let mut from_local_step = false;

    for n in ctx.neighbours {
        let di = lit::<F>((ctx.target_coord.0 - n.coord.0) as f32);
        let dj = lit::<F>((ctx.target_coord.1 - n.coord.1) as f32);
        let d2 = di * di + dj * dj;
        let w = if d2 > F::zero() {
            F::one() / d2
        } else {
            F::one()
        };

        let (i_step, used_local_i) = match (ctx.local_step_fallback, n.local_step_u) {
            (true, Some(step)) => (step, true),
            _ => (global_i_step, false),
        };
        let (j_step, used_local_j) = match (ctx.local_step_fallback, n.local_step_v) {
            (true, Some(step)) => (step, true),
            _ => (global_j_step, false),
        };
        from_local_step = from_local_step || used_local_i || used_local_j;

        let off = i_step * di + j_step * dj;
        sum_x += w * (n.position.x + off.x);
        sum_y += w * (n.position.y + off.y);
        weight_sum += w;
    }

    if weight_sum == F::zero() {
        return None;
    }

    Some(PredictedPosition {
        position: Point2::new(sum_x / weight_sum, sum_y / weight_sum),
        weight_sum,
        from_local_step,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::float::{abs, lit};

    fn assert_predict_weights_diagonal_less_than_cardinal<F: Float>() {
        // Replicate the legacy `predict_weights_diagonal_less_than_cardinal`
        // test verbatim: two neighbours, one cardinal at Δ²=1, one diagonal
        // at Δ²=8; expected weighted prediction is `(50 + 0.125 · 54) / 1.125`.
        let s = lit::<F>(10.0_f32);
        let u = Vector2::new(F::one(), F::zero());
        let v = Vector2::new(F::zero(), F::one());
        let target = (5, 5);
        let cardinal =
            LabelledNeighbour::new((5, 4), Point2::new(lit::<F>(50.0_f32), lit::<F>(40.0_f32)));
        let diagonal =
            LabelledNeighbour::new((3, 3), Point2::new(lit::<F>(30.0_f32), lit::<F>(34.0_f32)));
        let neighbours = [cardinal, diagonal];
        let ctx = PredictCtx {
            target_coord: target,
            neighbours: &neighbours,
            global_axes: [u, v],
            global_cell_size: s,
            local_step_fallback: true,
        };
        let pred = predict_from_neighbours(ctx).expect("non-empty neighbours predict");

        let expected_y =
            (lit::<F>(50.0_f32) + lit::<F>(0.125_f32) * lit::<F>(54.0_f32)) / lit::<F>(1.125_f32);
        assert!(abs::<F>(pred.position.x - lit::<F>(50.0_f32)) < lit::<F>(1e-4_f32));
        assert!(abs::<F>(pred.position.y - expected_y) < lit::<F>(1e-4_f32));
        // The d² down-weighting should have suppressed the diagonal's y-bias.
        let equal_weight_y = (lit::<F>(50.0_f32) + lit::<F>(54.0_f32)) * lit::<F>(0.5_f32);
        let weighted_bias = pred.position.y - lit::<F>(50.0_f32);
        let equal_bias = equal_weight_y - lit::<F>(50.0_f32);
        assert!(weighted_bias < equal_bias);
        // Neither neighbour carried a local step.
        assert!(!pred.from_local_step);
    }

    fn assert_predict_with_only_cardinal_recovers_exact_offset<F: Float>() {
        let s = lit::<F>(12.0_f32);
        let u = Vector2::new(F::one(), F::zero());
        let v = Vector2::new(F::zero(), F::one());
        let target = (2, 2);
        let neighbour = LabelledNeighbour::new((1, 2), Point2::new(s, lit::<F>(2.0_f32) * s));
        let ctx = PredictCtx {
            target_coord: target,
            neighbours: &[neighbour],
            global_axes: [u, v],
            global_cell_size: s,
            local_step_fallback: true,
        };
        let pred = predict_from_neighbours(ctx).unwrap();
        assert!(abs::<F>(pred.position.x - lit::<F>(2.0_f32) * s) < lit::<F>(1e-4_f32));
        assert!(abs::<F>(pred.position.y - lit::<F>(2.0_f32) * s) < lit::<F>(1e-4_f32));
    }

    fn assert_predict_uses_local_step_when_provided<F: Float>() {
        // Foreshortened-grid scenario from the legacy test:
        //   target = (2, 0); neighbour at (3, 0) with position (300, 0).
        //   Global cell size = 50; the neighbour carries its own
        //   local_step_u = (+10, 0). Expected prediction at (290, 0).
        let u = Vector2::new(F::one(), F::zero());
        let v = Vector2::new(F::zero(), F::one());
        let global_cell_size = lit::<F>(50.0_f32);
        let neighbour = LabelledNeighbour::new((3, 0), Point2::new(lit::<F>(300.0_f32), F::zero()))
            .with_local_step_u(Vector2::new(lit::<F>(10.0_f32), F::zero()));
        let ctx = PredictCtx {
            target_coord: (2, 0),
            neighbours: &[neighbour],
            global_axes: [u, v],
            global_cell_size,
            local_step_fallback: true,
        };
        let pred = predict_from_neighbours(ctx).unwrap();
        assert!(abs::<F>(pred.position.x - lit::<F>(290.0_f32)) < lit::<F>(1e-3_f32));
        assert!(abs::<F>(pred.position.y - F::zero()) < lit::<F>(1e-3_f32));
        assert!(pred.from_local_step);
    }

    fn assert_predict_falls_back_to_global_when_no_local_steps<F: Float>() {
        let u = Vector2::new(F::one(), F::zero());
        let v = Vector2::new(F::zero(), F::one());
        let s = lit::<F>(25.0_f32);
        let neighbour = LabelledNeighbour::new(
            (4, 4),
            Point2::new(lit::<F>(100.0_f32), lit::<F>(100.0_f32)),
        );
        let ctx = PredictCtx {
            target_coord: (5, 4),
            neighbours: &[neighbour],
            global_axes: [u, v],
            global_cell_size: s,
            local_step_fallback: true,
        };
        let pred = predict_from_neighbours(ctx).unwrap();
        assert!(abs::<F>(pred.position.x - (lit::<F>(100.0_f32) + s)) < lit::<F>(1e-3_f32));
        assert!(abs::<F>(pred.position.y - lit::<F>(100.0_f32)) < lit::<F>(1e-3_f32));
        assert!(!pred.from_local_step);
    }

    fn assert_predict_empty_neighbours_returns_none<F: Float>() {
        let u = Vector2::new(F::one(), F::zero());
        let v = Vector2::new(F::zero(), F::one());
        let ctx = PredictCtx::<F> {
            target_coord: (0, 0),
            neighbours: &[],
            global_axes: [u, v],
            global_cell_size: lit::<F>(20.0_f32),
            local_step_fallback: true,
        };
        assert!(predict_from_neighbours(ctx).is_none());
    }

    fn assert_local_step_fallback_off_uses_global<F: Float>() {
        let u = Vector2::new(F::one(), F::zero());
        let v = Vector2::new(F::zero(), F::one());
        let global_cell_size = lit::<F>(50.0_f32);
        let neighbour = LabelledNeighbour::new((3, 0), Point2::new(lit::<F>(300.0_f32), F::zero()))
            .with_local_step_u(Vector2::new(lit::<F>(10.0_f32), F::zero()));
        let ctx = PredictCtx {
            target_coord: (2, 0),
            neighbours: &[neighbour],
            global_axes: [u, v],
            global_cell_size,
            local_step_fallback: false,
        };
        let pred = predict_from_neighbours(ctx).unwrap();
        // With fallback off, the (3,0) neighbour's local step is ignored;
        // global step puts the prediction at 300 - 50 = 250.
        assert!(abs::<F>(pred.position.x - lit::<F>(250.0_f32)) < lit::<F>(1e-3_f32));
        assert!(!pred.from_local_step);
    }

    #[test]
    fn diagonal_lt_cardinal_f32() {
        assert_predict_weights_diagonal_less_than_cardinal::<f32>();
    }
    #[test]
    fn diagonal_lt_cardinal_f64() {
        assert_predict_weights_diagonal_less_than_cardinal::<f64>();
    }
    #[test]
    fn only_cardinal_exact_offset_f32() {
        assert_predict_with_only_cardinal_recovers_exact_offset::<f32>();
    }
    #[test]
    fn only_cardinal_exact_offset_f64() {
        assert_predict_with_only_cardinal_recovers_exact_offset::<f64>();
    }
    #[test]
    fn uses_local_step_when_provided_f32() {
        assert_predict_uses_local_step_when_provided::<f32>();
    }
    #[test]
    fn uses_local_step_when_provided_f64() {
        assert_predict_uses_local_step_when_provided::<f64>();
    }
    #[test]
    fn falls_back_to_global_when_no_local_steps_f32() {
        assert_predict_falls_back_to_global_when_no_local_steps::<f32>();
    }
    #[test]
    fn falls_back_to_global_when_no_local_steps_f64() {
        assert_predict_falls_back_to_global_when_no_local_steps::<f64>();
    }
    #[test]
    fn empty_neighbours_returns_none_f32() {
        assert_predict_empty_neighbours_returns_none::<f32>();
    }
    #[test]
    fn empty_neighbours_returns_none_f64() {
        assert_predict_empty_neighbours_returns_none::<f64>();
    }
    #[test]
    fn local_step_fallback_off_uses_global_f32() {
        assert_local_step_fallback_off_uses_global::<f32>();
    }
    #[test]
    fn local_step_fallback_off_uses_global_f64() {
        assert_local_step_fallback_off_uses_global::<f64>();
    }
}
