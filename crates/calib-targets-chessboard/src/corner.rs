//! Per-corner augmented state carried through the pipeline.

use calib_targets_core::AxisEstimate;
use nalgebra::Point2;
use serde::{Deserialize, Serialize};

/// Canonical 2D corner consumed by the chessboard detector.
///
/// Carries the per-corner data the pipeline needs to admit or reject the
/// corner during clustering, seed selection, grow, and post-grow validation:
/// pixel position, the two local grid-axis directions with per-axis 1σ
/// uncertainty, the ChESS detector's response (`strength`), the tanh-fit
/// `contrast` amplitude, and the tanh-fit residual (`fit_rms`).
///
/// Callers constructing corners from `chess_corners::CornerDescriptor` typically
/// go through the workspace facade's adapter; callers handing the detector a
/// pre-built corner cloud (tests, custom upstreams) construct `ChessCorner`
/// directly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ChessCorner {
    /// Corner position in pixel coordinates.
    pub position: Point2<f32>,
    /// Two local grid-axis directions with per-axis 1σ angular uncertainty.
    /// Default-constructed axes carry `sigma = π` ("no information") and
    /// cause the corner to be skipped by every axis-aware stage.
    pub axes: [AxisEstimate; 2],
    /// Bright/dark amplitude `|A|` (≥ 0, gray levels) from the upstream
    /// two-axis tanh fit. Independent from [`Self::strength`].
    pub contrast: f32,
    /// RMS residual of the two-axis intensity fit (gray levels). Lower is
    /// a tighter match to an ideal chessboard corner.
    pub fit_rms: f32,
    /// Corner detector response (raw ChESS response at the detected peak).
    /// Positive values are corner candidates.
    pub strength: f32,
}

impl ChessCorner {
    /// Construct a [`ChessCorner`] from a position. All other fields default
    /// to the no-information sentinel — primarily useful for test fixtures.
    pub fn from_position(position: Point2<f32>) -> Self {
        Self {
            position,
            ..Self::default()
        }
    }
}

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
    /// `axes[0] ≈ Θ₀` and `axes[1] ≈ Θ₁` — the unswapped slot assignment.
    Canonical,
    /// `axes[0] ≈ Θ₁` and `axes[1] ≈ Θ₀` — the slots are swapped.
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
    ///
    /// Part of the diagnostics-facing vocabulary (consumers interpreting a
    /// `CornerStage::Clustered { label }` out of a `DebugFrame`); compiled
    /// only with the `diagnostics` feature (or under `cfg(test)`).
    #[cfg(any(test, feature = "diagnostics"))]
    #[inline]
    pub fn slot_of_theta0(self) -> usize {
        match self {
            ClusterLabel::Canonical => 0,
            ClusterLabel::Swapped => 1,
        }
    }

    /// The slot index whose axis matches `Θ₁` under this label.
    ///
    /// See [`ClusterLabel::slot_of_theta0`]; same diagnostics-only gate.
    #[cfg(any(test, feature = "diagnostics"))]
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
///
/// Note: a future cleanup may split this enum into a pure pipeline cursor
/// (`Raw / Strong / NoCluster / Clustered / Labeled`) plus an orthogonal
/// `AttachmentOutcome { at, outcome, reason, local_h_residual_px }`. The
/// current shape is preserved for now because the split exposed a subtle
/// regression on the `puzzleboard_reference/example1.png` multi-component
/// test; landing it safely needs additional investigation of the
/// `fix_partial_slot_flips` + `detect_all` interaction.
#[non_exhaustive]
#[derive(Clone, Debug, Serialize)]
pub enum CornerStage {
    /// Newly ingested corner, no checks yet.
    Raw,
    /// Passed `prefilter` (strength + fit-quality + axes validity).
    Strong,
    /// `cluster_axes` rejected — at least one axis further than
    /// `cluster_tol_deg` from the matching center. `max_d_deg` is the
    /// worse of the two per-axis distances in the best assignment.
    NoCluster {
        /// Worse of the two per-axis distances (degrees) in the best
        /// assignment that still failed `cluster_tol_deg`.
        max_d_deg: f32,
    },
    /// `cluster_axes` accepted with the given label.
    Clustered {
        /// The axis-slot label assigned by clustering.
        label: ClusterLabel,
    },
    /// `extend_boundary` attempted to attach this corner at `at` but
    /// found ≥2 candidates inside `attach_ambiguity_factor × nearest`.
    ///
    /// A diagnostics-only stage marker (rendered by overlays); compiled
    /// only with the `diagnostics` feature. The current pipeline records
    /// ambiguous extension candidates as plain un-labelled corners.
    #[cfg(feature = "diagnostics")]
    AttachmentAmbiguous {
        /// The `(i, j)` cell the corner was being attached at.
        at: (i32, i32),
    },
    /// `extend_boundary` attempted to attach this corner at `at` but
    /// the induced edges failed an invariant. The pipeline leaves the
    /// corner un-labelled and continues.
    ///
    /// A diagnostics-only stage marker (rendered by overlays); compiled
    /// only with the `diagnostics` feature.
    #[cfg(feature = "diagnostics")]
    AttachmentFailedInvariants {
        /// The `(i, j)` cell the corner was being attached at.
        at: (i32, i32),
        /// Human-readable description of the invariant that failed.
        reason: String,
    },
    /// Attached as a labelled corner. Filled in by the `grow` /
    /// `extend_boundary` / `rescue_no_cluster` / `apply_boosters`
    /// stages.
    Labeled {
        /// The corner's final `(i, j)` grid label.
        at: (i32, i32),
        /// Local-homography reprojection residual in pixels, when the
        /// corner was attached via a local-H stage; `None` otherwise.
        local_h_residual_px: Option<f32>,
    },
    /// Previously labelled at `at`, then blacklisted during
    /// post-validation. `reason` is human-readable for overlays.
    LabeledThenBlacklisted {
        /// The `(i, j)` cell the corner had been labelled at.
        at: (i32, i32),
        /// Human-readable reason the corner was blacklisted.
        reason: String,
    },
}

/// Augmented corner carried through the pipeline.
///
/// Wraps a reference-like snapshot of the input [`ChessCorner`] plus
/// per-stage state: cluster label, current stage, (i, j) label when
/// attached.
#[derive(Clone, Debug, Serialize)]
pub struct CornerAug {
    /// Index in the original input corner slice. Stable across all
    /// pipeline stages, used as the identity key for blacklists.
    pub input_index: usize,
    /// Pixel position (copied from `ChessCorner.position` at construction).
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
    /// Cluster label assigned in `cluster_axes` (`None` while `stage`
    /// is `Raw`, `Strong`, or `NoCluster`).
    pub label: Option<ClusterLabel>,
}

impl CornerAug {
    /// Build a fresh [`CornerAug`] from a [`ChessCorner`] input.
    pub fn from_chess_corner(input_index: usize, c: &ChessCorner) -> Self {
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
