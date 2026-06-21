//! Input adaptation for the chessboard topological dispatch path.

use crate::corner::ChessCorner;
use calib_targets_core::AxisEstimate;
use nalgebra::Point2;
use projective_grid::{synthesize_oriented2, PointFeature};

use crate::params::DetectorParams;

/// Angular uncertainty stamped onto neighbour-edge–synthesized axes.
///
/// [`synthesize_oriented2`] returns axes carrying no per-corner sigma
/// (`sigma_rad = None`), which round-trips to the workspace no-information
/// sentinel (`sigma = π`). Every axis-aware stage — clustering, Delaunay
/// admission, the recovery boosters — *skips* sentinel axes, so left as-is the
/// synthesized axes would never cluster and `clustered_centers` would stay
/// `None`, gating off the entire booster stack (the bug this module's
/// [`corners_with_synthesized_axes`] fixes). Stamping a small finite sigma keeps
/// the axes out of the no-info bucket. Because `cluster_sigma_k` defaults to 0
/// the admission tolerance is unaffected; the value only avoids the sentinel and
/// gives a near-unit clustering vote weight, i.e. it behaves like a confident
/// axis (well inside both `cluster_tol_deg` and `max_axis_sigma_rad`).
const SYNTH_SIGMA_RAD: f32 = 0.035; // ≈ 2°

/// Corner data passed from `calib-targets-chessboard` into `projective-grid`.
pub(super) struct TopologicalInputs {
    pub(super) positions: Vec<Point2<f32>>,
    pub(super) axes: Vec<[AxisEstimate; 2]>,
    pub(super) usable_count: usize,
}

/// Return a corner view whose `axes` are synthesized from neighbour-edge
/// geometry instead of carried from the ChESS detector.
///
/// This is the single entry point for `OrientationSource::NeighbourEdges`:
/// [`synthesize_oriented2`] derives the two local grid directions per corner
/// from the 4-nearest-neighbour chord geometry over the *full* point cloud
/// (same cloud the ChESS-axis path feeds the Delaunay builder), and we stamp the
/// result back onto a clone of each [`ChessCorner`]. Only `axes` is overwritten;
/// `strength`, `contrast`, and `fit_rms` are preserved so the downstream
/// strength/fit prefilter and clustering vote weights behave identically to the
/// ChESS-axis path. The rest of the pipeline (clustering → boosters → geometry
/// check) then consumes synthesized axes with no orientation-specific branch.
pub(super) fn corners_with_synthesized_axes(corners: &[ChessCorner]) -> Vec<ChessCorner> {
    let point_features: Vec<PointFeature> = corners
        .iter()
        .enumerate()
        .map(|(i, c)| PointFeature::new(i, c.position))
        .collect();
    // `synthesize_oriented2` returns one oriented feature per input, in input
    // order, carrying the same `source_index`; zipping by position is safe.
    let synthesized = synthesize_oriented2(&point_features);
    corners
        .iter()
        .zip(synthesized.iter())
        .map(|(c, feat)| {
            let mut out = *c;
            out.axes = [
                AxisEstimate {
                    angle: feat.axes[0].angle_rad,
                    sigma: SYNTH_SIGMA_RAD,
                },
                AxisEstimate {
                    angle: feat.axes[1].angle_rad,
                    sigma: SYNTH_SIGMA_RAD,
                },
            ];
            out
        })
        .collect()
}

#[inline]
fn axes_from(c: &ChessCorner) -> [AxisEstimate; 2] {
    [
        AxisEstimate {
            angle: c.axes[0].angle,
            sigma: c.axes[0].sigma,
        },
        AxisEstimate {
            angle: c.axes[1].angle,
            sigma: c.axes[1].sigma,
        },
    ]
}

fn prefilter(corners: &[ChessCorner], params: &DetectorParams) -> Vec<bool> {
    let min_corner_strength = params.min_corner_strength;
    let max_fit_rms_ratio = params.effective_tuning().max_fit_rms_ratio;
    corners
        .iter()
        .map(|c| {
            let strong = c.strength >= min_corner_strength;
            let fit_ok = !max_fit_rms_ratio.is_finite()
                || c.contrast <= 0.0
                || c.fit_rms <= max_fit_rms_ratio * c.contrast;
            strong && fit_ok
        })
        .collect()
}

/// Convert ChESS corners into the image-free input format expected by
/// `projective-grid`.
///
/// Corners that fail the same strength / fit-quality gate used by
/// seed-and-grow are retained as positions but given no-information axes.
/// This keeps raw corner indices stable for traces while preventing weak
/// corners from classifying Delaunay edges.
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "debug",
        skip_all,
        fields(num_corners = corners.len()),
    )
)]
pub(super) fn topological_inputs(
    corners: &[ChessCorner],
    params: &DetectorParams,
) -> TopologicalInputs {
    let mask = prefilter(corners, params);
    let positions: Vec<Point2<f32>> = corners.iter().map(|c| c.position).collect();
    let axes: Vec<[AxisEstimate; 2]> = corners
        .iter()
        .zip(mask.iter())
        .map(|(c, ok)| {
            if *ok {
                axes_from(c)
            } else {
                [AxisEstimate::default(); 2]
            }
        })
        .collect();
    let usable_count = mask.iter().filter(|&&b| b).count();
    TopologicalInputs {
        positions,
        axes,
        usable_count,
    }
}
