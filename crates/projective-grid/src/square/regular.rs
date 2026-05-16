//! Zero-config regular square-grid detection from a bare point cloud.
//!
//! [`detect_regular_grid`] and [`RegularGridDetector`] turn a slice of
//! 2D points into a labelled `(i, j)` grid **without the caller writing
//! any validator scaffolding**. They are the onboarding entry point for
//! `projective-grid`: the pattern hooks ([`SeedQuadValidator`],
//! [`GrowValidator`], [`detect_square_grid`]) stay public under
//! [`crate::square`] for callers who need pattern-specific gates
//! (parity, marker slots), but a caller with only a point cloud should
//! reach for this module first.
//!
//! # What "regular" means here
//!
//! The detector assumes the points form a single roughly-uniform
//! square lattice (clean, rotated, or perspective-warped). It estimates
//! the global cell size and the two dominant grid-axis directions from
//! the cloud's nearest-neighbour offsets, then drives the generic
//! seed → grow → extend → fill → validate pipeline with an internal
//! **open regular-grid policy** that accepts any geometrically-valid
//! parallelogram seed and attachment. There is no colour, parity, or
//! marker reasoning — those belong to the pattern-specific detectors
//! built on top of [`detect_square_grid`].
//!
//! [`SeedQuadValidator`]: crate::square::seed::finder::SeedQuadValidator
//! [`GrowValidator`]: crate::square::grow::GrowValidator
//! [`detect_square_grid`]: crate::square::detect::detect_square_grid

use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;

use nalgebra::{Point2, Vector2};

use crate::circular_stats::{
    angle_to_bin, pick_two_peaks, smooth_circular_5, wrap_pi, PeakPickOptions,
};
use crate::global_step::{estimate_global_cell_size, GlobalStepParams};
use crate::square::alignment::GridTransform;
use crate::square::cleanup::{canonicalize_top_left, prune_to_main_component, sorted_grid_points};
use crate::square::detect::{
    detect_square_grid, detect_square_grid_all, ExtensionStrategy, MultiComponentParams,
    SquareGridParams,
};
use crate::square::extension::{ExtensionParams, LocalExtensionParams};
use crate::square::grow::{Admit, GrowValidator, LabelledNeighbour};
use crate::square::seed::finder::SeedQuadValidator;
use crate::topological::AxisEstimate;

/// Boundary-extension strategy used by [`detect_regular_grid`].
///
/// Boundary extension extrapolates the labelled set outward past the
/// BFS-grown bounding box. The two functional variants wrap the
/// pipeline's two homography-extension strategies; [`Self::Disabled`]
/// skips the stage entirely.
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub enum ExtensionMode {
    /// Skip boundary extension. The labelled set returned by BFS-grow
    /// (plus the hole-fill pass) stands as-is.
    Disabled,
    /// Fit one global homography over the whole labelled set and
    /// extrapolate from it. Cheaper, but refuses to extrapolate under
    /// heavy radial distortion or multi-region perspective.
    Global(ExtensionParams),
    /// Fit a per-candidate local homography from the nearest labelled
    /// corners. More compute, but materially better recall on
    /// extreme-angle inputs where a single global homography cannot
    /// fit. This is the default — see [`RegularGridParams::default`].
    Local(LocalExtensionParams),
}

impl Default for ExtensionMode {
    fn default() -> Self {
        // Local-H extension exists in `square::extension` and is the
        // strategy `detect_square_grid` exposes per-cell; it strictly
        // dominates global-H on recall for perspective-warped input
        // (see `square::extension` tests `local_h_reaches_further_than_global`).
        // A point-cloud caller with no distortion prior is best served
        // by the more tolerant strategy, so `Local` is the default.
        ExtensionMode::Local(LocalExtensionParams::default())
    }
}

/// Tuning knobs for [`RegularGridDetector`].
///
/// `#[non_exhaustive]`: new knobs may land in future releases. Build
/// fully-specified instances with [`RegularGridParams::new`] or start
/// from [`RegularGridParams::default`] and override fields.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct RegularGridParams {
    /// Core seed → grow → extend → fill → validate tuning. The internal
    /// regular-grid policy fills in the pattern hooks; this struct
    /// carries the geometric knobs only.
    ///
    /// The `extension` field of this struct is **ignored** — boundary
    /// extension is driven by [`Self::extension_mode`] instead, so the
    /// strategy choice and its parameters live in one place.
    pub pipeline: SquareGridParams,
    /// Boundary-extension strategy. See [`ExtensionMode`].
    pub extension_mode: ExtensionMode,
    /// When `true`, [`detect_regular_grid`] canonicalises the labelled
    /// grid to a visual top-left origin (`+i` → right, `+j` → down in
    /// pixel space) before returning. When `false`, the grid keeps the
    /// orientation BFS-grow produced (still rebased to `(0, 0)`).
    pub canonicalize_top_left: bool,
    /// When `true`, [`detect_regular_grid`] drops corners not
    /// 4-connected to the largest labelled component. Off-grid spurious
    /// points and bridged sub-grids both manifest as extra components;
    /// pruning is a pattern-agnostic precision guard.
    pub prune_disconnected: bool,
}

impl Default for RegularGridParams {
    fn default() -> Self {
        Self {
            pipeline: SquareGridParams::default(),
            extension_mode: ExtensionMode::default(),
            canonicalize_top_left: true,
            prune_disconnected: true,
        }
    }
}

impl RegularGridParams {
    /// Construct fully-specified params. The struct is
    /// `#[non_exhaustive]`, so this is the supported way to build one
    /// outside the crate.
    pub fn new(
        pipeline: SquareGridParams,
        extension_mode: ExtensionMode,
        canonicalize_top_left: bool,
        prune_disconnected: bool,
    ) -> Self {
        Self {
            pipeline,
            extension_mode,
            canonicalize_top_left,
            prune_disconnected,
        }
    }

    /// Override the boundary-extension strategy. Builder-style setter
    /// for use with [`RegularGridParams::default`] — the struct is
    /// `#[non_exhaustive]`, so struct-update syntax is not available
    /// to external crates.
    pub fn with_extension_mode(mut self, mode: ExtensionMode) -> Self {
        self.extension_mode = mode;
        self
    }

    /// Override the top-left canonicalisation toggle. Builder-style
    /// setter; see [`Self::with_extension_mode`].
    pub fn with_canonicalize_top_left(mut self, on: bool) -> Self {
        self.canonicalize_top_left = on;
        self
    }

    /// Override the connectivity-pruning toggle. Builder-style setter;
    /// see [`Self::with_extension_mode`].
    pub fn with_prune_disconnected(mut self, on: bool) -> Self {
        self.prune_disconnected = on;
        self
    }
}

/// One labelled point in a [`RegularGridDetection`].
///
/// Data carrier — fields are read directly; not `#[non_exhaustive]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetectedGridPoint {
    /// Integer grid coordinate `(i, j)`. Rebased so the labelled
    /// bounding box starts at `(0, 0)`.
    pub grid: (i32, i32),
    /// Pixel position of this corner (copied from the input slice).
    pub position: Point2<f32>,
    /// Index back into the caller's input `&[Point2<f32>]` slice.
    pub source_index: usize,
}

/// Per-stage diagnostics returned alongside a [`RegularGridDetection`].
///
/// `#[non_exhaustive]`: new counters may be added in future releases.
#[non_exhaustive]
#[derive(Clone, Debug, Default)]
pub struct RegularGridStats {
    /// Number of input points fed to the detector.
    pub input_points: usize,
    /// Number of distinct connected components considered before
    /// pruning. `1` on a clean single-board cloud.
    pub components_found: usize,
    /// Number of labelled corners in the chosen (largest) component
    /// before connectivity pruning ran.
    pub labelled_before_prune: usize,
    /// Number of corners dropped by connectivity pruning. `0` when
    /// pruning was disabled or the component was already connected.
    pub pruned_disconnected: usize,
    /// Number of corners flagged and dropped by the validation stage.
    pub dropped_by_validation: usize,
    /// `true` when the labelled grid was canonicalised to a visual
    /// top-left origin.
    pub canonicalized: bool,
}

/// Result of a regular-grid detection.
///
/// Data carrier — not `#[non_exhaustive]` (callers read fields and
/// build fixtures). Carries the labelled grid as a `(j, i)`-sorted
/// vector plus the inferred grid geometry and per-stage diagnostics.
#[derive(Clone, Debug)]
pub struct RegularGridDetection {
    /// Labelled corners sorted by `(j, i)` — row-major, top-to-bottom
    /// then left-to-right.
    pub points: Vec<DetectedGridPoint>,
    /// Pixel-space unit vector along the grid's `+i` direction.
    pub grid_u: Vector2<f32>,
    /// Pixel-space unit vector along the grid's `+j` direction.
    pub grid_v: Vector2<f32>,
    /// Estimated cell size in pixels (mean lattice spacing).
    pub cell_size: f32,
    /// Per-stage diagnostic counters.
    pub stats: RegularGridStats,
}

impl RegularGridDetection {
    /// Reconstruct the `(i, j) → source_index` map from [`Self::points`].
    pub fn labelled_map(&self) -> HashMap<(i32, i32), usize> {
        self.points
            .iter()
            .map(|p| (p.grid, p.source_index))
            .collect()
    }
}

/// Zero-config regular square-grid detection.
///
/// Equivalent to `RegularGridDetector::default().detect(points)`.
/// Returns `None` when no seed quad can be found (fewer than four
/// points, collinear cloud, or no roughly-square parallelogram in the
/// input).
///
/// # Example
///
/// ```rust
/// use nalgebra::Point2;
/// use projective_grid::detect_regular_grid;
///
/// // A clean 5×4 axis-aligned grid at 30 px pitch.
/// let mut points = Vec::new();
/// for j in 0..4 {
///     for i in 0..5 {
///         points.push(Point2::new(i as f32 * 30.0, j as f32 * 30.0));
///     }
/// }
///
/// let grid = detect_regular_grid(&points).expect("clean grid detects");
/// assert_eq!(grid.points.len(), 20);
/// // Labels are rebased so the bounding box starts at (0, 0).
/// assert!(grid.points.iter().any(|p| p.grid == (0, 0)));
/// ```
pub fn detect_regular_grid(points: &[Point2<f32>]) -> Option<RegularGridDetection> {
    RegularGridDetector::default().detect(points)
}

/// Configurable regular square-grid detector.
///
/// Holds a [`RegularGridParams`]. Use [`RegularGridDetector::default`]
/// for the zero-config path or construct one with custom params.
#[derive(Clone, Debug, Default)]
pub struct RegularGridDetector {
    /// Tuning knobs. See [`RegularGridParams`].
    pub params: RegularGridParams,
}

impl RegularGridDetector {
    /// Construct a detector with explicit params.
    pub fn new(params: RegularGridParams) -> Self {
        Self { params }
    }

    /// Detect a regular square grid in `points`.
    ///
    /// Runs the generic seed → grow → extend → fill → validate pipeline
    /// with an internal open regular-grid policy, applies generic
    /// output cleanup (connectivity pruning, top-left canonicalisation,
    /// `(j, i)` sort), and returns a [`RegularGridDetection`].
    ///
    /// Returns `None` when no seed quad is found.
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            level = "info",
            skip_all,
            fields(num_points = points.len()),
        )
    )]
    pub fn detect(&self, points: &[Point2<f32>]) -> Option<RegularGridDetection> {
        if points.len() < 4 {
            return None;
        }

        let policy = OpenRegularPolicy::new(points)?;
        let pipeline = self.pipeline_params();

        let detection = detect_square_grid(points, &policy, &policy, &pipeline)?;

        let mut stats = RegularGridStats {
            input_points: points.len(),
            components_found: 1,
            canonicalized: self.params.canonicalize_top_left,
            dropped_by_validation: detection.stats.dropped_by_validation,
            ..Default::default()
        };

        let labelled = detection.labelled;
        stats.labelled_before_prune = labelled.len();

        let labelled = if self.params.prune_disconnected {
            let pruned = prune_to_main_component(labelled);
            stats.pruned_disconnected = stats.labelled_before_prune - pruned.len();
            pruned
        } else {
            labelled
        };

        let (labelled, transform) = if self.params.canonicalize_top_left {
            canonicalize_top_left(labelled, points)
        } else {
            (labelled, GridTransform::IDENTITY)
        };

        // Map the grid basis vectors through the canonicalisation
        // transform so `grid_u` / `grid_v` stay consistent with the
        // returned labels.
        let (grid_u, grid_v) = transform_basis(detection.grid_u, detection.grid_v, transform);

        Some(build_detection(
            &labelled,
            points,
            grid_u,
            grid_v,
            detection.cell_size,
            stats,
        ))
    }

    /// Detect every disjoint regular grid in `points`.
    ///
    /// Multi-component variant of [`Self::detect`]: peels off one
    /// component at a time and returns each as its own
    /// [`RegularGridDetection`], in detection order. Each component is
    /// cleaned up independently (pruned, canonicalised, sorted).
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            level = "info",
            skip_all,
            fields(num_points = points.len()),
        )
    )]
    pub fn detect_all(&self, points: &[Point2<f32>]) -> Vec<RegularGridDetection> {
        if points.len() < 4 {
            return Vec::new();
        }
        let Some(policy) = OpenRegularPolicy::new(points) else {
            return Vec::new();
        };
        let pipeline = self.pipeline_params();

        let raw = detect_square_grid_all(
            points,
            &policy,
            &policy,
            &pipeline,
            &MultiComponentParams::default(),
        );
        let components_found = raw.len();

        raw.into_iter()
            .map(|detection| {
                let mut stats = RegularGridStats {
                    input_points: points.len(),
                    components_found,
                    canonicalized: self.params.canonicalize_top_left,
                    dropped_by_validation: detection.stats.dropped_by_validation,
                    ..Default::default()
                };
                let labelled = detection.labelled;
                stats.labelled_before_prune = labelled.len();

                let labelled = if self.params.prune_disconnected {
                    let pruned = prune_to_main_component(labelled);
                    stats.pruned_disconnected = stats.labelled_before_prune - pruned.len();
                    pruned
                } else {
                    labelled
                };
                let (labelled, transform) = if self.params.canonicalize_top_left {
                    canonicalize_top_left(labelled, points)
                } else {
                    (labelled, GridTransform::IDENTITY)
                };
                let (grid_u, grid_v) =
                    transform_basis(detection.grid_u, detection.grid_v, transform);
                build_detection(
                    &labelled,
                    points,
                    grid_u,
                    grid_v,
                    detection.cell_size,
                    stats,
                )
            })
            .collect()
    }

    /// Build the [`SquareGridParams`] actually handed to the generic
    /// pipeline, applying [`RegularGridParams::extension_mode`] onto
    /// the `extension` field.
    fn pipeline_params(&self) -> SquareGridParams {
        let mut pipeline = self.params.pipeline.clone();
        // The extension strategy is driven by `extension_mode`. Each
        // `ExtensionMode` variant maps onto the matching
        // `ExtensionStrategy` variant `detect_square_grid` consumes, so
        // `ExtensionMode::Local` now runs the genuine local-H pass.
        pipeline.extension = match self.params.extension_mode {
            ExtensionMode::Disabled => ExtensionStrategy::Disabled,
            ExtensionMode::Global(p) => ExtensionStrategy::Global(p),
            ExtensionMode::Local(p) => ExtensionStrategy::Local(p),
        };
        pipeline
    }
}

/// Assemble a [`RegularGridDetection`] from a cleaned labelled map.
fn build_detection(
    labelled: &HashMap<(i32, i32), usize>,
    points: &[Point2<f32>],
    grid_u: Vector2<f32>,
    grid_v: Vector2<f32>,
    cell_size: f32,
    stats: RegularGridStats,
) -> RegularGridDetection {
    let detected: Vec<DetectedGridPoint> = sorted_grid_points(labelled)
        .into_iter()
        .map(|(grid, idx)| DetectedGridPoint {
            grid,
            position: points[idx],
            source_index: idx,
        })
        .collect();
    RegularGridDetection {
        points: detected,
        grid_u,
        grid_v,
        cell_size,
        stats,
    }
}

/// Map the grid basis vectors through a D4 canonicalisation transform.
///
/// The transform acts on integer grid coordinates; its action on the
/// pixel-space basis is the same `2×2` integer matrix applied to the
/// `(u, v)` columns. The result is renormalised.
fn transform_basis(
    grid_u: Vector2<f32>,
    grid_v: Vector2<f32>,
    transform: GridTransform,
) -> (Vector2<f32>, Vector2<f32>) {
    // The new +i grid direction is `inv·(1, 0)` in old grid coords, so
    // its pixel image is `gi.i * u + gi.j * v`; likewise for +j.
    let inv = transform.inverse().unwrap_or(GridTransform::IDENTITY);
    let gi = inv.apply(1, 0);
    let gj = inv.apply(0, 1);
    let new_u = grid_u * gi.i as f32 + grid_v * gi.j as f32;
    let new_v = grid_u * gj.i as f32 + grid_v * gj.j as f32;
    let norm_u = new_u.norm().max(1e-6);
    let norm_v = new_v.norm().max(1e-6);
    (new_u / norm_u, new_v / norm_v)
}

// ---------------------------------------------------------------------------
// Open regular-grid policy: the built-in `SeedQuadValidator` +
// `GrowValidator` impl that accepts any geometrically-valid seed and
// attachment. This is what frees a point-cloud caller from writing
// validator scaffolding — it is the promotion of the `OpenValidator` /
// `ToySeedValidator` idea from the advanced-policy smoke test into the
// crate's built-in regular-grid policy.
// ---------------------------------------------------------------------------

/// Pattern-agnostic seed + grow policy for a single regular grid.
///
/// Holds the input positions and the two estimated grid-axis
/// directions. Every corner is eligible as both an `A`/`D` and a `B`/`C`
/// seed candidate (a regular grid has no colour split), every
/// attachment is accepted, and no parity / edge constraint is imposed —
/// the generic geometric checks inside `find_quad` / `bfs_grow` carry
/// the recovery.
struct OpenRegularPolicy {
    positions: Vec<Point2<f32>>,
    axes: [AxisEstimate; 2],
}

impl OpenRegularPolicy {
    /// Build the policy, estimating the grid axes from the cloud's
    /// nearest-neighbour offsets. Returns `None` when the cloud is too
    /// small or degenerate to infer an axis pair.
    fn new(points: &[Point2<f32>]) -> Option<Self> {
        let axes = estimate_grid_axes(points)?;
        Some(Self {
            positions: points.to_vec(),
            axes,
        })
    }
}

impl SeedQuadValidator for OpenRegularPolicy {
    fn position(&self, idx: usize) -> Point2<f32> {
        self.positions[idx]
    }

    fn axes(&self, _idx: usize) -> [AxisEstimate; 2] {
        // Every corner shares the globally-estimated axis pair: a
        // regular grid has one dominant orientation.
        self.axes
    }

    fn a_candidates(&self) -> Vec<usize> {
        // A regular grid has no colour split — every corner can serve
        // as the seed's A/D corner.
        (0..self.positions.len()).collect()
    }

    fn bc_candidates(&self) -> Vec<usize> {
        // ...and likewise as a B/C corner. `find_quad` rejects the
        // degenerate `A == B` / `A == C` cases internally.
        (0..self.positions.len()).collect()
    }
}

impl GrowValidator for OpenRegularPolicy {
    fn is_eligible(&self, _idx: usize) -> bool {
        true
    }

    fn required_label_at(&self, _i: i32, _j: i32) -> Option<u8> {
        None
    }

    fn label_of(&self, _idx: usize) -> Option<u8> {
        None
    }

    fn accept_candidate(
        &self,
        _idx: usize,
        _at: (i32, i32),
        _prediction: Point2<f32>,
        _neighbours: &[LabelledNeighbour],
    ) -> Admit {
        Admit::Accept
    }
}

/// Estimate the two dominant grid-axis directions from a point cloud.
///
/// Builds a weighted mod-π histogram of every corner's nearest-
/// neighbour offset angle, smooths it, and picks the two strongest
/// plateau-aware peaks. Falls back to the axis-aligned `(0, π/2)` pair
/// when the histogram has no two qualifying peaks (e.g. an exactly
/// axis-aligned grid produces a single sharp peak — the orthogonal
/// direction is implied).
fn estimate_grid_axes(points: &[Point2<f32>]) -> Option<[AxisEstimate; 2]> {
    use kiddo::{KdTree, SquaredEuclidean};

    if points.len() < 4 {
        return None;
    }
    // A cell-size estimate confirms the cloud is grid-like; it is not
    // used numerically here but guards against pure noise.
    estimate_global_cell_size(points, &GlobalStepParams::<f32>::default())?;

    let mut tree: KdTree<f32, 2> = KdTree::new();
    for (idx, p) in points.iter().enumerate() {
        tree.add(&[p.x, p.y], idx as u64);
    }

    const N_BINS: usize = 180;
    let mut hist = vec![0.0_f32; N_BINS];
    let mut total = 0.0_f32;
    for (i, p) in points.iter().enumerate() {
        // The four nearest neighbours capture both grid axes even when
        // the closest neighbour all lie along one direction.
        let hits = tree.nearest_n::<SquaredEuclidean>(&[p.x, p.y], 5);
        for hit in hits {
            let j = hit.item as usize;
            if j == i {
                continue;
            }
            let q = points[j];
            let off = Vector2::new(q.x - p.x, q.y - p.y);
            let len = off.norm();
            if len < 1e-3 {
                continue;
            }
            let ang = wrap_pi(off.y.atan2(off.x));
            let bin = angle_to_bin(ang, N_BINS);
            // Weight by length so the lattice step dominates over any
            // sub-cell marker spacing.
            hist[bin] += len;
            total += len;
        }
    }
    if total <= 0.0 {
        return None;
    }

    let smoothed = smooth_circular_5(&hist);
    let opts = PeakPickOptions::new(0.05, 30.0_f32.to_radians());
    match pick_two_peaks(&smoothed, total, &opts) {
        Some((t0, t1)) => {
            // Order so axis 0 is the smaller angle, axis 1 the larger,
            // matching the `SeedQuadValidator::axes` contract.
            let (lo, hi) = if t0 <= t1 { (t0, t1) } else { (t1, t0) };
            Some([AxisEstimate::from_angle(lo), AxisEstimate::from_angle(hi)])
        }
        None => {
            // A single dominant direction: the orthogonal axis is
            // implied. Pick the strongest bin and add π/2.
            let peak = smoothed
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .map(|(b, _)| b)?;
            let theta = wrap_pi(crate::circular_stats::bin_to_angle(peak, N_BINS));
            let other = wrap_pi(theta + FRAC_PI_2);
            let (lo, hi) = if theta <= other {
                (theta, other)
            } else {
                (other, theta)
            };
            Some([AxisEstimate::from_angle(lo), AxisEstimate::from_angle(hi)])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nalgebra::Matrix3;

    fn axis_aligned_grid(rows: i32, cols: i32, s: f32) -> Vec<Point2<f32>> {
        let mut out = Vec::new();
        for j in 0..rows {
            for i in 0..cols {
                out.push(Point2::new(i as f32 * s + 40.0, j as f32 * s + 40.0));
            }
        }
        out
    }

    #[test]
    fn detects_clean_axis_aligned_grid() {
        let pts = axis_aligned_grid(6, 6, 25.0);
        let grid = detect_regular_grid(&pts).expect("clean grid detects");
        assert_eq!(grid.points.len(), 36);
        assert_eq!(grid.stats.input_points, 36);
    }

    #[test]
    fn returns_none_on_collinear_cloud() {
        let pts: Vec<Point2<f32>> = (0..6).map(|i| Point2::new(i as f32 * 10.0, 0.0)).collect();
        assert!(detect_regular_grid(&pts).is_none());
    }

    #[test]
    fn estimate_grid_axes_recovers_rotation() {
        // 5×5 grid rotated by ~30°.
        let theta = 30.0_f32.to_radians();
        let (c, s) = (theta.cos(), theta.sin());
        let mut pts = Vec::new();
        for j in 0..5 {
            for i in 0..5 {
                let (x, y) = (i as f32 * 20.0, j as f32 * 20.0);
                pts.push(Point2::new(x * c - y * s + 100.0, x * s + y * c + 100.0));
            }
        }
        let axes = estimate_grid_axes(&pts).expect("axes");
        // One of the two axes should sit near 30° (mod π).
        let near = axes
            .iter()
            .any(|a| crate::circular_stats::angular_dist_pi(a.angle, theta) < 0.15);
        assert!(near, "expected an axis near 30°, got {axes:?}");
    }

    #[test]
    fn perspective_warped_grid_is_recovered() {
        let h = Matrix3::new(30.0_f32, 3.0, 50.0, 1.5, 30.0, 50.0, 2e-4, 1e-4, 1.0);
        let mut pts = Vec::new();
        for j in 0..7 {
            for i in 0..7 {
                let (x, y) = (i as f32, j as f32);
                let w = h[(2, 0)] * x + h[(2, 1)] * y + h[(2, 2)];
                let xp = (h[(0, 0)] * x + h[(0, 1)] * y + h[(0, 2)]) / w;
                let yp = (h[(1, 0)] * x + h[(1, 1)] * y + h[(1, 2)]) / w;
                pts.push(Point2::new(xp, yp));
            }
        }
        let grid = detect_regular_grid(&pts).expect("warped grid detects");
        assert!(grid.points.len() >= 40, "got {}", grid.points.len());
    }
}
