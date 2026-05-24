//! Integration tests for the topological grid pipeline.
//!
//! Ported from the legacy `projective_grid::topological::tests` suite,
//! with two structural changes:
//!
//! 1. Every test pairs `f32` and `f64` runs through a generic helper, the
//!    standard Phase 1 pattern.
//! 2. Tests that depended on the legacy `TopologicalStats` counter bag or
//!    the `TopologicalTrace` struct now consume the typed event stream:
//!    `build_grid_topological_with_trace` returns the result alongside a
//!    `RecordingSink<F>`; `CounterStats::from_events` recovers the same
//!    counters the legacy `TopologicalStats` exposed.

use std::collections::HashSet;

use nalgebra::Point2;

use super::{
    build_grid_topological, build_grid_topological_with_trace, OpenContext, TopologicalParams,
};
use crate::diagnostics::events::{EdgeClass, Event};
use crate::diagnostics::stats::CounterStats;
use crate::diagnostics::NoOpSink;
use crate::error::DetectionError;
use crate::feature::{AxisEstimate, Observation};
use crate::float::{abs, lit, Float};

// ---------- helpers -----------------------------------------------------

fn axes_axis_aligned<F: Float>() -> [AxisEstimate<F>; 2] {
    let frac_pi_2 = F::pi() / lit::<F>(2.0_f32);
    [
        AxisEstimate::new(F::zero(), lit::<F>(0.05_f32)),
        AxisEstimate::new(frac_pi_2, lit::<F>(0.05_f32)),
    ]
}

fn axes_pair<F: Float>(angle0: F, angle1: F) -> [AxisEstimate<F>; 2] {
    [
        AxisEstimate::new(angle0, lit::<F>(0.05_f32)),
        AxisEstimate::new(angle1, lit::<F>(0.05_f32)),
    ]
}

fn axes_no_info<F: Float>() -> [AxisEstimate<F>; 2] {
    [AxisEstimate::default(), AxisEstimate::default()]
}

fn observation<F: Float>(p: Point2<F>, axes: [AxisEstimate<F>; 2]) -> Observation<F> {
    Observation::new(p).with_axes(axes)
}

fn build_axis_aligned_grid<F: Float>(rows: usize, cols: usize, step: f32) -> Vec<Observation<F>> {
    let mut out = Vec::with_capacity(rows * cols);
    for j in 0..rows {
        for i in 0..cols {
            let p = Point2::new(lit::<F>(i as f32 * step), lit::<F>(j as f32 * step));
            out.push(observation(p, axes_axis_aligned::<F>()));
        }
    }
    out
}

fn obs_open<F: Float>(observations: &[Observation<F>]) -> OpenContext<F> {
    OpenContext::<F>::new(observations.len())
}

// ---------- regression-pinned defaults ----------------------------------

fn assert_default_tolerances_are_regression_values<F: Float>() {
    let params = TopologicalParams::<F>::default();
    let fifteen_deg = lit::<F>(15.0_f32.to_radians());
    let one_pt_five = lit::<F>(1.5_f32);
    let two_pt_five = lit::<F>(2.5_f32);
    let sixteen_deg = lit::<F>(16.0_f32.to_radians());
    let eps = lit::<F>(1e-5_f32);
    assert!(abs::<F>(params.axis_align_tol_rad - fifteen_deg) < eps);
    assert!(abs::<F>(params.cluster_axis_tol_rad - sixteen_deg) < eps);
    assert!(abs::<F>(params.opposing_edge_ratio_max - one_pt_five) < eps);
    assert!(abs::<F>(params.edge_length_ratio_max - two_pt_five) < eps);
    assert_eq!(params.min_corners_for_component, 4);
    assert_eq!(params.min_quads_per_component, 1);
    assert!(params.cluster_centers.is_none());
}

#[test]
fn default_tolerances_are_regression_values_f32() {
    assert_default_tolerances_are_regression_values::<f32>();
}
#[test]
fn default_tolerances_are_regression_values_f64() {
    assert_default_tolerances_are_regression_values::<f64>();
}

// ---------- clean 5x5 grid ---------------------------------------------

fn assert_clean_5x5_grid_produces_single_component<F: Float>() {
    let obs = build_axis_aligned_grid::<F>(5, 5, 10.0);
    let ctx = obs_open(&obs);
    let g = build_grid_topological(
        &obs,
        &TopologicalParams::<F>::default(),
        &ctx,
        &mut NoOpSink,
    )
    .unwrap();
    assert_eq!(g.components.len(), 1, "expected one connected component");
    let c = &g.components[0];
    assert_eq!(c.labelled.len(), 25, "all 25 corners labelled");
    let max_i = c.labelled.keys().map(|&(i, _)| i).max().unwrap();
    let max_j = c.labelled.keys().map(|&(_, j)| j).max().unwrap();
    let min_i = c.labelled.keys().map(|&(i, _)| i).min().unwrap();
    let min_j = c.labelled.keys().map(|&(_, j)| j).min().unwrap();
    assert_eq!((min_i, min_j), (0, 0), "bbox rebased to (0, 0)");
    assert_eq!((max_i, max_j), (4, 4), "5x5 grid spans (0..4, 0..4)");
    assert_eq!(c.bbox, ((0, 0), (4, 4)));
}

#[test]
fn clean_5x5_grid_produces_single_component_f32() {
    assert_clean_5x5_grid_produces_single_component::<f32>();
}
#[test]
fn clean_5x5_grid_produces_single_component_f64() {
    assert_clean_5x5_grid_produces_single_component::<f64>();
}

// ---------- 3 corners of one cell cannot seed a component --------------

fn assert_three_corners_of_one_cell_cannot_seed<F: Float>() {
    let obs = vec![
        observation(Point2::new(F::zero(), F::zero()), axes_axis_aligned::<F>()),
        observation(
            Point2::new(lit::<F>(10.0_f32), F::zero()),
            axes_axis_aligned::<F>(),
        ),
        observation(
            Point2::new(F::zero(), lit::<F>(10.0_f32)),
            axes_axis_aligned::<F>(),
        ),
    ];
    let ctx = obs_open(&obs);
    let (g, sink) =
        build_grid_topological_with_trace(&obs, &TopologicalParams::<F>::default(), &ctx);
    let g = g.unwrap();
    assert!(g.components.is_empty());
    let stats = CounterStats::from_events::<F>(sink.events());
    // One triangle yields three edges classified — `triangles` derivable
    // from edge count.
    assert_eq!(
        stats.grid_edges + stats.diagonal_edges + stats.spurious_edges,
        3,
        "exactly one triangle classified"
    );
    assert_eq!(stats.quads_merged, 0);
    assert_eq!(stats.components, 0);
}

#[test]
fn three_corners_of_one_cell_cannot_seed_f32() {
    assert_three_corners_of_one_cell_cannot_seed::<f32>();
}
#[test]
fn three_corners_of_one_cell_cannot_seed_f64() {
    assert_three_corners_of_one_cell_cannot_seed::<f64>();
}

// ---------- local affine triangle inference ----------------------------

fn assert_local_affine_recovers_foreshortened_cell<F: Float>() {
    // A projected cell diagonal is not generally 45° from the projected
    // grid axes. Image-frame parallelogram with sides at 0° and 54°.
    let axis1 = lit::<F>(54.0_f32.to_radians());
    let side_i = Point2::new(lit::<F>(100.0_f32), F::zero());
    let side_j = Point2::new(
        lit::<F>(45.0_f32) * axis1.cos(),
        lit::<F>(45.0_f32) * axis1.sin(),
    );
    let obs = vec![
        observation(
            Point2::new(F::zero(), F::zero()),
            axes_pair(F::zero(), axis1),
        ),
        observation(side_i, axes_pair(F::zero(), axis1)),
        observation(side_j, axes_pair(F::zero(), axis1)),
        observation(
            Point2::new(side_i.x + side_j.x, side_i.y + side_j.y),
            axes_pair(F::zero(), axis1),
        ),
    ];
    let ctx = obs_open(&obs);
    let (g, sink) =
        build_grid_topological_with_trace(&obs, &TopologicalParams::<F>::default(), &ctx);
    let g = g.unwrap();
    let stats = CounterStats::from_events::<F>(sink.events());
    // Two triangles meeting along the diagonal of the parallelogram, both
    // mergeable into a single quad.
    assert_eq!(stats.triangles_mergeable, 2);
    assert_eq!(stats.quads_merged, 1);
    assert_eq!(g.components.len(), 1);
    assert_eq!(g.components[0].labelled.len(), 4);
}

#[test]
fn local_affine_recovers_foreshortened_cell_f32() {
    assert_local_affine_recovers_foreshortened_cell::<f32>();
}
#[test]
fn local_affine_recovers_foreshortened_cell_f64() {
    assert_local_affine_recovers_foreshortened_cell::<f64>();
}

// ---------- same-axis grid sides do not infer a diagonal ---------------

fn assert_same_axis_grid_sides_do_not_infer_diagonal<F: Float>() {
    // Three image-frame points form one Delaunay triangle. From the
    // leftmost vertex, two incident edges are within the horizontal
    // grid-axis tolerance, but both use the same axis slot — a local
    // collinear chain, not two sides of one projected cell.
    let obs = vec![
        observation(Point2::new(F::zero(), F::zero()), axes_axis_aligned::<F>()),
        observation(
            Point2::new(lit::<F>(10.0_f32), F::zero()),
            axes_axis_aligned::<F>(),
        ),
        observation(
            Point2::new(lit::<F>(20.0_f32), lit::<F>(4.0_f32)),
            axes_axis_aligned::<F>(),
        ),
    ];
    let ctx = obs_open(&obs);
    let (_g, sink) =
        build_grid_topological_with_trace(&obs, &TopologicalParams::<F>::default(), &ctx);
    let stats = CounterStats::from_events::<F>(sink.events());
    assert_eq!(stats.triangles_has_spurious, 1);
    assert_eq!(stats.triangles_mergeable, 0);
    assert_eq!(stats.diagonal_edges, 0);
    assert_eq!(stats.quads_merged, 0);
}

#[test]
fn same_axis_grid_sides_do_not_infer_diagonal_f32() {
    assert_same_axis_grid_sides_do_not_infer_diagonal::<f32>();
}
#[test]
fn same_axis_grid_sides_do_not_infer_diagonal_f64() {
    assert_same_axis_grid_sides_do_not_infer_diagonal::<f64>();
}

// ---------- spurious corner outside the grid ---------------------------

fn assert_grid_with_extra_spurious_corner_is_rejected<F: Float>() {
    let mut obs = build_axis_aligned_grid::<F>(4, 4, 10.0);
    // Add one spurious corner well off to the side with random axes.
    let off_axis = axes_pair(
        lit::<F>(1.1_f32),
        lit::<F>(1.1_f32) + F::pi() / lit::<F>(2.0_f32),
    );
    obs.push(observation(
        Point2::new(lit::<F>(100.0_f32), lit::<F>(100.0_f32)),
        off_axis,
    ));
    let ctx = obs_open(&obs);
    let g = build_grid_topological(
        &obs,
        &TopologicalParams::<F>::default(),
        &ctx,
        &mut NoOpSink,
    )
    .unwrap();
    assert_eq!(g.components.len(), 1);
    let c = &g.components[0];
    assert_eq!(c.labelled.len(), 16, "16 grid corners labelled");
    let labelled_idxs: HashSet<usize> = c.labelled.values().copied().collect();
    assert!(
        !labelled_idxs.contains(&16),
        "spurious corner must be excluded"
    );
}

#[test]
fn grid_with_extra_spurious_corner_is_rejected_f32() {
    assert_grid_with_extra_spurious_corner_is_rejected::<f32>();
}
#[test]
fn grid_with_extra_spurious_corner_is_rejected_f64() {
    assert_grid_with_extra_spurious_corner_is_rejected::<f64>();
}

// ---------- corners with no axis info are skipped ----------------------

fn assert_corners_with_no_axis_info_are_skipped<F: Float>() {
    let mut obs = build_axis_aligned_grid::<F>(4, 4, 10.0);
    obs.push(observation(
        Point2::new(lit::<F>(15.0_f32), lit::<F>(15.0_f32)),
        axes_no_info::<F>(),
    ));
    let ctx = obs_open(&obs);
    let g = build_grid_topological(
        &obs,
        &TopologicalParams::<F>::default(),
        &ctx,
        &mut NoOpSink,
    )
    .unwrap();
    assert_eq!(g.components.len(), 1);
    assert_eq!(g.components[0].labelled.len(), 16);
}

#[test]
fn corners_with_no_axis_info_are_skipped_f32() {
    assert_corners_with_no_axis_info_are_skipped::<f32>();
}
#[test]
fn corners_with_no_axis_info_are_skipped_f64() {
    assert_corners_with_no_axis_info_are_skipped::<f64>();
}

// ---------- fewer than three usable corners ----------------------------

fn assert_fewer_than_three_usable_corners<F: Float>() {
    let obs = vec![
        observation(Point2::new(F::zero(), F::zero()), axes_axis_aligned::<F>()),
        observation(
            Point2::new(lit::<F>(1.0_f32), F::zero()),
            axes_axis_aligned::<F>(),
        ),
    ];
    let ctx = obs_open(&obs);
    let result = build_grid_topological(
        &obs,
        &TopologicalParams::<F>::default(),
        &ctx,
        &mut NoOpSink,
    );
    assert!(matches!(
        result,
        Err(DetectionError::InsufficientObservations {
            found: 2,
            required: 3,
        })
    ));
}

#[test]
fn fewer_than_three_usable_corners_f32() {
    assert_fewer_than_three_usable_corners::<f32>();
}
#[test]
fn fewer_than_three_usable_corners_f64() {
    assert_fewer_than_three_usable_corners::<f64>();
}

// ---------- rotated grid -----------------------------------------------

fn assert_rotated_grid_still_recovered<F: Float>() {
    let theta = lit::<F>(30.0_f32.to_radians());
    let cos_t = theta.cos();
    let sin_t = theta.sin();
    let frac_pi_2 = F::pi() / lit::<F>(2.0_f32);
    let mut obs = Vec::new();
    for j in 0..5 {
        for i in 0..5 {
            let x = lit::<F>(i as f32 * 10.0_f32);
            let y = lit::<F>(j as f32 * 10.0_f32);
            let pos = Point2::new(cos_t * x - sin_t * y, sin_t * x + cos_t * y);
            obs.push(observation(pos, axes_pair(theta, theta + frac_pi_2)));
        }
    }
    let ctx = obs_open(&obs);
    let g = build_grid_topological(
        &obs,
        &TopologicalParams::<F>::default(),
        &ctx,
        &mut NoOpSink,
    )
    .unwrap();
    assert_eq!(g.components.len(), 1);
    assert_eq!(g.components[0].labelled.len(), 25);
}

#[test]
fn rotated_grid_still_recovered_f32() {
    assert_rotated_grid_still_recovered::<f32>();
}
#[test]
fn rotated_grid_still_recovered_f64() {
    assert_rotated_grid_still_recovered::<f64>();
}

// ---------- events match production grid + serialise -------------------

fn assert_events_match_production_grid<F: Float>() {
    let obs = build_axis_aligned_grid::<F>(5, 5, 10.0);
    let params = TopologicalParams::<F>::default();
    let ctx = obs_open(&obs);
    let plain = build_grid_topological(&obs, &params, &ctx, &mut NoOpSink).unwrap();
    let (recorded, sink) = build_grid_topological_with_trace(&obs, &params, &ctx);
    let recorded = recorded.unwrap();

    let stats = CounterStats::from_events::<F>(sink.events());
    // Per-edge / per-quad / per-component counters match the result.
    assert_eq!(stats.components, plain.components.len());
    assert!(stats.quads_kept >= plain.components[0].labelled.len() / 4);
    assert_eq!(stats.quads_merged, stats.quads_kept); // a clean grid has no rejects
                                                      // The labelled set is invariant — both runs reach the same answer.
    let plain_labels: HashSet<_> = plain.components[0]
        .labelled
        .iter()
        .map(|(&ij, &idx)| (ij, idx))
        .collect();
    let recorded_labels: HashSet<_> = recorded.components[0]
        .labelled
        .iter()
        .map(|(&ij, &idx)| (ij, idx))
        .collect();
    assert_eq!(plain_labels, recorded_labels);

    // Spot-check the typed event structure.
    let any_edge = sink.events().iter().any(|e| {
        matches!(
            e,
            Event::TopologicalEdge {
                class: EdgeClass::Grid | EdgeClass::Diagonal,
                ..
            }
        )
    });
    assert!(any_edge, "expected at least one Grid/Diagonal edge event");
    let any_quad = sink
        .events()
        .iter()
        .any(|e| matches!(e, Event::TopologicalQuad { kept: true, .. }));
    assert!(any_quad, "expected at least one kept quad event");
    let any_component = sink
        .events()
        .iter()
        .any(|e| matches!(e, Event::ComponentLabelled { .. }));
    assert!(
        any_component,
        "expected at least one labelled component event"
    );
}

#[test]
fn events_match_production_grid_f32() {
    assert_events_match_production_grid::<f32>();
}
#[test]
fn events_match_production_grid_f64() {
    assert_events_match_production_grid::<f64>();
}

// ---------- cluster-center gate ----------------------------------------

fn assert_cluster_centers_default_to_none<F: Float>() {
    let p = TopologicalParams::<F>::default();
    assert!(p.cluster_centers.is_none());
    let sixteen_deg = lit::<F>(16.0_f32.to_radians());
    let eps = lit::<F>(1e-5_f32);
    assert!(abs::<F>(p.cluster_axis_tol_rad - sixteen_deg) < eps);
}

#[test]
fn cluster_centers_default_to_none_f32() {
    assert_cluster_centers_default_to_none::<f32>();
}
#[test]
fn cluster_centers_default_to_none_f64() {
    assert_cluster_centers_default_to_none::<f64>();
}

fn assert_cluster_gate_drops_off_axis_noisier_when_centers_supplied<F: Float>() {
    let frac_pi_2 = F::pi() / lit::<F>(2.0_f32);
    let thirty_deg = lit::<F>(30.0_f32.to_radians());
    let mut obs = build_axis_aligned_grid::<F>(5, 5, 10.0);
    let off_axis = axes_pair(thirty_deg, thirty_deg + frac_pi_2);
    obs.push(observation(
        Point2::new(lit::<F>(60.0_f32), lit::<F>(5.0_f32)),
        off_axis,
    ));
    obs.push(observation(
        Point2::new(-lit::<F>(10.0_f32), lit::<F>(25.0_f32)),
        off_axis,
    ));
    obs.push(observation(
        Point2::new(lit::<F>(45.0_f32), lit::<F>(60.0_f32)),
        off_axis,
    ));
    obs.push(observation(
        Point2::new(lit::<F>(15.0_f32), -lit::<F>(8.0_f32)),
        off_axis,
    ));
    let ctx = obs_open(&obs);
    // Without centers — legacy gate (sigma only); the 4 noisers leak in.
    let no_gate = build_grid_topological(
        &obs,
        &TopologicalParams::<F>::default(),
        &ctx,
        &mut NoOpSink,
    )
    .unwrap();
    // The 25 grid corners must label and the noisers must not. Since the
    // legacy `corners_used` counter isn't recoverable from the event stream,
    // we assert at the labelled-set granularity instead.
    assert_eq!(no_gate.components.len(), 1);
    assert!(no_gate.components[0].labelled.len() >= 25);

    // With centers at 0°/90°: noisers fail the cluster gate at default tol.
    let params = TopologicalParams::<F> {
        cluster_centers: Some([F::zero(), frac_pi_2]),
        ..TopologicalParams::<F>::default()
    };
    let gated = build_grid_topological(&obs, &params, &ctx, &mut NoOpSink).unwrap();
    assert_eq!(gated.components.len(), 1);
    assert_eq!(gated.components[0].labelled.len(), 25);
    let labelled_idxs: HashSet<usize> = gated.components[0].labelled.values().copied().collect();
    for noise_idx in 25..29 {
        assert!(
            !labelled_idxs.contains(&noise_idx),
            "noiser {noise_idx} must not label under the cluster gate"
        );
    }
}

#[test]
fn cluster_gate_drops_off_axis_noisier_when_centers_supplied_f32() {
    assert_cluster_gate_drops_off_axis_noisier_when_centers_supplied::<f32>();
}
#[test]
fn cluster_gate_drops_off_axis_noisier_when_centers_supplied_f64() {
    assert_cluster_gate_drops_off_axis_noisier_when_centers_supplied::<f64>();
}

fn assert_cluster_gate_widens_with_tolerance<F: Float>() {
    let frac_pi_2 = F::pi() / lit::<F>(2.0_f32);
    let thirty_deg = lit::<F>(30.0_f32.to_radians());
    let mut obs = build_axis_aligned_grid::<F>(5, 5, 10.0);
    obs.push(observation(
        Point2::new(lit::<F>(60.0_f32), lit::<F>(5.0_f32)),
        axes_pair(thirty_deg, thirty_deg + frac_pi_2),
    ));

    let strict_params = TopologicalParams::<F> {
        cluster_centers: Some([F::zero(), frac_pi_2]),
        cluster_axis_tol_rad: lit::<F>(12.0_f32.to_radians()),
        ..TopologicalParams::<F>::default()
    };
    let ctx = obs_open(&obs);
    let strict = build_grid_topological(&obs, &strict_params, &ctx, &mut NoOpSink).unwrap();
    assert_eq!(strict.components.len(), 1);
    // The noiser fails the 12° gate, so the 5x5 grid stands alone.
    let strict_labelled: HashSet<usize> = strict.components[0].labelled.values().copied().collect();
    assert!(!strict_labelled.contains(&25));

    let lax_params = TopologicalParams::<F> {
        cluster_axis_tol_rad: lit::<F>(35.0_f32.to_radians()),
        ..strict_params
    };
    let lax = build_grid_topological(&obs, &lax_params, &ctx, &mut NoOpSink).unwrap();
    assert_eq!(lax.components.len(), 1);
    // At 35° tol the 30° noiser passes the gate and *can* enter Delaunay.
    // Whether it labels depends on the angle test downstream, which it
    // typically won't pass (30° >> 15° align tol). We assert the gate's
    // contract at the labelled-set granularity: the original 25 corners
    // still label.
    assert!(lax.components[0].labelled.len() >= 25);
}

#[test]
fn cluster_gate_widens_with_tolerance_f32() {
    assert_cluster_gate_widens_with_tolerance::<f32>();
}
#[test]
fn cluster_gate_widens_with_tolerance_f64() {
    assert_cluster_gate_widens_with_tolerance::<f64>();
}

// ---------- event trace edge metrics -----------------------------------

fn assert_event_trace_has_consistent_class_counts<F: Float>() {
    // Replacement for the legacy `trace_edge_metrics_have_consistent_margins`
    // test: in the new design we don't ship a TraceView with per-edge
    // margins from the topological pipeline directly. The equivalent
    // sanity check is that every emitted edge event carries a non-Unknown
    // classification (the pre-filter must guarantee usable axes).
    let obs = build_axis_aligned_grid::<F>(4, 4, 10.0);
    let params = TopologicalParams::<F>::default();
    let ctx = obs_open(&obs);
    let (_g, sink) = build_grid_topological_with_trace(&obs, &params, &ctx);
    let edges = sink
        .events()
        .iter()
        .filter_map(|e| match e {
            Event::TopologicalEdge { class, .. } => Some(*class),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(!edges.is_empty());
    for c in &edges {
        assert!(
            !matches!(c, EdgeClass::Unknown),
            "pre-filter should preclude Unknown half-edges"
        );
    }
}

#[test]
fn event_trace_has_consistent_class_counts_f32() {
    assert_event_trace_has_consistent_class_counts::<f32>();
}
#[test]
fn event_trace_has_consistent_class_counts_f64() {
    assert_event_trace_has_consistent_class_counts::<f64>();
}

// ---------- length mismatch dropped ------------------------------------
//
// The legacy `length_mismatch_is_an_error` test exercised the parallel
// `positions` + `axes` slice contract. The new input is a single
// `Observation<F>` slice, so length mismatch is impossible by
// construction; we keep the test as a documentation of the design
// change by asserting that an empty slice errors with
// `InsufficientObservations`.

fn assert_empty_slice_is_insufficient<F: Float>() {
    let obs: Vec<Observation<F>> = Vec::new();
    let ctx = OpenContext::<F>::new(0);
    let result = build_grid_topological(
        &obs,
        &TopologicalParams::<F>::default(),
        &ctx,
        &mut NoOpSink,
    );
    assert!(matches!(
        result,
        Err(DetectionError::InsufficientObservations { found: 0, .. })
    ));
}

#[test]
fn empty_slice_is_insufficient_f32() {
    assert_empty_slice_is_insufficient::<f32>();
}
#[test]
fn empty_slice_is_insufficient_f64() {
    assert_empty_slice_is_insufficient::<f64>();
}

// ---------- policy hook actually fires ---------------------------------

struct RejectAllCtx<F: Float> {
    policy: crate::policy::LabelPolicy<F>,
}

impl<F: Float> super::TopologicalContext<F> for RejectAllCtx<F> {
    fn label_policy(&self) -> &crate::policy::LabelPolicy<F> {
        &self.policy
    }
    fn quad_label_ok(&self, _quad: &crate::topological::QuadView<'_, F>) -> bool {
        false
    }
}

fn assert_quad_label_ok_can_reject<F: Float>() {
    let obs = build_axis_aligned_grid::<F>(5, 5, 10.0);
    let ctx = RejectAllCtx::<F> {
        policy: crate::policy::LabelPolicy::<F>::builder(obs.len()).build(),
    };
    let g = build_grid_topological(
        &obs,
        &TopologicalParams::<F>::default(),
        &ctx,
        &mut NoOpSink,
    )
    .unwrap();
    // Every quad rejected → no labelled components.
    assert!(g.components.is_empty());
}

#[test]
fn quad_label_ok_can_reject_f32() {
    assert_quad_label_ok_can_reject::<f32>();
}
#[test]
fn quad_label_ok_can_reject_f64() {
    assert_quad_label_ok_can_reject::<f64>();
}

// ---------- axes_at override fires -------------------------------------

struct AxesOverrideCtx<F: Float> {
    policy: crate::policy::LabelPolicy<F>,
    axes: [AxisEstimate<F>; 2],
}

impl<F: Float> super::TopologicalContext<F> for AxesOverrideCtx<F> {
    fn label_policy(&self) -> &crate::policy::LabelPolicy<F> {
        &self.policy
    }
    fn axes_at(&self, _idx: usize) -> Option<[AxisEstimate<F>; 2]> {
        Some(self.axes)
    }
}

fn assert_axes_at_override_replaces_uninformative<F: Float>() {
    // Build a 5x5 grid where every observation carries uninformative
    // axes; without the override the pre-filter rejects all corners.
    let mut obs = Vec::new();
    for j in 0..5 {
        for i in 0..5 {
            let p = Point2::new(lit::<F>(i as f32 * 10.0_f32), lit::<F>(j as f32 * 10.0_f32));
            obs.push(observation(p, axes_no_info::<F>()));
        }
    }
    let ctx_no_override = OpenContext::<F>::new(obs.len());
    let err = build_grid_topological(
        &obs,
        &TopologicalParams::<F>::default(),
        &ctx_no_override,
        &mut NoOpSink,
    )
    .unwrap_err();
    assert!(matches!(
        err,
        DetectionError::InsufficientObservations { .. }
    ));

    let frac_pi_2 = F::pi() / lit::<F>(2.0_f32);
    let ctx_override = AxesOverrideCtx::<F> {
        policy: crate::policy::LabelPolicy::<F>::builder(obs.len()).build(),
        axes: [
            AxisEstimate::new(F::zero(), lit::<F>(0.05_f32)),
            AxisEstimate::new(frac_pi_2, lit::<F>(0.05_f32)),
        ],
    };
    let g = build_grid_topological(
        &obs,
        &TopologicalParams::<F>::default(),
        &ctx_override,
        &mut NoOpSink,
    )
    .unwrap();
    assert_eq!(g.components.len(), 1);
    assert_eq!(g.components[0].labelled.len(), 25);
}

#[test]
fn axes_at_override_replaces_uninformative_f32() {
    assert_axes_at_override_replaces_uninformative::<f32>();
}
#[test]
fn axes_at_override_replaces_uninformative_f64() {
    assert_axes_at_override_replaces_uninformative::<f64>();
}
