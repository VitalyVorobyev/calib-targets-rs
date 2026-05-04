//! Topological dispatch path for the chessboard detector.
//!
//! This module is the adapter between two layers:
//!
//! - `projective-grid`, which is image-free and labels connected quad-mesh
//!   components from positions plus per-corner axis hints;
//! - `calib-targets-chessboard`, which owns ChESS corner filtering, recall
//!   boosters, final canonicalisation, and the public [`Detection`] type.
//!
//! The production path intentionally remains one path. Blog overlays use
//! [`trace_topological`] for intermediate `projective-grid` stages; benchmark
//! reports use the optional `tracing` feature to time the same functions rather
//! than a second timed implementation.

mod inputs;
mod recovery;

use calib_targets_core::Corner;
use projective_grid::{
    build_grid_topological, build_grid_topological_trace, merge_components_local, ComponentInput,
    TopologicalGrid, TopologicalTrace,
};

use crate::detector::Detection;
use crate::params::DetectorParams;

use self::inputs::topological_inputs;
use self::recovery::{
    build_topological_detections, clustered_augs, recover_topological_components,
};

/// Run the topological pipeline and return one [`Detection`] per surviving
/// labelled component.
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "info",
        skip_all,
        fields(num_corners = corners.len()),
    )
)]
pub fn detect_all_topological(corners: &[Corner], params: &DetectorParams) -> Vec<Detection> {
    if corners.is_empty() {
        return Vec::new();
    }

    let inputs = topological_inputs(corners, params);
    if inputs.usable_count < params.min_labeled_corners {
        return Vec::new();
    }

    let topo: TopologicalGrid =
        match build_grid_topological(&inputs.positions, &inputs.axes, &params.topological) {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
    if topo.components.is_empty() {
        return Vec::new();
    }

    let component_views: Vec<ComponentInput<'_>> = topo
        .components
        .iter()
        .map(|c| ComponentInput {
            labelled: &c.labelled,
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
        merge_components_local(&component_views, &params.component_merge)
    };
    #[cfg(not(feature = "tracing"))]
    let merged = merge_components_local(&component_views, &params.component_merge);

    let (base_augs, clustered_centers) = clustered_augs(corners, params);
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
    corners: &[Corner],
    params: &DetectorParams,
) -> Result<TopologicalTrace, projective_grid::TopologicalError> {
    let inputs = topological_inputs(corners, params);
    build_grid_topological_trace(&inputs.positions, &inputs.axes, &params.topological)
}
