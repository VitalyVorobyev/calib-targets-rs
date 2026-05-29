//! Serializable trace for the square topological detector.
//!
//! This diagnostic entry point is intentionally layered over the production
//! [`crate::detect_grid_all`] facade so timing and recovery stay aligned with
//! the detector path. It records the stable facts downstream diagnostics need:
//! input corner usability, final labelled components, and summary counts.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::detect::{
    detect_grid_all, DetectionParams, DetectionRequest, Evidence, SquareAlgorithm,
    TopologicalParams,
};
use crate::feature::OrientedFeature;
use crate::lattice::LatticeKind;

/// One input corner as seen by the topological trace.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TopologicalCornerTrace {
    /// Index of this feature in the supplied slice.
    pub index: usize,
    /// Caller-owned source index.
    pub source_index: usize,
    /// Corner position `[x, y]` in image pixels.
    pub position: [f32; 2],
    /// Local axis angles in radians.
    pub axis_angles_rad: [f32; 2],
    /// Local axis uncertainties in radians. `None` means no uncertainty was supplied.
    pub axis_sigmas_rad: [Option<f32>; 2],
    /// Whether the feature survived the topological sigma/axis prefilter.
    pub usable: bool,
}

/// One final `(u, v) -> source_index` label.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TopologicalLabelTrace {
    /// First square-grid coordinate.
    pub u: i32,
    /// Second square-grid coordinate.
    pub v: i32,
    /// Source index of the labelled input feature.
    pub source_index: usize,
}

impl TopologicalLabelTrace {
    /// Build a label trace entry mapping grid `(u, v)` to a source index.
    pub fn new(u: i32, v: i32, source_index: usize) -> Self {
        Self { u, v, source_index }
    }
}

/// One connected labelled component.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TopologicalComponentTrace {
    /// Index of this component within the result.
    pub index: usize,
    /// The component's labels, sorted by `(v, u, source_index)`.
    pub labels: Vec<TopologicalLabelTrace>,
}

/// Summary counters for the topological trace.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TopologicalTraceDiagnostics {
    /// Number of input features.
    pub corners_in: usize,
    /// Number of features that survived the axis usability filter.
    pub corners_used: usize,
    /// Number of labelled components returned by production detection.
    pub components: usize,
    /// Total number of labelled entries across all components.
    pub labels: usize,
}

/// Full topological trace.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TopologicalTrace {
    /// Parameters used by the topological stage.
    pub params: TopologicalParams<f32>,
    /// Every input corner with its usable flag.
    pub corners: Vec<TopologicalCornerTrace>,
    /// Labelled connected components.
    pub components: Vec<TopologicalComponentTrace>,
    /// Per-stage summary counters.
    pub diagnostics: TopologicalTraceDiagnostics,
}

/// Errors emitted by [`build_grid_topological_trace`].
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum TopologicalTraceError {
    /// Fewer than three usable corners survived the topological prefilter.
    #[error("not enough usable corners ({usable}) for Delaunay triangulation")]
    NotEnoughCorners {
        /// Number of usable corners.
        usable: usize,
    },
    /// Production detection did not return a labelled component.
    #[error("topological detection produced no labelled components")]
    NoComponents,
}

/// Build a trace for the production square topological detector.
pub fn build_grid_topological_trace(
    features: &[OrientedFeature<f32, 2>],
    params: TopologicalParams<f32>,
) -> Result<TopologicalTrace, TopologicalTraceError> {
    let corners: Vec<TopologicalCornerTrace> = features
        .iter()
        .enumerate()
        .map(|(index, feature)| {
            let usable = feature.axes.iter().any(|axis| {
                axis.sigma_rad
                    .map(|s| s < params.max_axis_sigma_rad)
                    .unwrap_or(true)
            });
            TopologicalCornerTrace {
                index,
                source_index: feature.point.source_index,
                position: [feature.point.position.x, feature.point.position.y],
                axis_angles_rad: [feature.axes[0].angle_rad, feature.axes[1].angle_rad],
                axis_sigmas_rad: [feature.axes[0].sigma_rad, feature.axes[1].sigma_rad],
                usable,
            }
        })
        .collect();
    let corners_used = corners.iter().filter(|c| c.usable).count();
    if corners_used < 3 {
        return Err(TopologicalTraceError::NotEnoughCorners {
            usable: corners_used,
        });
    }

    let validate = crate::detect::ValidateParams::<f32>::default()
        .with_line_tol_rel(f32::INFINITY)
        .with_local_h_tol_rel(f32::INFINITY)
        .with_edge_length_band_rel(f32::INFINITY);
    let request = DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Oriented2(features),
        None,
        DetectionParams::default()
            .with_algorithm(SquareAlgorithm::Topological)
            .with_topological(params)
            .with_validate(validate)
            .with_max_residual_px(f32::INFINITY),
    );
    let report = detect_grid_all(request).map_err(|_| TopologicalTraceError::NoComponents)?;
    if report.solutions.is_empty() {
        return Err(TopologicalTraceError::NoComponents);
    }

    let components: Vec<TopologicalComponentTrace> = report
        .solutions
        .iter()
        .enumerate()
        .map(|(index, solution)| {
            let mut labels: Vec<TopologicalLabelTrace> = solution
                .grid
                .entries
                .iter()
                .map(|entry| TopologicalLabelTrace {
                    u: entry.coord.u,
                    v: entry.coord.v,
                    source_index: entry.source_index,
                })
                .collect();
            labels.sort_by_key(|label| (label.v, label.u, label.source_index));
            TopologicalComponentTrace { index, labels }
        })
        .collect();
    let labels = components
        .iter()
        .map(|component| component.labels.len())
        .sum();
    let diagnostics = TopologicalTraceDiagnostics {
        corners_in: features.len(),
        corners_used,
        components: components.len(),
        labels,
    };
    Ok(TopologicalTrace {
        params,
        corners,
        components,
        diagnostics,
    })
}
