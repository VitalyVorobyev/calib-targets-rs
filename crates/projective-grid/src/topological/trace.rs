//! Exact, serializable observations of the production square pipeline.
//!
//! Every stage below is captured while the normal Rust implementation runs.
//! No Delaunay edge, quad, component, or label is reconstructed downstream.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

use crate::detect::{validate_request, DetectionParams, DetectionRequest, Evidence};
use crate::feature::OrientedFeature;
use crate::lattice::{GridDimensions, LatticeKind};
use crate::shared::recovery_schedule::SquareAxisProvenance;
use crate::topological::classify::EdgeClass;
use crate::topological::square_detector::{
    detect_square_oriented2_all_observed, SquarePipelineTrace,
};
use crate::topological::TopologicalParams;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[non_exhaustive]
/// One input feature and its exact admission decision.
pub struct TopologicalCornerTrace {
    /// Position in the evidence slice.
    pub index: usize,
    /// Caller-owned stable identifier.
    pub source_index: usize,
    /// Pixel-center coordinates `[x, y]` in the input image frame.
    pub position: [f32; 2],
    /// Two undirected local axes, in radians modulo π.
    pub axis_angles_rad: [f32; 2],
    /// Optional one-sigma uncertainties for the two axes.
    pub axis_sigmas_rad: [Option<f32>; 2],
    /// Whether the feature entered Delaunay triangulation.
    pub usable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
/// Production classification of a directed Delaunay half-edge.
pub enum TopologicalEdgeClass {
    /// Accepted as a lattice edge by both endpoints.
    Grid,
    /// Inferred to cross one square cell.
    Diagonal,
    /// Neither an accepted lattice edge nor an inferred diagonal.
    Spurious,
}

impl From<EdgeClass> for TopologicalEdgeClass {
    fn from(value: EdgeClass) -> Self {
        match value {
            EdgeClass::Grid => Self::Grid,
            EdgeClass::Diagonal => Self::Diagonal,
            EdgeClass::Spurious => Self::Spurious,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[non_exhaustive]
/// One directed Delaunay half-edge.
pub struct TopologicalEdgeTrace {
    /// Start feature-slice index.
    pub start: usize,
    /// End feature-slice index.
    pub end: usize,
    /// Edge class used by quad assembly.
    pub class: TopologicalEdgeClass,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[non_exhaustive]
/// One Delaunay triangle with its three directed edge classes.
pub struct TopologicalTriangleTrace {
    /// Feature-slice indices in Delaunay order.
    pub vertices: [usize; 3],
    /// Classes matching `(0→1, 1→2, 2→0)`.
    pub edge_classes: [TopologicalEdgeClass; 3],
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[non_exhaustive]
/// One square-cell hypothesis at a named filter checkpoint.
pub struct TopologicalQuadTrace {
    /// Feature-slice indices in TL–TR–BR–BL winding.
    pub vertices: [usize; 4],
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[non_exhaustive]
/// One lattice label with both internal and caller-owned provenance.
pub struct TopologicalLabelTrace {
    /// First square-lattice coordinate, increasing image-right after normalization.
    pub u: i32,
    /// Second square-lattice coordinate, increasing image-down after normalization.
    pub v: i32,
    /// Index into the supplied feature slice.
    pub feature_index: usize,
    /// Caller-owned source identifier.
    pub source_index: usize,
    /// Model-to-image residual at fitted checkpoints.
    pub residual_px: Option<f32>,
}

/// Projective-fit residual summary for one final generic component.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct TopologicalFitTrace {
    /// Number of fitted labels.
    pub count: usize,
    /// Mean residual in image pixels.
    pub mean_px: f32,
    /// Maximum residual in image pixels.
    pub max_px: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
/// One labelled component at a pipeline checkpoint.
pub struct TopologicalComponentTrace {
    /// Deterministic component position within this checkpoint.
    pub index: usize,
    /// Labels sorted by `(v, u, feature_index)`.
    pub labels: Vec<TopologicalLabelTrace>,
    /// Fit summary when this checkpoint has already been fitted.
    pub fit: Option<TopologicalFitTrace>,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[non_exhaustive]
/// Exact item counts for consistency checks between trace stages.
pub struct TopologicalTraceDiagnostics {
    /// Number of supplied features.
    pub corners_in: usize,
    /// Number admitted to Delaunay triangulation.
    pub corners_used: usize,
    /// Number of Delaunay triangles.
    pub triangles: usize,
    /// Number of triangle-pair quad hypotheses.
    pub raw_quads: usize,
    /// Quads surviving the mesh-degree filter.
    pub topology_quads: usize,
    /// Quads surviving the opposing-edge geometry filter.
    pub geometry_quads: usize,
    /// Quads surviving the component-scale filter.
    pub scale_quads: usize,
    /// Connected components produced by the quad walk.
    pub walk_components: usize,
    /// Components after generic label-space merge.
    pub merged_components: usize,
    /// Components after validation and projective fit.
    pub final_components: usize,
    /// Total labels across final generic detections.
    pub final_labels: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
/// Complete exact trace of one production square-detector execution.
pub struct TopologicalTrace {
    /// Version of this diagnostics-only serialization schema.
    pub schema_version: u32,
    /// Effective topological tuning used by the execution.
    pub params: TopologicalParams,
    /// Input feature evidence and admission decisions.
    pub corners: Vec<TopologicalCornerTrace>,
    /// Directed half-edges in triangle order.
    pub edges: Vec<TopologicalEdgeTrace>,
    /// Delaunay triangles and the classes consumed by quad assembly.
    pub triangles: Vec<TopologicalTriangleTrace>,
    /// All triangle-pair quad hypotheses.
    pub raw_quads: Vec<TopologicalQuadTrace>,
    /// Quads after the topology filter.
    pub topology_quads: Vec<TopologicalQuadTrace>,
    /// Quads after the geometry filter.
    pub geometry_quads: Vec<TopologicalQuadTrace>,
    /// Quads after the component-scale filter.
    pub scale_quads: Vec<TopologicalQuadTrace>,
    /// Components directly after walking the filtered quad mesh.
    pub walk_components: Vec<TopologicalComponentTrace>,
    /// Components after generic label-space merge.
    pub merged_components: Vec<TopologicalComponentTrace>,
    /// Normalized components after validation and projective fit.
    pub final_components: Vec<TopologicalComponentTrace>,
    /// Redundant counts used to validate exported evidence.
    pub diagnostics: TopologicalTraceDiagnostics,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
/// Failure to produce an exact topological trace.
pub enum TopologicalTraceError {
    /// The production detector rejected the evidence or geometry.
    #[error("topological detection failed: {message}")]
    DetectionFailed {
        /// Human-readable underlying detector error.
        message: String,
    },
}

/// Run the square detector once and capture its exact stage outputs.
pub fn build_grid_topological_trace(
    features: &[OrientedFeature<2>],
    dimensions: Option<GridDimensions>,
    params: DetectionParams,
) -> Result<TopologicalTrace, TopologicalTraceError> {
    let request = DetectionRequest::new(LatticeKind::Square, Evidence::Oriented2(features))
        .with_params(params.clone());
    let request = match dimensions {
        Some(dimensions) => request.with_dimensions(dimensions),
        None => request,
    };
    validate_request(&request).map_err(|error| TopologicalTraceError::DetectionFailed {
        message: error.to_string(),
    })?;
    let topological_params = params.tuning().topological;
    let mut raw = SquarePipelineTrace::default();
    let solutions = detect_square_oriented2_all_observed(
        features,
        dimensions,
        &params,
        SquareAxisProvenance::FullyMeasured,
        Some(&mut raw),
    )
    .map_err(|error| TopologicalTraceError::DetectionFailed {
        message: error.to_string(),
    })?;

    let corners = features
        .iter()
        .enumerate()
        .map(|(index, feature)| TopologicalCornerTrace {
            index,
            source_index: feature.point.source_index,
            position: [feature.point.position.x, feature.point.position.y],
            axis_angles_rad: [feature.axes[0].angle_rad, feature.axes[1].angle_rad],
            axis_sigmas_rad: [feature.axes[0].sigma_rad, feature.axes[1].sigma_rad],
            usable: raw.usable[index],
        })
        .collect();
    let edges: Vec<TopologicalEdgeTrace> = raw
        .edges
        .iter()
        .map(|&(start, end, class)| TopologicalEdgeTrace {
            start,
            end,
            class: class.into(),
        })
        .collect();
    let triangles = raw
        .triangles
        .iter()
        .enumerate()
        .map(|(index, &vertices)| TopologicalTriangleTrace {
            vertices,
            edge_classes: [
                edges[3 * index].class,
                edges[3 * index + 1].class,
                edges[3 * index + 2].class,
            ],
        })
        .collect();
    let raw_quads = quads(&raw.raw_quads);
    let topology_quads = quads(&raw.topology_quads);
    let geometry_quads = quads(&raw.geometry_quads);
    let scale_quads = quads(&raw.scale_quads);
    let walk_components = components(&raw.walk_components, features);
    let merged_components = components(&raw.merged_components, features);
    let feature_index_by_source: HashMap<usize, usize> = features
        .iter()
        .enumerate()
        .map(|(index, feature)| (feature.point.source_index, index))
        .collect();
    let final_components: Vec<TopologicalComponentTrace> = solutions
        .iter()
        .enumerate()
        .map(|(index, solution)| TopologicalComponentTrace {
            index,
            labels: solution
                .detection
                .grid()
                .entries()
                .iter()
                .map(|entry| TopologicalLabelTrace {
                    u: entry.coord.u,
                    v: entry.coord.v,
                    feature_index: feature_index_by_source[&entry.source_index],
                    source_index: entry.source_index,
                    residual_px: entry.residual_px,
                })
                .collect(),
            fit: Some(TopologicalFitTrace {
                count: solution.detection.fit().residuals.count,
                mean_px: solution.detection.fit().residuals.mean_px,
                max_px: solution.detection.fit().residuals.max_px,
            }),
        })
        .collect();
    let diagnostics = TopologicalTraceDiagnostics {
        corners_in: features.len(),
        corners_used: raw.usable.iter().filter(|&&usable| usable).count(),
        triangles: raw.triangles.len(),
        raw_quads: raw.raw_quads.len(),
        topology_quads: raw.topology_quads.len(),
        geometry_quads: raw.geometry_quads.len(),
        scale_quads: raw.scale_quads.len(),
        walk_components: raw.walk_components.len(),
        merged_components: raw.merged_components.len(),
        final_components: final_components.len(),
        final_labels: final_components
            .iter()
            .map(|component| component.labels.len())
            .sum(),
    };
    Ok(TopologicalTrace {
        schema_version: 1,
        params: topological_params,
        corners,
        edges,
        triangles,
        raw_quads,
        topology_quads,
        geometry_quads,
        scale_quads,
        walk_components,
        merged_components,
        final_components,
        diagnostics,
    })
}

fn quads(items: &[[usize; 4]]) -> Vec<TopologicalQuadTrace> {
    items
        .iter()
        .copied()
        .map(|vertices| TopologicalQuadTrace { vertices })
        .collect()
}

fn components(
    items: &[Vec<(crate::Coord, usize)>],
    features: &[OrientedFeature<2>],
) -> Vec<TopologicalComponentTrace> {
    items
        .iter()
        .enumerate()
        .map(|(index, labels)| TopologicalComponentTrace {
            index,
            labels: labels
                .iter()
                .map(|&(coord, feature_index)| TopologicalLabelTrace {
                    u: coord.u,
                    v: coord.v,
                    feature_index,
                    source_index: features[feature_index].point.source_index,
                    residual_px: None,
                })
                .collect(),
            fit: None,
        })
        .collect()
}
