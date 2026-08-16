//! The ChESS corner front-end pass and the per-corner state carried through
//! the pipeline.
//!
//! [`detect_corners`] is the workspace's single corner front-end: it runs
//! `chess-corners` over a [`GrayImageView`] and adapts every
//! [`chess_corners::CornerDescriptor`] into the [`ChessCorner`] the detectors
//! consume. Every whole-image entry point in the workspace — the composite
//! detectors' `detect` methods and the facade's `detect_*` helpers — routes
//! through it, so there is exactly one place where ChESS output becomes a
//! corner cloud.

use calib_targets_core::{AxisEstimate, DetectorConfig, GrayImageView};
use chess_corners::Detector as ChessDetector;
use nalgebra::Point2;
use serde::{Deserialize, Serialize};

/// Run the ChESS corner front-end over `image` and adapt the result into
/// [`ChessCorner`]s.
///
/// Operates on the image as supplied — no pre-blur, no upscale beyond what
/// `cfg` itself requests. Corner positions are returned in the input image
/// frame.
///
/// Returns an empty vector rather than an error in both failure modes: when
/// `cfg` is one `chess_corners::Detector::new` rejects, and when the corner
/// pass itself fails (e.g. a buffer whose length disagrees with
/// `width * height`). "No corners" is the correct downstream input in either
/// case — every detector treats an empty cloud as "no board here" — so the
/// front-end has no error channel of its own.
pub fn detect_corners(image: &GrayImageView<'_>, cfg: &DetectorConfig) -> Vec<ChessCorner> {
    let Ok(mut detector) = ChessDetector::new(*cfg) else {
        return Vec::new();
    };
    detector
        .detect_u8(image.data, image.width as u32, image.height as u32)
        .unwrap_or_default()
        .iter()
        .map(adapt_chess_corner)
        .collect()
}

/// Adapt one upstream [`chess_corners::CornerDescriptor`] into a
/// [`ChessCorner`].
fn adapt_chess_corner(c: &chess_corners::CornerDescriptor) -> ChessCorner {
    // `CornerDescriptor::axes` is `None` when the upstream orientation fit was
    // skipped (`DetectorConfig::without_orientation`). Every detection entry
    // point in this workspace leaves orientation enabled, so this is the
    // defensive branch rather than the common one — but it must not fabricate
    // a confident axis. `AxisEstimate::default()` is the workspace's existing
    // no-information sentinel (`sigma = π`), which every axis-aware stage
    // already treats as "skip this corner", so an orientation-free descriptor
    // degrades to a bare position instead of poisoning the grid with a
    // zero-sigma axis at angle 0.
    let axes = c.axes.map_or_else(
        || [AxisEstimate::default(); 2],
        |axes| {
            [
                AxisEstimate {
                    angle: axes[0].angle,
                    sigma: axes[0].sigma,
                },
                AxisEstimate {
                    angle: axes[1].angle,
                    sigma: axes[1].sigma,
                },
            ]
        },
    );
    ChessCorner::new(Point2::new(c.x, c.y), axes, c.response)
}

/// Canonical 2D corner consumed by the chessboard detector.
///
/// Not to be confused with [`ChessboardCorner`](crate::ChessboardCorner): this
/// is the raw ChESS feature fed *into* the detector, not the labelled grid
/// corner it returns.
///
/// Carries the per-corner data the pipeline needs to admit or reject the
/// corner during clustering, seed selection, grow, and post-grow validation:
/// pixel position, the two local grid-axis directions with per-axis 1σ
/// uncertainty, and the ChESS detector's response (`strength`).
///
/// Callers running the ChESS front-end go through [`detect_corners`], which
/// adapts `chess_corners::CornerDescriptor` into this type; callers handing the
/// detector a pre-built corner cloud (tests, custom upstreams) construct
/// `ChessCorner` directly.
///
/// The former `contrast` and `fit_rms` fields are gone. They mirrored
/// `CornerDescriptor` fields that `chess-corners` 1.0 removed, having declared
/// `response` the single detection-strength contract and `axes[i].sigma` the
/// per-axis confidence. Nothing downstream can reconstruct them, so the
/// `fit_rms ≤ max_fit_rms_ratio · contrast` prefilter they fed was retired
/// with them; [`Self::strength`] and the per-axis sigmas now carry the whole
/// corner-quality signal.
///
/// `#[non_exhaustive]`: construct with [`ChessCorner::new`] (or
/// [`ChessCorner::from_position`] for a position-only test fixture).
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ChessCorner {
    /// Corner position in pixel coordinates.
    pub position: Point2<f32>,
    /// Two local grid-axis directions with per-axis 1σ angular uncertainty.
    /// Default-constructed axes carry `sigma = π` ("no information") and
    /// cause the corner to be skipped by every axis-aware stage.
    pub axes: [AxisEstimate; 2],
    /// Corner detector response (raw ChESS response at the detected peak).
    /// Positive values are corner candidates.
    pub strength: f32,
}

impl ChessCorner {
    /// Construct a fully-specified [`ChessCorner`] from its pixel position, the
    /// two local grid-axis estimates, and the ChESS detector `strength`.
    ///
    /// This is the constructor a custom corner-cloud upstream uses;
    /// [`detect_corners`]' adapter for `chess_corners::CornerDescriptor` also
    /// routes through it.
    pub fn new(position: Point2<f32>, axes: [AxisEstimate; 2], strength: f32) -> Self {
        Self {
            position,
            axes,
            strength,
        }
    }

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
}

/// Stage marker tracked per input corner through the pipeline.
///
/// Every `Corner` the detector sees starts at [`CornerStage::Raw`]
/// and advances as it passes (or fails) pipeline stages. It is the
/// internal cursor the clustering, topological recovery, booster, and
/// geometry-check stages read and write to decide which corners are
/// eligible at each step.
#[non_exhaustive]
#[derive(Clone, Debug, Serialize)]
pub enum CornerStage {
    /// Newly ingested corner, no checks yet.
    Raw,
    /// Passed the strength + fit-quality + axes-validity pre-filter.
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
    /// Attached as a labelled corner. Filled in by the topological
    /// recovery / booster stages.
    Labeled {
        /// The corner's final `(i, j)` grid label.
        at: (i32, i32),
        /// Local-homography reprojection residual in pixels, when the
        /// corner was attached via a local-H stage; `None` otherwise.
        local_h_residual_px: Option<f32>,
    },
    /// Previously labelled at `at`, then dropped by the geometry check.
    /// `reason` is human-readable for overlays.
    LabeledThenBlacklisted {
        /// The `(i, j)` cell the corner had been labelled at.
        at: (i32, i32),
        /// Human-readable reason the corner was dropped.
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
        assert_eq!(ClusterLabel::Canonical.as_u8(), 0);
        assert_eq!(ClusterLabel::Swapped.as_u8(), 1);
    }
}
