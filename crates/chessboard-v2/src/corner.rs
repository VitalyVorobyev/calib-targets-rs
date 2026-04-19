//! Per-corner augmented state carried through the v2 pipeline.

use calib_targets_core::{AxisEstimate, Corner};
use nalgebra::Point2;
use serde::Serialize;

/// Binary axis-slot label derived from the matched cluster centers.
///
/// A corner's `axes[0]` matches one of the two global cluster centers
/// `{Θ₀, Θ₁}`; `axes[1]` matches the other. The label records which
/// slot holds the `Θ₀`-matching axis:
///
/// * `ClusterLabel::Canonical` — `axes[0] ≈ Θ₀` and `axes[1] ≈ Θ₁`.
/// * `ClusterLabel::Swapped`   — `axes[0] ≈ Θ₁` and `axes[1] ≈ Θ₀`.
///
/// On a chessboard, adjacent grid corners carry opposite labels (the
/// slot assignment flips across every edge). This is the parity
/// invariant that drives the edge-axis-slot-swap check.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum ClusterLabel {
    Canonical,
    Swapped,
}

impl ClusterLabel {
    /// `0` for `Canonical`, `1` for `Swapped`. Used as the cluster
    /// integer in the spec.
    #[inline]
    pub fn as_u8(self) -> u8 {
        match self {
            ClusterLabel::Canonical => 0,
            ClusterLabel::Swapped => 1,
        }
    }

    /// The slot index whose axis matches `Θ₀` under this label.
    ///
    /// * `Canonical` → `0`.
    /// * `Swapped`   → `1`.
    #[inline]
    pub fn slot_of_theta0(self) -> usize {
        match self {
            ClusterLabel::Canonical => 0,
            ClusterLabel::Swapped => 1,
        }
    }

    /// The slot index whose axis matches `Θ₁` under this label.
    #[inline]
    pub fn slot_of_theta1(self) -> usize {
        1 - self.slot_of_theta0()
    }

    /// The other label.
    #[inline]
    pub fn flipped(self) -> Self {
        match self {
            ClusterLabel::Canonical => ClusterLabel::Swapped,
            ClusterLabel::Swapped => ClusterLabel::Canonical,
        }
    }
}

/// Stage marker tracked per input corner through the pipeline.
///
/// Every `Corner` the detector sees starts at [`CornerStage::Raw`]
/// and advances as it passes (or fails) pipeline stages. This is the
/// unit of observability for the debug frame.
#[non_exhaustive]
#[derive(Clone, Debug, Serialize)]
pub enum CornerStage {
    /// Newly ingested corner, no checks yet.
    Raw,
    /// Passed Stage 1 (strength + fit-quality + axes validity).
    Strong,
    /// Stage 3 rejected — at least one axis further than
    /// `cluster_tol_deg` from the matching center. `max_d_deg` is the
    /// worse of the two per-axis distances in the best assignment.
    NoCluster { max_d_deg: f32 },
    /// Stage 3 accepted with the given label.
    Clustered { label: ClusterLabel },
    /// Stage 6 attempted to attach this corner at `at` but found
    /// ≥2 candidates inside `attach_ambiguity_factor × nearest`.
    AttachmentAmbiguous { at: (i32, i32) },
    /// Stage 6 attempted to attach this corner at `at` but the
    /// induced edges failed an invariant. The pipeline leaves the
    /// corner un-labelled and continues.
    AttachmentFailedInvariants { at: (i32, i32), reason: String },
    /// Attached as a labelled corner. Filled in by Stages 5–8.
    Labeled {
        at: (i32, i32),
        local_h_residual_px: Option<f32>,
    },
    /// Previously labelled at `at`, then blacklisted during
    /// post-validation. `reason` is human-readable for overlays.
    LabeledThenBlacklisted { at: (i32, i32), reason: String },
}

/// Augmented corner carried through the pipeline.
///
/// Wraps a reference-like snapshot of the input [`Corner`] plus
/// per-stage state: cluster label, current stage, (i, j) label when
/// attached.
#[derive(Clone, Debug, Serialize)]
pub struct CornerAug {
    /// Index in the original input corner slice. Stable across all
    /// pipeline stages, used as the identity key for blacklists.
    pub input_index: usize,
    /// Pixel position (copied from `Corner.position` at construction).
    pub position: Point2<f32>,
    /// Both grid axes with per-axis uncertainty.
    pub axes: [AxisEstimate; 2],
    /// ChESS strength (copied at construction).
    pub strength: f32,
    /// Upstream contrast amplitude.
    pub contrast: f32,
    /// Upstream tanh-fit RMS.
    pub fit_rms: f32,
    /// Stage cursor — starts at `Raw`.
    pub stage: CornerStage,
    /// Cluster label assigned in Stage 3 (`None` while `stage` is
    /// `Raw`, `Strong`, or `NoCluster`).
    pub label: Option<ClusterLabel>,
}

impl CornerAug {
    /// Build a fresh [`CornerAug`] from a Corner input.
    pub fn from_corner(input_index: usize, c: &Corner) -> Self {
        Self {
            input_index,
            position: c.position,
            axes: c.axes,
            strength: c.strength,
            contrast: c.contrast,
            fit_rms: c.fit_rms,
            stage: CornerStage::Raw,
            label: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cluster_label_slot_invariants() {
        let a = ClusterLabel::Canonical;
        assert_eq!(a.slot_of_theta0(), 0);
        assert_eq!(a.slot_of_theta1(), 1);
        assert_eq!(a.flipped(), ClusterLabel::Swapped);
        assert_eq!(a.as_u8(), 0);

        let b = ClusterLabel::Swapped;
        assert_eq!(b.slot_of_theta0(), 1);
        assert_eq!(b.slot_of_theta1(), 0);
        assert_eq!(b.flipped(), ClusterLabel::Canonical);
        assert_eq!(b.as_u8(), 1);
    }
}
