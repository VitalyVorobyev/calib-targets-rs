//! Opt-in evidence explaining how detections were produced.
//!
//! Diagnostics are intentionally separate from the ordinary detection result.
//! Their serializable schema supports debugging, tuning, benchmarks, and blog
//! visualizations without making intermediate pipeline stages part of the
//! stable facade contract.

/// Exact trace of the square topological pipeline.
pub mod trace {
    pub use crate::topological::trace::*;
}

pub use crate::result::{RejectedFeature, RejectionReason};

use crate::detect::{detect_grid_all_internal, DetectionRequest};
use crate::error::{GridError, Result};
use crate::result::GridDetection;

/// Diagnostics associated with one returned grid component.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct ComponentDiagnostics {
    rejected: Vec<RejectedFeature>,
}

impl ComponentDiagnostics {
    /// Features rejected while assembling this component.
    pub fn rejected(&self) -> &[RejectedFeature] {
        &self.rejected
    }
}

/// Opt-in diagnostics for a multi-component detection.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct DetectionDiagnostics {
    schema_version: u32,
    components: Vec<ComponentDiagnostics>,
}

impl DetectionDiagnostics {
    /// Diagnostics schema version.
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// Per-component diagnostics in the same order as returned detections.
    pub fn components(&self) -> &[ComponentDiagnostics] {
        &self.components
    }
}

/// Detect all components and retain rejection diagnostics.
pub fn detect_grid_all(
    request: DetectionRequest<'_>,
) -> Result<(Vec<GridDetection>, DetectionDiagnostics)> {
    let solutions = detect_grid_all_internal(request)?;
    let mut detections = Vec::with_capacity(solutions.len());
    let mut components = Vec::with_capacity(solutions.len());
    for solution in solutions {
        detections.push(solution.detection);
        components.push(ComponentDiagnostics {
            rejected: solution.rejected,
        });
    }
    Ok((
        detections,
        DetectionDiagnostics {
            schema_version: 1,
            components,
        },
    ))
}

/// Detect the primary component and retain its rejection diagnostics.
pub fn detect_grid(request: DetectionRequest<'_>) -> Result<(GridDetection, ComponentDiagnostics)> {
    let (mut detections, diagnostics) = detect_grid_all(request)?;
    if detections.is_empty() {
        return Err(GridError::InsufficientEvidence);
    }
    let detection = detections.remove(0);
    let component = diagnostics
        .components
        .into_iter()
        .next()
        .unwrap_or(ComponentDiagnostics {
            rejected: Vec::new(),
        });
    Ok((detection, component))
}
