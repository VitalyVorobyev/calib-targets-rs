//! Topological detection pipeline for the chessboard detector.
//!
//! This module is the adapter between two layers:
//!
//! - `projective-grid`, which is image-free and labels connected
//!   quad-mesh components from oriented features (positions + per-corner
//!   axis hints) via
//!   [`assemble_oriented2_components`](projective_grid::expert::square::assemble_oriented2_components);
//! - `calib-targets-chessboard`, which owns ChESS corner filtering, recall
//!   boosters, final canonicalisation, and the public
//!   [`ChessboardDetection`](crate::ChessboardDetection) type.
//!
//! The production path intentionally remains one path. Diagnostics use
//! `trace_topological` (behind the opt-in `diagnostics` feature) for the same
//! `projective-grid` topological detector path; benchmark reports use the
//! optional `tracing` feature to time the same functions rather than a
//! second timed implementation.
//!
//! Production [`detect_all_topological`] calls the curated projective-grid
//! detector-builder seam. It stops after the shared local-geometry component
//! merge and preserves the walk's axis-slot frame, then feeds those components
//! to the chessboard's own recovery and validation pipeline. Ordinary
//! `projective-grid` callers continue through fit and public coordinate
//! normalization instead.
//!
//! The facade merge runs with `LocalMergeParams::default()`. The
//! chessboard's `tuning.component_merge` is `LocalMergeParams::default()`
//! for every shipping config (no preset or sweep overrides it), so the
//! merged components — and hence production output — are byte-identical to
//! the prior chessboard-side merge.
//!
//! `trace_topological_detection` observes every generic stage and continues
//! that same result through recovery and the public output; it never runs a
//! parallel reconstruction.
//!
//! # Submodules
//!
//! [`crate::detector::ChessboardDetector`] runs the topological grid builder here;
//! the post-build stages and the result types it consumes live in this
//! subtree:
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`cluster`] | Stage 2/3 — axis clustering into the two grid-direction centres. |
//! | [`inputs`] | Input adaptation: ChESS corners → `projective-grid` features. |
//! | [`recover`] | Stage 4 — per-component recall boosters + component merge. |
//! | [`boosters`] | Stage 11 — interior gap fill + line extrapolation. |
//! | [`types`] | [`ChessboardDetection`] / [`ChessboardCorner`] result types. |
//! | [`geometry_check`] | Stage 12 — the mandatory final precision gate. |
//! | [`output`] | Stage 13 — labelled grid → [`ChessboardDetection`]. |

mod boosters;
mod cluster;
mod geometry_check;
mod inputs;
mod output;
mod recover;
mod types;

use crate::corner::{ChessCorner, CornerAug};
#[cfg(feature = "diagnostics")]
use crate::corner::{ClusterLabel, CornerStage};
use calib_targets_core::{axis_estimate_to_next, AxisEstimate};
#[cfg(feature = "diagnostics")]
use projective_grid::diagnostics::trace::{
    build_grid_topological_trace, TopologicalTrace, TopologicalTraceError,
};
use projective_grid::expert::square::assemble_oriented2_components;
use projective_grid::expert::validation::ValidationParams as NextValidateParams;
use projective_grid::expert::{
    DetectionTuning as NextDetectionTuning, RecoverySchedule,
    TopologicalParams as NextTopologicalParams,
};
use projective_grid::{DetectionParams as NextDetectionParams, OrientedFeature, PointFeature};
use std::collections::HashMap;

use crate::params::ChessboardParams;

use self::cluster::ClusterCenters;
use self::inputs::{topological_inputs, TopologicalInputs};
use self::recover::{build_topological_detections, clustered_augs, recover_topological_components};

type LabelledComponent = HashMap<(i32, i32), usize>;
type LabelledComponents = Vec<LabelledComponent>;
type FinishedTopological = (Vec<ChessboardDetection>, Option<recover::RecoveryOutcome>);

#[cfg(feature = "diagnostics")]
pub use types::{ChessboardClusterTrace, ChessboardFeatureTrace};
pub use types::{ChessboardCorner, ChessboardDetection};
#[cfg(feature = "diagnostics")]
pub use types::{ChessboardLabelTrace, ChessboardStageTrace, ChessboardTopologicalTrace};

/// Largest 1σ axis uncertainty (radians) at which a corner is still allowed to
/// take part in the axis-driven grid stages.
///
/// This is *derived*, not tuned: it is exactly the topological builder's own
/// [`axis_align_tol_rad`](projective_grid::expert::TopologicalParams::axis_align_tol_rad),
/// the angular window inside which an edge counts as aligned with a corner's
/// axis. A corner whose own axis direction is less certain than that window
/// cannot answer the question the cell test asks of it — the classification it
/// would contribute is noise at the resolution being tested. Requiring
/// `sigma ≤ axis_align_tol_rad` states that as a dimensional relation between
/// an estimate and the tolerance it is compared against, so the two stay
/// coupled if either is retuned.
///
/// # Why this gate exists
///
/// It replaces the `fit_rms ≤ max_fit_rms_ratio · contrast` prefilter, retired
/// when `chess-corners` 1.0 removed the `contrast` / `fit_rms` descriptor
/// fields. That gate was load-bearing, and not in the direction one might
/// expect: corners failing it are *kept as positions but stripped of axes*, so
/// removing the gate does not merely admit extra corners, it lets
/// poorly-determined axes vote in Delaunay classification and axis clustering,
/// fragmenting the grid. On the densest public regression frame, dropping the
/// gate outright collapsed the labelled count from >2400 to 564.
///
/// Note the builder's own [`max_axis_sigma_rad`] is a *separate*, looser
/// backstop (`0.6 rad ≈ 34°`, more than twice `axis_align_tol_rad`), applied
/// inside `projective-grid` and shared with non-chessboard lattice callers.
/// This gate is the chessboard's stricter admission rule and is applied first.
///
/// [`max_axis_sigma_rad`]: projective_grid::expert::TopologicalParams::max_axis_sigma_rad
pub(super) fn axis_admission_sigma(params: &ChessboardParams) -> f32 {
    params.effective_tuning().topological.axis_align_tol_rad
}

/// Build a `projective-grid` [`NextDetectionParams`] for the
/// chessboard adapter's topological grid finder.
///
/// The new pipeline also runs a post-grow validation + fit-residual
/// drop. Both are disabled here (tolerances pushed to `+inf`,
/// `max_residual_px = +inf`) because the
/// chessboard owns its own geometry check downstream — the migration
/// must preserve the labelled components produced by the topological
/// graph builder.
fn detection_params_for_topological(topological: NextTopologicalParams) -> NextDetectionParams {
    // ChESS axes are accurate enough that the walk alone reaches full recall,
    // and the chessboard owns its own validation + booster recovery downstream.
    // Disable the facade's post-grow validation, post-fit residual drop, and
    // recovery schedule (tolerances → +inf, recovery `Off`) so the facade adds
    // nothing and production output stays byte-identical: a corner the facade
    // would have flagged still gets a chance to survive the chessboard's
    // downstream stages.
    let validate = NextValidateParams::default()
        .with_line_tol_rel(f32::INFINITY)
        .with_local_h_tol_rel(f32::INFINITY);
    let advanced = NextDetectionTuning::default()
        .with_topological(topological)
        .with_validation(validate)
        .with_recovery(RecoverySchedule::Off);
    NextDetectionParams::default()
        .with_advanced(advanced)
        .with_max_residual_px(f32::INFINITY)
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

struct PreparedTopological {
    inputs: TopologicalInputs,
    base_augs: Vec<CornerAug>,
    clustered_centers: Option<ClusterCenters>,
    features: Vec<OrientedFeature<2>>,
    component_params: NextTopologicalParams,
    grid_params: NextDetectionParams,
}

fn prepare_topological(
    corners: &[ChessCorner],
    params: &ChessboardParams,
) -> Option<PreparedTopological> {
    if corners.is_empty() {
        return None;
    }
    let (base_augs, clustered_centers) = clustered_augs(corners, params);
    let inputs = topological_inputs(corners, params);
    if inputs.usable_count < params.min_labeled_corners {
        return None;
    }
    let mut component_params = params.effective_tuning().topological;
    component_params.axis_cluster_centers = clustered_centers.map(|c| [c.theta0, c.theta1]);
    let grid_params = detection_params_for_topological(component_params);
    let features = build_oriented_features(&inputs.positions, &inputs.axes);
    Some(PreparedTopological {
        inputs,
        base_augs,
        clustered_centers,
        features,
        component_params,
        grid_params,
    })
}

fn finish_topological(
    merged_components: LabelledComponents,
    prepared: &PreparedTopological,
    params: &ChessboardParams,
    capture_recovered: bool,
) -> FinishedTopological {
    let recovered = recover_topological_components(
        &merged_components,
        &prepared.inputs.positions,
        &prepared.base_augs,
        prepared.clustered_centers,
        params,
    );
    let captured = capture_recovered.then(|| recovered.clone());
    let detections = build_topological_detections(
        recovered.components,
        &prepared.inputs.positions,
        &prepared.base_augs,
        prepared.clustered_centers,
        params,
    );
    (detections, captured)
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
pub(crate) fn detect_all_topological(
    corners: &[ChessCorner],
    params: &ChessboardParams,
) -> Vec<ChessboardDetection> {
    let Some(prepared) = prepare_topological(corners, params) else {
        return Vec::new();
    };

    // The topological builder consumes the per-corner ChESS axis estimates
    // carried by each `ChessCorner` directly: clustering, Delaunay admission,
    // and the recovery boosters all read `ChessCorner.axes`.

    // Hoist clustering: compute centers once up front to gate Delaunay
    // admission and reuse the same `(augs, centers)` pair for booster
    // recovery (no re-clustering), avoiding spurious-edge admissions.
    // Build the projective-grid input shape. `tuning.topological` carries
    // the chessboard tuning field names; `detection_params_for_topological` translates
    // them into `projective-grid`'s sub-config layout.
    //
    // Note on `cluster_axis_tol_rad`: keep the default 16° baked into
    // `NextTopologicalParams::default`. Do not reuse
    // `tuning.cluster_tol_deg` (12°) — the tighter literal value
    // regresses recovery on foreshortened boards (e.g. Gemini2).
    //
    // The expert assembly seam stops at component merge because the chessboard
    // owns pattern-specific validation and booster recovery downstream.
    let components =
        match assemble_oriented2_components(&prepared.features, &prepared.component_params) {
            Ok(components) => components,
            // `InsufficientEvidence` / `DegenerateGeometry` /
            // `UnsupportedCombination` all collapse to "no components". The
            // topological path maps these cases to "no components".
            Err(_) => return Vec::new(),
        };
    if components.is_empty() {
        return Vec::new();
    }

    // The projective-grid expert seam runs the local-geometry component merge,
    // so the returned components already arrive merged. The adapter consumes
    // them directly:
    // the previous chessboard-side `merge_components_local` call would have
    // double-merged and produced measured false attachments. Both merges
    // moved together in one commit; the facade merge runs with
    // `LocalMergeParams::default()`, which equals the chessboard's
    // `tuning.component_merge` for every shipping config (the field is never
    // overridden), so production output is byte-identical.
    let merged_components: LabelledComponents = components
        .iter()
        .map(|component| {
            component
                .entries()
                .iter()
                .map(|entry| {
                    let coord = entry.coord();
                    ((coord.u, coord.v), entry.source_index())
                })
                .collect()
        })
        .collect();

    finish_topological(merged_components, &prepared, params, false).0
}

/// Run the same topological input adaptation as `detect_all_topological`
/// (the crate-internal production orchestrator), but return a compact
/// topological trace instead of detections.
///
/// Corners that fail the chessboard strength / fit pre-filter are passed to
/// `projective-grid` with no-information axes, matching the production
/// topological dispatch path.
///
/// Available only with the `diagnostics` feature enabled.
#[cfg(feature = "diagnostics")]
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
    params: &ChessboardParams,
) -> Result<TopologicalTrace, TopologicalTraceError> {
    trace_topological_detection(corners, params).map(|trace| trace.projective_grid)
}

/// Run one exact generic-grid trace and continue that same result through the
/// chessboard recovery and final geometry gate.
#[cfg(feature = "diagnostics")]
pub fn trace_topological_detection(
    corners: &[ChessCorner],
    params: &ChessboardParams,
) -> Result<ChessboardTopologicalTrace, TopologicalTraceError> {
    let prepared = prepare_topological(corners, params).ok_or_else(|| {
        TopologicalTraceError::DetectionFailed {
            message: "chessboard admission produced too few usable corners".to_owned(),
        }
    })?;
    let grid_trace =
        build_grid_topological_trace(&prepared.features, None, prepared.grid_params.clone())?;
    let generic_components: LabelledComponents = grid_trace
        .merged_components
        .iter()
        .map(|component| {
            component
                .labels
                .iter()
                .map(|label| ((label.u, label.v), label.feature_index))
                .collect()
        })
        .collect();
    let generic_indices: std::collections::HashSet<usize> = generic_components
        .iter()
        .flat_map(|component| component.values().copied())
        .collect();
    let max_axis_sigma = axis_admission_sigma(params);
    let feature_trace: Vec<ChessboardFeatureTrace> = corners
        .iter()
        .zip(prepared.base_augs.iter())
        .enumerate()
        .map(|(corner_index, (corner, aug))| {
            let strength_admitted = corner.strength >= params.min_corner_strength;
            let sigma_admitted = corner.axes[0].sigma.max(corner.axes[1].sigma) <= max_axis_sigma;
            let prefilter_admitted = strength_admitted && sigma_admitted;
            let cluster = match (&aug.stage, aug.label) {
                (_, Some(ClusterLabel::Canonical)) => ChessboardClusterTrace::Canonical,
                (_, Some(ClusterLabel::Swapped)) => ChessboardClusterTrace::Swapped,
                (CornerStage::NoCluster { .. }, None) | (CornerStage::Strong, None) => {
                    ChessboardClusterTrace::Rejected
                }
                _ => ChessboardClusterTrace::NotAdmitted,
            };
            ChessboardFeatureTrace {
                corner_index,
                strength_admitted,
                sigma_admitted,
                prefilter_admitted,
                cluster,
            }
        })
        .collect();
    let (detections, recovered) = finish_topological(generic_components, &prepared, params, true);
    let recovered = recovered.unwrap_or_default();
    let recovered_indices: std::collections::HashSet<usize> = recovered
        .components
        .iter()
        .flat_map(|component| component.values().copied())
        .collect();
    let final_indices: std::collections::HashSet<usize> = detections
        .iter()
        .flat_map(|detection| detection.corners.iter().map(|corner| corner.input_index))
        .collect();
    let mut recovery_additions: Vec<usize> = recovered_indices
        .difference(&generic_indices)
        .copied()
        .collect();
    recovery_additions.sort_unstable();
    // A booster may rediscover a corner in a parity-aligned component even
    // though that corner was already present in another generic component.
    // Diagnostics describe net checkpoint changes, so retain only additions
    // absent from the generic union and surviving the post-recovery merge.
    let net_recovery_additions = |indices: &[usize]| {
        indices
            .iter()
            .copied()
            .filter(|index| recovered_indices.contains(index) && !generic_indices.contains(index))
            .collect::<Vec<_>>()
    };
    let axis_aware_recovery_additions = net_recovery_additions(&recovered.axis_aware_additions);
    let geometry_only_recovery_additions =
        net_recovery_additions(&recovered.geometry_only_additions);
    let mut final_drops: Vec<usize> = recovered_indices
        .difference(&final_indices)
        .copied()
        .collect();
    final_drops.sort_unstable();
    let recovered_components = recovered
        .components
        .into_iter()
        .map(|component| {
            let mut labels: Vec<ChessboardLabelTrace> = component
                .into_iter()
                .map(|((u, v), corner_index)| ChessboardLabelTrace { u, v, corner_index })
                .collect();
            labels.sort_by_key(|label| (label.v, label.u, label.corner_index));
            labels
        })
        .collect();
    Ok(ChessboardTopologicalTrace {
        projective_grid: grid_trace,
        chessboard: ChessboardStageTrace {
            features: feature_trace,
            recovered_components,
            recovery_additions,
            axis_aware_recovery_additions,
            geometry_only_recovery_additions,
            final_drops,
        },
        detections,
    })
}
