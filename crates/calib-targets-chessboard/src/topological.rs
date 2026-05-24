//! Topological dispatch path for the chessboard detector.
//!
//! This module is the adapter between two layers:
//!
//! - `projective-grid-next`, which is image-free and labels connected
//!   quad-mesh components from oriented features (positions + per-corner
//!   axis hints) via [`detect_grid_all`](projective_grid_next::detect_grid_all);
//! - `calib-targets-chessboard`, which owns ChESS corner filtering, recall
//!   boosters, final canonicalisation, and the public
//!   [`ChessboardDetection`](crate::ChessboardDetection) type.
//!
//! The production path intentionally remains one path. Blog overlays use
//! [`trace_topological`] for intermediate `projective-grid` stages; benchmark
//! reports use the optional `tracing` feature to time the same functions rather
//! than a second timed implementation.
//!
//! ## Phase E.1a — grid-build call migrated to `projective-grid-next`
//!
//! Production [`detect_all_topological`] now calls
//! [`projective_grid_next::detect_grid_all`] with
//! [`SquareAlgorithm::Topological`](projective_grid_next::SquareAlgorithm::Topological).
//! The output is bridged back to legacy
//! [`projective_grid::ComponentInput`] views so the existing recovery
//! pipeline ([`recovery::merge`](self::recovery), boosters, geometry
//! check) is byte-identical with the pre-migration version. Validation
//! and over-residual drops in the new pipeline are disabled (tolerances
//! pushed to `+inf`) because the chessboard owns its own validation
//! downstream; the new path is asked solely to produce labelled
//! `(coord -> source_index)` components.
//!
//! [`trace_topological`] keeps using the legacy
//! [`projective_grid::topological`] crate — it is a debug-only diagnostic
//! surface and the legacy trace types remain richer than what
//! `projective-grid-next` exposes today. Migration of the trace path is
//! deferred to a later phase.

mod inputs;
mod recovery;

use crate::corner::ChessCorner;
use calib_targets_core::axis_estimate_to_next;
use projective_grid::topological::trace::{build_grid_topological_trace, TopologicalTrace};
use projective_grid::{
    merge_components_local, AxisClusterCenters, ComponentInput,
    TopologicalParams as LegacyTopologicalParams,
};
use projective_grid_next::{
    detect_grid_all, DetectionParams as NextDetectionParams, DetectionRequest, Evidence,
    LatticeKind, OrientedFeature, PointFeature, SquareAlgorithm,
    TopologicalParams as NextTopologicalParams, ValidateParams as NextValidateParams,
};
use std::collections::HashMap;

use crate::cluster::ClusterCenters;
use crate::detector::ChessboardDetection;
use crate::params::DetectorParams;

use self::inputs::topological_inputs;
use self::recovery::{
    build_topological_detections, clustered_augs, recover_topological_components,
};

#[inline]
fn axis_centers_to_topological(centers: Option<ClusterCenters>) -> Option<AxisClusterCenters> {
    centers.map(|c| AxisClusterCenters::new(c.theta0, c.theta1))
}

/// Build a `projective-grid-next` [`NextDetectionParams`] that mirrors the
/// chessboard adapter's intent for the topological grid finder.
///
/// The mapping from the legacy
/// [`projective_grid::TopologicalParams`](LegacyTopologicalParams) to the
/// new [`NextTopologicalParams`]:
///
/// * `axis_align_tol_rad`, `max_axis_sigma_rad`, `min_quads_per_component`,
///   and `cluster_axis_tol_rad` map field-for-field.
/// * `edge_ratio_max` (legacy opposing-edge parallelogram cap) maps to
///   `opposing_edge_ratio_max`.
/// * `quad_edge_max_rel` (legacy per-component upper edge-length band)
///   maps to `edge_length_ratio_max`. The legacy lower band
///   `quad_edge_min_rel` is unconditionally `0.0` in the chessboard
///   defaults; the new pipeline applies a symmetric band
///   `[1 / edge_length_ratio_max, edge_length_ratio_max]`. With the
///   default `quad_edge_max_rel = 1.8` this introduces an implicit
///   `lower = 1 / 1.8 ≈ 0.556 * component_median` floor. On the
///   regression set this is below the smallest legitimate quad seen so
///   far; the assumption is verified by the topo_grid + testdata
///   regression gates.
/// * `axis_cluster_centers`: the per-frame two-cluster centers are
///   forwarded as a `[theta0, theta1]` pair.
///
/// The new pipeline also runs a post-grow validation + fit-residual
/// drop. Both are disabled here (tolerances pushed to `+inf`,
/// edge-parity check off, `max_residual_px = +inf`) because the
/// chessboard owns its own geometry check downstream — the migration
/// must produce the same labelled components the legacy
/// `build_grid_topological` produced.
fn detection_params_for_topological(
    legacy: &LegacyTopologicalParams,
    clustered_centers: Option<ClusterCenters>,
) -> NextDetectionParams<f32> {
    let mut topo = NextTopologicalParams::<f32>::default();
    topo.axis_align_tol_rad = legacy.axis_align_tol_rad;
    topo.max_axis_sigma_rad = legacy.max_axis_sigma_rad;
    topo.opposing_edge_ratio_max = legacy.edge_ratio_max;
    // Per-component cell-size filter:
    //   * Legacy uses `[quad_edge_min_rel, quad_edge_max_rel] * median`
    //     with `quad_edge_min_rel = 0.0` (lower band disabled) and
    //     `quad_edge_max_rel = 1.8` (upper-only band).
    //   * The new pipeline only exposes a *symmetric* band
    //     `[1 / edge_length_ratio_max, edge_length_ratio_max] * median`,
    //     so passing `quad_edge_max_rel` directly would also enforce a
    //     lower band of `1 / 1.8 ≈ 0.556 * median` that the legacy did
    //     NOT enforce. Empirically this regresses the GeminiChess2
    //     regression gate by ~3 corners (drops short quads on
    //     perspective-stretched boards that legacy admitted).
    //
    // We therefore disable the new per-component cell-size filter
    // entirely (`+inf` is the documented "disable" sentinel). The
    // chessboard's downstream `run_geometry_check` (line collinearity +
    // local-H + axis parity) catches the double-cell-hop quads the
    // legacy upper band was guarding against. Restoring the legacy
    // upper-only semantics in the new pipeline requires an asymmetric
    // band knob on `projective_grid_next::TopologicalParams`; that's a
    // follow-up to Phase E.1a, not a migration prerequisite.
    topo.edge_length_ratio_max = f32::INFINITY;
    topo.min_quads_per_component = legacy.min_quads_per_component;
    // `min_corners_for_component`: new field with no legacy equivalent;
    // 4 corners = 1 quad matches the legacy "keep all quad-mesh
    // components" intent. (Legacy had only `min_quads_per_component`.)
    topo.min_corners_for_component = 4;
    topo.cluster_axis_tol_rad = legacy.cluster_axis_tol_rad;
    topo.axis_cluster_centers = clustered_centers.map(|c| [c.theta0, c.theta1]);

    // Disable the post-grow validation: the chessboard runs its own
    // geometry check in `build_topological_detections::run_geometry_check`
    // and its own per-component boosters first. Disabling here means a
    // corner the new pipeline would have flagged still gets a chance to
    // survive the chessboard's downstream stages.
    let mut validate = NextValidateParams::<f32>::default();
    validate.line_tol_rel = f32::INFINITY;
    validate.local_h_tol_rel = f32::INFINITY;
    validate.edge_length_band_rel = f32::INFINITY;
    validate.enable_edge_parity_check = false;

    NextDetectionParams::<f32>::default()
        .with_algorithm(SquareAlgorithm::Topological)
        .with_topological(topo)
        .with_validate(validate)
        // Disable the post-fit residual drop in the new pipeline for the
        // same reason: chessboard's `run_geometry_check` owns residual
        // gating downstream.
        .with_max_residual_px(f32::INFINITY)
}

/// Build the new-crate oriented-feature slice from the chessboard's
/// `(positions, axes)` shape. `source_index` is the slice index, so the
/// returned `GridEntry.source_index` is directly usable as a
/// `positions[]` index by the downstream legacy recovery stages.
fn build_oriented_features(
    positions: &[nalgebra::Point2<f32>],
    axes: &[[projective_grid::AxisEstimate; 2]],
) -> Vec<OrientedFeature<f32, 2>> {
    debug_assert_eq!(positions.len(), axes.len());
    positions
        .iter()
        .zip(axes.iter())
        .enumerate()
        .map(|(i, (&pos, axes))| {
            OrientedFeature::new(
                PointFeature::new(i, pos),
                [
                    axis_estimate_to_next(axes[0]),
                    axis_estimate_to_next(axes[1]),
                ],
            )
        })
        .collect()
}

/// Run the topological pipeline and return one [`ChessboardDetection`] per
/// surviving labelled component.
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "info",
        skip_all,
        fields(num_corners = corners.len()),
    )
)]
pub fn detect_all_topological(
    corners: &[ChessCorner],
    params: &DetectorParams,
) -> Vec<ChessboardDetection> {
    if corners.is_empty() {
        return Vec::new();
    }

    // Hoist clustering: chessboard-v2 uses `cluster_axes` as a precision
    // bedrock before its seed-and-grow. Topological used to skip this and
    // pay the cost in spurious-edge admissions; we now compute centers
    // once up front, gate Delaunay through them, and reuse the same
    // `(augs, centers)` pair for booster recovery (no re-clustering).
    let (base_augs, clustered_centers) = clustered_augs(corners, params);

    let inputs = topological_inputs(corners, params);
    if inputs.usable_count < params.min_labeled_corners {
        return Vec::new();
    }

    // Build the new-crate input shape. `params.tuning.topological` carries
    // the legacy field names; `detection_params_for_topological` translates
    // them into `projective-grid-next`'s sub-config layout.
    //
    // Note on `cluster_axis_tol_rad`: keep the default 16° baked into
    // `NextTopologicalParams::default`. Do not reuse
    // `params.tuning.cluster_tol_deg` (12°) — chessboard-v2's cluster gate
    // has a sigma bonus and a booster fallback that topological lacks;
    // matching the 12° literally regresses Gemini2.
    let next_features = build_oriented_features(&inputs.positions, &inputs.axes);
    let next_params =
        detection_params_for_topological(&params.tuning.topological, clustered_centers);
    let request = DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Oriented2(&next_features),
        None,
        next_params,
    );
    let report = match detect_grid_all(request) {
        Ok(r) => r,
        // `InsufficientEvidence` / `DegenerateGeometry` /
        // `UnsupportedCombination` all collapse to "no components". The
        // legacy path mapped its `TopologicalError` variants the same way.
        Err(_) => return Vec::new(),
    };
    if report.solutions.is_empty() {
        return Vec::new();
    }

    // Bridge the new-crate output back to the legacy
    // `ComponentInput<'_>` shape so the existing recovery pipeline is
    // byte-identical with the pre-migration version.
    //
    // The `&HashMap` borrows in `ComponentInput` outlive the call as long
    // as the owning `Vec<HashMap<...>>` is alive — hence the two-vector
    // split: first allocate the maps (owned), then collect references to
    // them.
    let labelled_maps: Vec<HashMap<(i32, i32), usize>> = report
        .solutions
        .iter()
        .map(|sol| {
            sol.grid
                .entries
                .iter()
                .map(|e| ((e.coord.u, e.coord.v), e.source_index))
                .collect()
        })
        .collect();
    let component_views: Vec<ComponentInput<'_>> = labelled_maps
        .iter()
        .map(|labelled| ComponentInput {
            labelled,
            positions: &inputs.positions,
        })
        .collect();

    #[cfg(feature = "tracing")]
    let merged = {
        let _span = tracing::debug_span!(
            "topological_initial_component_merge",
            num_components = component_views.len()
        )
        .entered();
        merge_components_local(&component_views, &params.tuning.component_merge)
    };
    #[cfg(not(feature = "tracing"))]
    let merged = merge_components_local(&component_views, &params.tuning.component_merge);

    let final_components = recover_topological_components(
        &merged.components,
        &inputs.positions,
        &base_augs,
        clustered_centers,
        params,
    );

    build_topological_detections(
        final_components,
        &inputs.positions,
        &base_augs,
        clustered_centers,
        params,
    )
}

/// Run the same topological input adaptation as [`detect_all_topological`],
/// but return the full projective-grid trace instead of detections.
///
/// Corners that fail the chessboard strength / fit pre-filter are passed to
/// `projective-grid` with no-information axes, matching the production
/// topological dispatch path.
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "info",
        skip_all,
        fields(num_corners = corners.len()),
    )
)]
pub fn trace_topological(
    corners: &[ChessCorner],
    params: &DetectorParams,
) -> Result<TopologicalTrace, projective_grid::TopologicalError> {
    let inputs = topological_inputs(corners, params);
    let (_augs, clustered_centers) = clustered_augs(corners, params);
    let mut topo_params = params.tuning.topological;
    topo_params.axis_cluster_centers = axis_centers_to_topological(clustered_centers);
    build_grid_topological_trace(&inputs.positions, &inputs.axes, &topo_params)
}
