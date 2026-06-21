//! Per-corner label assignment — chessboard glue over
//! [`projective_grid::cluster`].
//!
//! Once the two grid-direction centres `(Θ₀, Θ₁)` are known (see
//! [`super::cluster_axes`]), every corner is labelled by matching its two
//! axes against those centres. The pure assignment math (canonical /
//! swapped cost, tolerance gate) lives in [`projective_grid::cluster`];
//! this module maps the generic outputs onto the chessboard
//! [`ClusterLabel`] / [`AxisCluster`] vocabulary the pipeline consumes.

use crate::corner::ClusterLabel;
use projective_grid::cluster::AxisAssignment;
use serde::Serialize;

/// Per-corner assignment produced by [`super::cluster_axes`].
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum AxisCluster {
    /// Axes matched both centers within `cluster_tol_deg`, with the
    /// given slot assignment.
    Labeled {
        label: ClusterLabel,
        /// Worst per-axis distance to its matched center (radians).
        max_d_rad: f32,
    },
    /// The best assignment still left one axis further than
    /// `cluster_tol_deg` from its matched center.
    Unclustered { max_d_rad: f32 },
}

/// Map a generic [`AxisAssignment`] onto the chessboard [`AxisCluster`].
pub(super) fn map_assignment(assign: AxisAssignment) -> AxisCluster {
    match assign {
        AxisAssignment::Canonical { max_d_rad } => AxisCluster::Labeled {
            label: ClusterLabel::Canonical,
            max_d_rad,
        },
        AxisAssignment::Swapped { max_d_rad } => AxisCluster::Labeled {
            label: ClusterLabel::Swapped,
            max_d_rad,
        },
        AxisAssignment::None { max_d_rad } => AxisCluster::Unclustered { max_d_rad },
        _ => unreachable!("AxisAssignment is exhaustively handled"),
    }
}
