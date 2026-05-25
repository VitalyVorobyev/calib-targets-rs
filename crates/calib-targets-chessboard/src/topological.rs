//! Topological dispatch path for the chessboard detector.
//!
//! This module is the adapter between two layers:
//!
//! - `projective-grid`, which is image-free and labels connected
//!   quad-mesh components from oriented features (positions + per-corner
//!   axis hints) via [`detect_grid_all`](projective_grid::detect_grid_all);
//! - `calib-targets-chessboard`, which owns ChESS corner filtering, recall
//!   boosters, final canonicalisation, and the public
//!   [`ChessboardDetection`](crate::ChessboardDetection) type.
//!
//! The production path intentionally remains one path. Diagnostics use
//! [`trace_topological`] for the same `projective-grid` topological
//! detector path; benchmark reports use the optional `tracing` feature to time
//! the same functions rather than a second timed implementation.
//!
//! Production [`detect_all_topological`] now calls
//! [`projective_grid::detect_grid_all`] with
//! [`SquareAlgorithm::Topological`](projective_grid::SquareAlgorithm::Topological).
//! The output is bridged into the advanced
//! [`projective-grid`](projective_grid) component-merge view so the existing recovery
//! pipeline ([`recovery::merge`](self::recovery), boosters, geometry
//! check) stays byte-identical with the pre-migration version. Validation
//! and over-residual drops in the new pipeline are disabled (tolerances
//! pushed to `+inf`) because the chessboard owns its own validation
//! downstream; the new path is asked solely to produce labelled
//! `(coord -> source_index)` components.
//!
//! [`trace_topological`] uses the same `projective-grid` production
//! detector path and returns a compact serializable trace of the final
//! labelled components.

mod inputs;
mod recovery;

use crate::corner::ChessCorner;
use calib_targets_core::{axis_estimate_to_next, AxisEstimate};
use projective_grid::detect::advanced::square::component_merge::{
    merge_components_local, ComponentInput,
};
use projective_grid::detect::advanced::square::topological_trace::{
    build_grid_topological_trace, TopologicalTrace, TopologicalTraceError,
};
use projective_grid::detect::ValidateParams as NextValidateParams;
use projective_grid::{
    detect_grid_all, DetectionParams as NextDetectionParams, DetectionRequest, Evidence,
    LatticeKind, OrientedFeature, PointFeature, SquareAlgorithm,
    TopologicalParams as NextTopologicalParams,
};
use std::collections::HashMap;

use crate::cluster::ClusterCenters;
use crate::detector::ChessboardDetection;
use crate::params::DetectorParams;

use self::inputs::topological_inputs;
use self::recovery::{
    build_topological_detections, clustered_augs, recover_topological_components,
};

/// Build a `projective-grid` [`NextDetectionParams`] for the
/// chessboard adapter's topological grid finder.
///
/// The new pipeline also runs a post-grow validation + fit-residual
/// drop. Both are disabled here (tolerances pushed to `+inf`,
/// `max_residual_px = +inf`) because the
/// chessboard owns its own geometry check downstream — the migration
/// must preserve the labelled components produced by the topological
/// graph builder.
fn detection_params_for_topological(
    topological: &NextTopologicalParams<f32>,
    clustered_centers: Option<ClusterCenters>,
) -> NextDetectionParams<f32> {
    let mut topo = *topological;
    topo.axis_cluster_centers = clustered_centers.map(|c| [c.theta0, c.theta1]);

    // Disable the post-grow validation: the chessboard runs its own
    // geometry check in `build_topological_detections::run_geometry_check`
    // and its own per-component boosters first. Disabling here means a
    // corner the new pipeline would have flagged still gets a chance to
    // survive the chessboard's downstream stages.
    let validate = NextValidateParams::<f32>::default()
        .with_line_tol_rel(f32::INFINITY)
        .with_local_h_tol_rel(f32::INFINITY)
        .with_edge_length_band_rel(f32::INFINITY);

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
/// `positions[]` index by the downstream recovery stages.
fn build_oriented_features(
    positions: &[nalgebra::Point2<f32>],
    axes: &[[AxisEstimate; 2]],
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
    // the chessboard tuning field names; `detection_params_for_topological` translates
    // them into `projective-grid`'s sub-config layout.
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
        // The topological path maps these cases to "no components".
        Err(_) => return Vec::new(),
    };
    if report.solutions.is_empty() {
        return Vec::new();
    }

    // Bridge the new-crate output into the advanced component-merge
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
/// but return a compact topological trace instead of detections.
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
) -> Result<TopologicalTrace, TopologicalTraceError> {
    let inputs = topological_inputs(corners, params);
    let (_augs, clustered_centers) = clustered_augs(corners, params);
    let mut topo_params = params.tuning.topological;
    topo_params.axis_cluster_centers = clustered_centers.map(|c| [c.theta0, c.theta1]);
    let next_features = build_oriented_features(&inputs.positions, &inputs.axes);
    build_grid_topological_trace(&next_features, topo_params)
}
