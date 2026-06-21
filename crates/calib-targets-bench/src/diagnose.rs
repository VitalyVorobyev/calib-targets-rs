//! Diagnose computations shared by the bench CLI's `diagnose` subcommand and
//! programmatic consumers (e.g. the studio server): the topological
//! labelled-vs-unlabelled breakdown with pre-filter survival counts.
//!
//! All printing / overlay-file output stays with the callers; this module
//! only computes serializable data.

use std::collections::HashSet;

use calib_targets::chessboard::{ChessCorner, Detector, DetectorParams};
use serde::Serialize;

/// The effective topological tolerances a diagnosis ran with (radians /
/// relative units, straight from the resolved `AdvancedTuning`).
#[derive(Clone, Copy, Debug, Serialize)]
pub struct TolSummary {
    /// Maximum angular deviation between an edge and a cluster axis for the
    /// edge to count as "grid".
    pub axis_align_tol_rad: f32,
    /// Per-corner axis-sigma ceiling for the pre-filter.
    pub max_axis_sigma_rad: f32,
    /// Cluster-assignment tolerance.
    pub cluster_axis_tol_rad: f32,
    /// Maximum edge length relative to the local median.
    pub edge_length_max_rel: f32,
}

/// Pre-filter survival funnel: how many input corners pass each successive
/// gate (strength → fit quality → axis sigma).
#[derive(Clone, Copy, Debug, Serialize)]
pub struct PrefilterCounts {
    /// Corners with `strength >= min_corner_strength`.
    pub survives_strength: usize,
    /// Of those, corners that also pass the fit-RMS/contrast gate.
    pub survives_fit: usize,
    /// Of those, corners with at least one axis under the sigma ceiling.
    pub survives_axis: usize,
}

/// One labelled component recovered by `detect_all`.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct ComponentSummary {
    /// Number of labelled corners in this component.
    pub labelled: usize,
    /// Labelled bounding box as `[min_i, max_i, min_j, max_j]`.
    pub bbox: [i32; 4],
}

/// Per-input-corner diagnosis row: position, axis sigmas, and whether the
/// corner ended up labelled in any component.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct DiagnoseCorner {
    /// Pixel x in the fed image.
    pub x: f32,
    /// Pixel y in the fed image.
    pub y: f32,
    /// Axis-0 angular sigma, radians.
    pub sigma0: f32,
    /// Axis-1 angular sigma, radians.
    pub sigma1: f32,
    /// `true` when the corner is labelled in at least one component.
    pub labelled: bool,
}

/// Topological-pipeline diagnosis: pre-filter funnel, effective tolerances,
/// per-component sizes, and the labelled/unlabelled split over all input
/// corners.
#[derive(Clone, Debug, Serialize)]
pub struct TopologicalDiagnosis {
    /// Number of input ChESS corners.
    pub input_count: usize,
    /// Effective topological tolerances the run used.
    pub effective_tols: TolSummary,
    /// Pre-filter survival funnel.
    pub prefilter: PrefilterCounts,
    /// Labelled components recovered by `detect_all`, in detection order.
    pub components: Vec<ComponentSummary>,
    /// Input-corner indices labelled in at least one component (ascending).
    pub labelled_indices: Vec<usize>,
    /// One row per input corner, aligned with the input slice.
    pub corners: Vec<DiagnoseCorner>,
}

/// Run the production topological detector path on `corners` and compute a
/// [`TopologicalDiagnosis`]. `params` is used as-is.
pub fn diagnose_topological(
    params: &DetectorParams,
    corners: &[ChessCorner],
) -> TopologicalDiagnosis {
    let tuning = params.effective_tuning();
    let topo = &tuning.topological;
    let effective_tols = TolSummary {
        axis_align_tol_rad: topo.axis_align_tol_rad,
        max_axis_sigma_rad: topo.max_axis_sigma_rad,
        cluster_axis_tol_rad: topo.cluster_axis_tol_rad,
        edge_length_max_rel: topo.edge_length_max_rel,
    };

    // Pre-filter: at least one axis with sigma below threshold AND the
    // standard chessboard strength + fit-quality gates.
    let mut survives_strength = 0usize;
    let mut survives_fit = 0usize;
    let mut survives_axis = 0usize;
    for c in corners {
        let strong = c.strength >= params.min_corner_strength;
        let fit_ok = !tuning.max_fit_rms_ratio.is_finite()
            || c.contrast <= 0.0
            || c.fit_rms <= tuning.max_fit_rms_ratio * c.contrast;
        let axis_ok =
            c.axes[0].sigma < topo.max_axis_sigma_rad || c.axes[1].sigma < topo.max_axis_sigma_rad;
        if strong {
            survives_strength += 1;
        }
        if strong && fit_ok {
            survives_fit += 1;
        }
        if strong && fit_ok && axis_ok {
            survives_axis += 1;
        }
    }
    let prefilter = PrefilterCounts {
        survives_strength,
        survives_fit,
        survives_axis,
    };

    let detections = Detector::new(params.clone())
        .expect("valid detector params")
        .detect_all(corners);
    let labelled_corner_set: HashSet<usize> = detections
        .iter()
        .flat_map(|d| d.corners.iter().map(|c| c.input_index))
        .collect();

    let components = detections
        .iter()
        .map(|detection| {
            let min_i = detection
                .corners
                .iter()
                .map(|c| c.grid.i)
                .min()
                .unwrap_or(0);
            let max_i = detection
                .corners
                .iter()
                .map(|c| c.grid.i)
                .max()
                .unwrap_or(0);
            let min_j = detection
                .corners
                .iter()
                .map(|c| c.grid.j)
                .min()
                .unwrap_or(0);
            let max_j = detection
                .corners
                .iter()
                .map(|c| c.grid.j)
                .max()
                .unwrap_or(0);
            ComponentSummary {
                labelled: detection.corners.len(),
                bbox: [min_i, max_i, min_j, max_j],
            }
        })
        .collect();

    let corner_rows = corners
        .iter()
        .enumerate()
        .map(|(k, c)| DiagnoseCorner {
            x: c.position.x,
            y: c.position.y,
            sigma0: c.axes[0].sigma,
            sigma1: c.axes[1].sigma,
            labelled: labelled_corner_set.contains(&k),
        })
        .collect();

    let mut labelled_indices: Vec<usize> = labelled_corner_set.into_iter().collect();
    labelled_indices.sort_unstable();

    TopologicalDiagnosis {
        input_count: corners.len(),
        effective_tols,
        prefilter,
        components,
        labelled_indices,
        corners: corner_rows,
    }
}
