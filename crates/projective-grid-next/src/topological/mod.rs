//! Topological grid finder (Shu/Brunton/Fiala 2009).
//!
//! Axis-driven cell test — image-free. The pipeline:
//!
//! 1. Delaunay-triangulate the eligible observations.
//! 2. Classify every Delaunay half-edge via the per-observation grid
//!    axes (no image colour sampling needed).
//! 3. Merge triangle pairs whose shared edge is a diagonal into quads
//!    (one quad per chessboard cell).
//! 4. Drop quads with two illegal corners (degree > 4) or extreme
//!    parallelogram shape (`opposing_edge_ratio_max`) or per-component
//!    out-of-band cell scale (`edge_length_ratio_max`).
//! 5. Flood-fill integer `(i, j)` labels through the surviving quad
//!    mesh and rebase each connected component to `(0, 0)`.
//!
//! Pattern hooks via [`TopologicalContext<F>`]; diagnostics via the typed
//! event sink shared with the rest of the crate
//! ([`crate::diagnostics`]).
//!
//! ## Float discipline
//!
//! Every public item is `F: Float`-generic. The legacy crate hard-coded
//! `f32`; the port draws literal constants from a small private helper
//! in [`crate::float`] and uses `F::pi()` for π so an `f64` caller gets
//! the full precision their `Observation<f64>` slice can carry.
//!
//! ## References
//!
//! - **SBF09**: Y. Shu, A. Brunton, M. Fiala — *Chessboard corner finding
//!   using triangulation and topology*, Proc. SPIE 7239
//!   (Electronic Imaging 2009).

mod classify;
mod delaunay;
mod filter;
mod quads;
mod walk;

pub use quads::QuadView;
pub use walk::TopologicalComponent;

use nalgebra::Point2;

use crate::diagnostics::events::Event;
use crate::diagnostics::DiagnosticSink;
use crate::error::DetectionError;
use crate::feature::{AxisEstimate, Observation};
use crate::float::{lit, Float};
use crate::policy::LabelPolicy;
use crate::stats::circular::{angular_dist_pi, wrap_pi};

const MIN_USABLE_FOR_DELAUNAY: usize = 3;

/// Tuning knobs for [`build_grid_topological`].
///
/// `#[non_exhaustive]` so new fields are non-breaking; literal-construction
/// from outside the crate must go through [`Self::default`] + struct-update
/// syntax.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct TopologicalParams<F: Float> {
    /// Maximum angular distance, in radians, between an edge's direction
    /// and a corner's axis for the edge to classify as a *grid edge* at
    /// that corner. Default: `15° = 0.262 rad` — paired with the
    /// pre-Delaunay [`Self::cluster_centers`] gate.
    pub axis_align_tol_rad: F,
    /// Per-axis admission tolerance against [`Self::cluster_centers`],
    /// in radians. Only consulted when `cluster_centers` is `Some`.
    /// Default: `16° = 0.279 rad`.
    pub cluster_axis_tol_rad: F,
    /// Maximum 1σ axis uncertainty (radians) for an observation to enter
    /// Delaunay. Observations whose both axes have `sigma >=
    /// max_axis_sigma_rad` are excluded; classification skips individual
    /// axes whose `sigma >= max_axis_sigma_rad`. Default: `0.6 ≈ 34°`
    /// (matches the legacy `projective-grid::topological::TopologicalParams`
    /// regression-pinned value).
    pub max_axis_sigma_rad: F,
    /// Reject quads whose opposing edges differ in length by more than
    /// this factor (matches the paper's parallelogram test). Default:
    /// `1.5`.
    pub opposing_edge_ratio_max: F,
    /// Reject quads whose perimeter edges fall outside
    /// `[1.0 / edge_length_ratio_max, edge_length_ratio_max] *
    /// component_median_edge_length`. Default: `2.5`. Set to `+inf` to
    /// disable.
    pub edge_length_ratio_max: F,
    /// Discard labelled components with fewer than this many corners.
    /// Default: `4` (one quad of four corners).
    pub min_corners_for_component: usize,
    /// Discard connected quad-mesh components below this size. Default:
    /// `1` (keep all). Set higher to reject isolated noise quads.
    pub min_quads_per_component: usize,
    /// Optional global grid-direction centers `[theta0, theta1]` (both in
    /// `[0, π)`). When `Some`, every observation must have at least one
    /// informative axis within [`Self::cluster_axis_tol_rad`] of either
    /// center to enter Delaunay. When `None`, the gate is skipped,
    /// preserving the legacy crate's behaviour as a standalone primitive.
    pub cluster_centers: Option<[F; 2]>,
}

impl<F: Float> Default for TopologicalParams<F> {
    fn default() -> Self {
        Self {
            axis_align_tol_rad: lit::<F>(15.0_f32.to_radians()),
            cluster_axis_tol_rad: lit::<F>(16.0_f32.to_radians()),
            max_axis_sigma_rad: lit::<F>(0.6_f32),
            opposing_edge_ratio_max: lit::<F>(1.5_f32),
            edge_length_ratio_max: lit::<F>(2.5_f32),
            min_corners_for_component: 4,
            min_quads_per_component: 1,
            cluster_centers: None,
        }
    }
}

/// Top-level result of [`build_grid_topological`].
///
/// `#[non_exhaustive]` so a future component-level diagnostic (e.g. cell
/// size estimate) can be added without breaking downstream consumers.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct TopologicalGrid<F: Float> {
    /// One entry per connected component of the surviving quad mesh.
    /// Components are returned in the order the walker discovered them,
    /// which is the BFS order on the quad index.
    pub components: Vec<TopologicalComponent>,
    /// Carries the Float parameter through the result struct so callers
    /// can write `let g: TopologicalGrid<f64> = …`.
    pub(crate) _phantom: std::marker::PhantomData<F>,
}

/// Pattern-aware hooks consulted by the topological pipeline.
///
/// Default impls return permissive answers so a vanilla `OpenContext`
/// (provided below) is a drop-in caller. Pattern-aware consumers
/// (chessboard, marker board) override the hooks to filter on per-corner
/// tags, parity, or eligibility.
pub trait TopologicalContext<F: Float> {
    /// The active label policy (parity / tag / eligibility). The
    /// pipeline consults it through this hook so a consumer crate may
    /// share a single policy object across multiple pipeline calls.
    fn label_policy(&self) -> &LabelPolicy<F>;

    /// Optional per-observation axis override. Returning `None` falls
    /// back to `Observation.axes`.
    ///
    /// Provided so a consumer can substitute cluster-refined axes for
    /// the raw per-corner axes without rewriting the observation slice.
    #[allow(unused_variables)]
    fn axes_at(&self, idx: usize) -> Option<[AxisEstimate<F>; 2]> {
        None
    }

    /// Whether a candidate quad is acceptable under the consumer's
    /// pattern rules. Consulted post-filter, pre-commit by the walker
    /// with the proposed `[Coord; 4]` already populated.
    ///
    /// Default: always accept. A chessboard consumer overrides this to
    /// enforce parity consistency between corner tags and the quad's
    /// label-parity assignment.
    #[allow(unused_variables)]
    fn quad_label_ok(&self, quad: &QuadView<'_, F>) -> bool {
        true
    }

    /// Whether observation `idx` is eligible to participate at all.
    ///
    /// Default: always eligible (the bench harness's
    /// [`OpenContext`] uses this default; pattern-aware consumers
    /// typically override to read through to
    /// `self.label_policy().is_eligible(idx)`).
    #[allow(unused_variables)]
    fn corner_eligible(&self, idx: usize) -> bool {
        true
    }
}

/// Zero-config drop-in [`TopologicalContext`]: every observation is
/// eligible, every quad is acceptable, the label policy is empty
/// (no parity, no tags).
///
/// Used by the bench harness and by integration tests. Pattern-specific
/// consumers (chessboard, marker) provide their own context type.
#[derive(Debug, Clone)]
pub struct OpenContext<F: Float> {
    policy: LabelPolicy<F>,
}

impl<F: Float> OpenContext<F> {
    /// Construct an open context covering `n_observations` features.
    /// The underlying policy treats every observation as eligible.
    pub fn new(n_observations: usize) -> Self {
        Self {
            policy: LabelPolicy::<F>::builder(n_observations).build(),
        }
    }
}

impl<F: Float> TopologicalContext<F> for OpenContext<F> {
    fn label_policy(&self) -> &LabelPolicy<F> {
        &self.policy
    }
}

/// Build labelled grid components from a slice of observations.
///
/// Returns one [`TopologicalComponent`] per connected component of the
/// surviving quad mesh. The bench harness wires component merging as a
/// post-stage (Phase 4); the topological core itself is pure
/// label-extraction.
///
/// # Errors
///
/// * [`DetectionError::InsufficientObservations`] when fewer than three
///   observations survive the eligibility / axis-validity / optional
///   cluster-center pre-filter (Delaunay needs three points).
/// * [`DetectionError::DegenerateCloud`] when Delaunay produces zero
///   triangles (e.g. all eligible observations are collinear).
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "info",
        skip_all,
        fields(num_observations = observations.len()),
    )
)]
pub fn build_grid_topological<F: Float, C: TopologicalContext<F>>(
    observations: &[Observation<F>],
    params: &TopologicalParams<F>,
    ctx: &C,
    sink: &mut impl DiagnosticSink<F>,
) -> Result<TopologicalGrid<F>, DetectionError> {
    use crate::diagnostics::events::Stage;
    sink.emit(Event::StageStarted {
        stage: Stage::Topological,
    });

    // Resolve per-observation positions and axes, honouring ctx overrides.
    let positions: Vec<Point2<F>> = observations.iter().map(|o| o.position).collect();
    let mut axes: Vec<[AxisEstimate<F>; 2]> = (0..observations.len())
        .map(|idx| ctx.axes_at(idx).unwrap_or(observations[idx].axes))
        .collect();

    // Cap per-axis `sigma` at `max_axis_sigma_rad`: any axis whose uncertainty
    // exceeds the threshold gets sigma = π, the workspace convention for
    // "uninformative." Downstream `is_informative()` and finite-sigma checks
    // in `classify::nearest_axis_at_corner` then correctly skip those axes
    // while still allowing the corner to participate via its other (good) axis.
    let sigma_max = params.max_axis_sigma_rad;
    for pair in axes.iter_mut() {
        for axis in pair.iter_mut() {
            if !axis.sigma.is_finite() || axis.sigma >= sigma_max {
                axis.sigma = F::pi();
            }
        }
    }

    // Pre-filter: at least one axis with `sigma < max_axis_sigma_rad`,
    // plus caller eligibility, plus optional cluster-center gate.
    let usable_mask = usable_mask(&axes, params, ctx);
    let n_usable = usable_mask.iter().filter(|&&b| b).count();
    if n_usable < MIN_USABLE_FOR_DELAUNAY {
        return Err(DetectionError::InsufficientObservations {
            found: n_usable,
            required: MIN_USABLE_FOR_DELAUNAY,
        });
    }

    let triangulation = triangulate_usable(&positions, &usable_mask);
    if triangulation.num_tri() == 0 {
        return Err(DetectionError::DegenerateCloud);
    }

    // Classify every half-edge.
    let edge_kinds =
        classify::classify_all_edges(&positions, &axes, &triangulation, params.axis_align_tol_rad);

    // Emit one TopologicalEdge event per half-edge. CounterStats will then
    // aggregate per-edge classes and per-triangle composition.
    for (e, &class) in edge_kinds.iter().enumerate() {
        sink.emit(Event::TopologicalEdge {
            triangle: e / 3,
            half_edge: e % 3,
            class,
        });
    }

    // Merge triangle pairs sharing a diagonal whose other edges are grid.
    let raw_quads = quads::merge_triangle_pairs(&triangulation, &edge_kinds, &positions);

    // Topological + geometric filtering. Emit one TopologicalQuad event
    // per merged quad.
    let decisions = filter::filter_quad_decisions(
        &raw_quads,
        &positions,
        params.opposing_edge_ratio_max,
        params.edge_length_ratio_max,
    );
    for (id, d) in decisions.iter().enumerate() {
        sink.emit(Event::TopologicalQuad {
            id,
            kept: d.kept,
            reason: d.rejection,
        });
    }
    let kept_quads: Vec<quads::Quad> = decisions
        .into_iter()
        .filter(|d| d.kept)
        .map(|d| d.quad)
        .collect();

    // Flood-fill labels per connected component.
    let components = walk::label_components(
        &kept_quads,
        &positions,
        params.min_quads_per_component,
        params.min_corners_for_component,
        ctx,
    );
    for (id, c) in components.iter().enumerate() {
        sink.emit(Event::ComponentLabelled {
            id,
            n_labels: c.labelled.len(),
        });
    }

    Ok(TopologicalGrid {
        components,
        _phantom: std::marker::PhantomData,
    })
}

/// Whether `theta` (modulo π) is within `tol` of either cluster center.
#[inline]
fn axis_passes_cluster<F: Float>(angle: F, sigma: F, centers: &[F; 2], tol: F) -> bool {
    if !sigma.is_finite() || sigma >= F::pi() {
        return false;
    }
    let wrapped = wrap_pi(angle);
    let d0 = angular_dist_pi(wrapped, centers[0]);
    let d1 = angular_dist_pi(wrapped, centers[1]);
    let min_d = if d0 < d1 { d0 } else { d1 };
    min_d < tol
}

/// Build the per-observation eligibility mask. An observation is usable
/// when (a) `ctx.corner_eligible` admits it, (b) at least one of its two
/// axes has `sigma < max_axis_sigma_rad`, and (c) when `cluster_centers`
/// is set, at least one informative axis lies within
/// `cluster_axis_tol_rad` of either center.
fn usable_mask<F: Float, C: TopologicalContext<F>>(
    axes: &[[AxisEstimate<F>; 2]],
    params: &TopologicalParams<F>,
    ctx: &C,
) -> Vec<bool> {
    let tol = params.cluster_axis_tol_rad;
    let sigma_max = params.max_axis_sigma_rad;
    axes.iter()
        .enumerate()
        .map(|(idx, a)| {
            if !ctx.corner_eligible(idx) {
                return false;
            }
            let sigma_ok = (a[0].sigma.is_finite() && a[0].sigma < sigma_max)
                || (a[1].sigma.is_finite() && a[1].sigma < sigma_max);
            if !sigma_ok {
                return false;
            }
            match params.cluster_centers.as_ref() {
                None => true,
                Some(c) => {
                    axis_passes_cluster(a[0].angle, a[0].sigma, c, tol)
                        || axis_passes_cluster(a[1].angle, a[1].sigma, c, tol)
                }
            }
        })
        .collect()
}

/// Triangulate only the usable observations and remap triangle vertex
/// indices back into the global `positions` index space.
fn triangulate_usable<F: Float>(
    positions: &[Point2<F>],
    usable: &[bool],
) -> delaunay::Triangulation {
    let mut packed_to_global: Vec<usize> = Vec::with_capacity(positions.len());
    let mut packed_positions: Vec<Point2<F>> = Vec::with_capacity(positions.len());
    for (i, (&u, &p)) in usable.iter().zip(positions.iter()).enumerate() {
        if u {
            packed_to_global.push(i);
            packed_positions.push(p);
        }
    }
    let mut triangulation = delaunay::triangulate(&packed_positions);
    // After triangulation, the indices reference the packed slice. But the
    // downstream stages (classify, merge, walk) consume positions and axes
    // that are still global. We need to either remap or pass positions in
    // the packed order. The legacy approach remaps `triangles` to global
    // indices but keeps `halfedges` as offsets in the SAME (packed) edge
    // index space — half-edges are not vertex indices, they're flat offsets
    // into `triangles`, so they stay valid post-remap. We mirror that here.
    for v in triangulation.triangles.iter_mut() {
        *v = packed_to_global[*v];
    }
    triangulation
}

/// Build labelled components in a single call that owns its own
/// `RecordingSink`. Returns both the grid and the recorded event trace,
/// which the bench harness uses for trace-style overlays and JSON dumps.
///
/// This is a convenience wrapper around [`build_grid_topological`]; it
/// exists because most non-production callers want both the result and
/// the events in one go.
pub fn build_grid_topological_with_trace<F: Float, C: TopologicalContext<F>>(
    observations: &[Observation<F>],
    params: &TopologicalParams<F>,
    ctx: &C,
) -> (
    Result<TopologicalGrid<F>, DetectionError>,
    crate::diagnostics::RecordingSink<F>,
) {
    let mut sink = crate::diagnostics::RecordingSink::<F>::new();
    let result = build_grid_topological(observations, params, ctx, &mut sink);
    (result, sink)
}

#[cfg(test)]
mod tests;
