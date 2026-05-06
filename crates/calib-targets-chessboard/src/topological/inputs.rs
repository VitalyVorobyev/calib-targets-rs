//! Input adaptation for the chessboard topological dispatch path.

use calib_targets_core::Corner;
use nalgebra::Point2;
use projective_grid::AxisHint;

use crate::params::DetectorParams;

/// Corner data passed from `calib-targets-chessboard` into `projective-grid`.
pub(super) struct TopologicalInputs {
    pub(super) positions: Vec<Point2<f32>>,
    pub(super) axes: Vec<[AxisHint; 2]>,
    pub(super) usable_count: usize,
}

#[inline]
fn axis_hint_from(c: &Corner) -> [AxisHint; 2] {
    [
        AxisHint {
            angle: c.axes[0].angle,
            sigma: c.axes[0].sigma,
        },
        AxisHint {
            angle: c.axes[1].angle,
            sigma: c.axes[1].sigma,
        },
    ]
}

fn prefilter(corners: &[Corner], params: &DetectorParams) -> Vec<bool> {
    corners
        .iter()
        .map(|c| {
            let strong = c.strength >= params.min_corner_strength;
            let fit_ok = !params.max_fit_rms_ratio.is_finite()
                || c.contrast <= 0.0
                || c.fit_rms <= params.max_fit_rms_ratio * c.contrast;
            strong && fit_ok
        })
        .collect()
}

/// Convert ChESS corners into the image-free input format expected by
/// `projective-grid`.
///
/// Corners that fail the same strength / fit-quality gate used by
/// chessboard-v2 are retained as positions but given no-information axes.
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
pub(super) fn topological_inputs(corners: &[Corner], params: &DetectorParams) -> TopologicalInputs {
    let mask = prefilter(corners, params);
    let positions: Vec<Point2<f32>> = corners.iter().map(|c| c.position).collect();
    let axes: Vec<[AxisHint; 2]> = corners
        .iter()
        .zip(mask.iter())
        .map(|(c, ok)| {
            if *ok {
                axis_hint_from(c)
            } else {
                [AxisHint::default(); 2]
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
