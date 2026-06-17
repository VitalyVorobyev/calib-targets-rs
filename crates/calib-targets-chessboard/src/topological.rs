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
//! The facade runs the shared local-geometry component merge itself (the
//! same `merge_components_local` the seed-and-grow facade uses), so
//! `report.solutions` arrives already merged. The adapter consumes those
//! merged labelled components directly and feeds them to the chessboard's
//! own recovery pipeline (boosters, post-recovery merge, geometry check).
//! Validation and over-residual drops in the facade are disabled
//! (tolerances pushed to `+inf`) because the chessboard owns its own
//! validation downstream; the facade is asked solely to produce labelled
//! `(coord -> source_index)` components and to reunite them in label space.
//!
//! The facade merge runs with `LocalMergeParams::default()`. The
//! chessboard's `tuning.component_merge` is `LocalMergeParams::default()`
//! for every shipping config (no preset or sweep overrides it), so the
//! merged components — and hence production output — are byte-identical to
//! the prior chessboard-side merge.
//!
//! [`trace_topological`] uses the same `projective-grid` production
//! detector path and returns a compact serializable trace of the final
//! labelled components.

mod inputs;
mod recovery;

use crate::corner::ChessCorner;
use calib_targets_core::{axis_estimate_to_next, AxisEstimate};
use projective_grid::detect::ValidateParams as NextValidateParams;
use projective_grid::topological::trace::{
    build_grid_topological_trace, TopologicalTrace, TopologicalTraceError,
};
use projective_grid::{
    detect_grid_all, DetectionParams as NextDetectionParams, DetectionRequest, Evidence,
    LatticeKind, OrientedFeature, PointFeature, RecoveryParams, RecoverySchedule, SquareAlgorithm,
    TopologicalParams as NextTopologicalParams,
};
use std::borrow::Cow;
use std::collections::HashMap;

use crate::cluster::ClusterCenters;
use crate::detector::ChessboardDetection;
use crate::params::{DetectorParams, OrientationSource};

use self::inputs::{corners_with_synthesized_axes, topological_inputs};
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
    topological: &NextTopologicalParams,
    clustered_centers: Option<ClusterCenters>,
    orientation_source: OrientationSource,
) -> NextDetectionParams {
    let mut topo = *topological;
    topo.axis_cluster_centers = clustered_centers.map(|c| [c.theta0, c.theta1]);

    match orientation_source {
        OrientationSource::ChessAxes => {
            // ChESS axes are accurate enough that the walk alone reaches full
            // recall, and the chessboard owns its own validation + booster
            // recovery downstream. Disable the facade's post-grow validation,
            // post-fit residual drop, and recovery schedule (tolerances → +inf,
            // recovery `Off`) so the facade adds nothing and production output
            // stays byte-identical: a corner the facade would have flagged still
            // gets a chance to survive the chessboard's downstream stages.
            let validate = NextValidateParams::default()
                .with_line_tol_rel(f32::INFINITY)
                .with_local_h_tol_rel(f32::INFINITY)
                .with_edge_length_band_rel(f32::INFINITY);
            NextDetectionParams::default()
                .with_algorithm(SquareAlgorithm::Topological)
                .with_topological(topo)
                .with_validate(validate)
                .with_max_residual_px(f32::INFINITY)
                .with_recovery(RecoverySchedule::Off)
        }
        OrientationSource::NeighbourEdges => {
            // Neighbour-edge synthesized axes are noisier, so the walk alone
            // reaches a smaller component. Run the facade's geometry-only
            // recovery — purpose-built for synthesized-axis grids — to lift it to
            // full recall. `Auto` would stay off here (the Oriented2 dispatch
            // reports `synthesized_axes = false`), so force it `On`.
            //
            // Gating split: keep the *local* validation (line collinearity +
            // local-H residual, both cell-size-relative) at its default, because
            // the recovery's per-sweep revalidation reuses it and that is what
            // rejects the over-extension a fully-disabled validate would attach
            // one cell past the board edge into the margin (measured on the
            // canonical reference frame). Disable only the *global* post-fit
            // residual drop (`max_residual_px = +inf`): a single global-homography
            // reprojection gate rejects whole legitimate but perspective-/
            // distortion-warped grids (e.g. the AI-generated GeminiChess set) —
            // exactly why the ChESS path disables it and leans on local checks.
            NextDetectionParams::default()
                .with_algorithm(SquareAlgorithm::Topological)
                .with_topological(topo)
                .with_max_residual_px(f32::INFINITY)
                .with_recovery(RecoverySchedule::On(RecoveryParams::default()))
        }
    }
}

/// Build the new-crate oriented-feature slice from the chessboard's
/// `(positions, axes)` shape. `source_index` is the slice index, so the
/// returned `GridEntry.source_index` is directly usable as a
/// `positions[]` index by the downstream recovery stages.
fn build_oriented_features(
    positions: &[nalgebra::Point2<f32>],
    axes: &[[AxisEstimate; 2]],
) -> Vec<OrientedFeature<2>> {
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

    // Resolve the per-corner axis source once, up front. `ChessAxes` uses the
    // corners as handed in (borrowed — the path stays byte-identical to before).
    // `NeighbourEdges` synthesizes the two grid directions from neighbour-edge
    // geometry and stamps them onto a cloned corner view; every downstream stage
    // (clustering, boosters, geometry check) then consumes them exactly as it
    // would ChESS axes. There is no longer a separate orientation-free code
    // path — neighbour-edge orientation is just another way to populate
    // `ChessCorner.axes`, which is what unlocks the recovery boosters for it.
    let corners: Cow<'_, [ChessCorner]> = match params.orientation_source {
        OrientationSource::ChessAxes => Cow::Borrowed(corners),
        OrientationSource::NeighbourEdges => Cow::Owned(corners_with_synthesized_axes(corners)),
    };
    let corners: &[ChessCorner] = &corners;

    // Hoist clustering: seed-and-grow uses `cluster_axes` as a precision
    // bedrock before its seed-and-grow. Topological used to skip this and
    // pay the cost in spurious-edge admissions; we now compute centers
    // once up front, gate Delaunay through them, and reuse the same
    // `(augs, centers)` pair for booster recovery (no re-clustering).
    // Clustering runs over whichever axes the corner view carries, so the
    // centers are `Some(..)` for both orientation sources and the booster pass
    // in `recovery` fires for both — see [`recover_topological_components`].
    let (base_augs, clustered_centers) = clustered_augs(corners, params);

    let inputs = topological_inputs(corners, params);
    if inputs.usable_count < params.min_labeled_corners {
        return Vec::new();
    }

    let tuning = params.effective_tuning();

    // Build the new-crate input shape. `tuning.topological` carries
    // the chessboard tuning field names; `detection_params_for_topological` translates
    // them into `projective-grid`'s sub-config layout.
    //
    // Note on `cluster_axis_tol_rad`: keep the default 16° baked into
    // `NextTopologicalParams::default`. Do not reuse
    // `tuning.cluster_tol_deg` (12°) — seed-and-grow's cluster gate
    // has a sigma bonus and a booster fallback that topological lacks;
    // matching the 12° literally regresses Gemini2.
    //
    // The facade's recovery + validation gating differs by orientation source
    // (see `detection_params_for_topological`): ChESS axes leave it to the
    // chessboard downstream (byte-identical); neighbour-edge synthesized axes
    // run the facade's geometry-only recovery to reach full recall.
    let next_params = detection_params_for_topological(
        &tuning.topological,
        clustered_centers,
        params.orientation_source,
    );
    // Both orientation sources now feed `Evidence::Oriented2` built from
    // `inputs.axes` — ChESS-derived in `ChessAxes` mode, neighbour-edge
    // synthesized in `NeighbourEdges` mode. The point cloud is identical
    // (same `inputs.positions`); only the axis *source* differs, and that was
    // already resolved into the corner view above.
    let next_features = build_oriented_features(&inputs.positions, &inputs.axes);
    let report = detect_grid_all(DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Oriented2(&next_features),
        None,
        next_params,
    ));
    let report = match report {
        Ok(r) => r,
        // `InsufficientEvidence` / `DegenerateGeometry` /
        // `UnsupportedCombination` all collapse to "no components". The
        // topological path maps these cases to "no components".
        Err(_) => return Vec::new(),
    };
    if report.solutions.is_empty() {
        return Vec::new();
    }

    // `projective_grid::detect_grid_all` now runs the local-geometry
    // component merge inside the topological facade itself (mirroring its
    // seed-and-grow facade), so `report.solutions` already arrives merged.
    // The adapter therefore consumes the facade-merged components directly:
    // the previous chessboard-side `merge_components_local` call would have
    // double-merged and produced measured false attachments. Both merges
    // moved together in one commit; the facade merge runs with
    // `LocalMergeParams::default()`, which equals the chessboard's
    // `tuning.component_merge` for every shipping config (the field is never
    // overridden), so production output is byte-identical.
    let merged_components: Vec<HashMap<(i32, i32), usize>> = report
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

    let final_components = recover_topological_components(
        &merged_components,
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
    // Mirror the production path: resolve the axis source into a unified corner
    // view, then trace the single `Oriented2` evidence path. This keeps the
    // diagnostic trace consistent with production for both orientation sources.
    let corners: Cow<'_, [ChessCorner]> = match params.orientation_source {
        OrientationSource::ChessAxes => Cow::Borrowed(corners),
        OrientationSource::NeighbourEdges => Cow::Owned(corners_with_synthesized_axes(corners)),
    };
    let corners: &[ChessCorner] = &corners;

    let inputs = topological_inputs(corners, params);
    let (_augs, clustered_centers) = clustered_augs(corners, params);
    let mut topo_params = params.effective_tuning().topological;
    topo_params.axis_cluster_centers = clustered_centers.map(|c| [c.theta0, c.theta1]);
    let next_features = build_oriented_features(&inputs.positions, &inputs.axes);
    build_grid_topological_trace(&next_features, topo_params)
}
