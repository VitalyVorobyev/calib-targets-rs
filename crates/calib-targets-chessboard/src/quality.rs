//! Quality metrics for chessboard grid reconstructions.
//!
//! Computes per-frame metrics (G1–G4 in the `docs/grid_plan.md` quality-gate
//! table) without depending on per-corner ground truth. Used by the
//! `chessboard_sweep_3536119669` harness to track detection quality against
//! a committed baseline.
//!
//! The self-consistency gate (G2/G3) fits an ideal-grid homography from each
//! detected corner's integer `(i, j)` label to its pixel position, then reports
//! the residual distribution. For a correct planar detection this collapses to
//! sub-pixel RMS; false corners with wrong `(i, j)` labels contribute large
//! outliers that pull up the percentile metrics.

use crate::gridgraph::estimate_corner_local_steps;
use calib_targets_core::{estimate_homography_rect_to_img, Corner, TargetDetection};
use nalgebra::Point2;
use projective_grid::GridGraph;
use serde::Serialize;

/// Per-frame metrics for a chessboard detection.
///
/// The first five fields are the original pass/fail signals used by
/// [`VISIBLE_SUBSET_GATE_3536119669`]. The remaining fields are continuous
/// signals added by Phase A of the local-consistency overhaul (see
/// `docs/grid_plan.md` and the top-level plan file). They are all
/// `Option<...>` so callers that invoke [`score_frame`] without the extra
/// inputs (raw corner cloud, graph, image width) still get the old subset.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct GridFrameMetrics {
    /// Number of labelled corners in the detection.
    pub corner_count: usize,
    /// Bounding-box extent of assigned grid coords in the i and j directions.
    /// For a full 22×22 ChArUco board with 21×21 interior corners, this is
    /// `(21, 21)` on a perfect detection.
    pub extent_i: u32,
    pub extent_j: u32,
    /// Median residual (in pixels) of detected positions against the best-fit
    /// homography from grid coords → image coords. `None` when fewer than 4
    /// corners are available (below DLT rank).
    pub residual_median_px: Option<f32>,
    /// p95 residual (in pixels) under the same fit.
    pub residual_p95_px: Option<f32>,

    // -- Phase A continuous signals -------------------------------------
    /// Horizontal bounding-box extent of detected corner positions divided
    /// by image width. On the 3536119669 set the in-focus strip runs left
    /// to right, so this is a direct measure of spatial coverage.
    pub horizontal_coverage_frac: Option<f32>,
    /// Median angular residual (degrees) between accepted graph edges and
    /// the nearest axis-line of either endpoint. Lower is better.
    pub edge_axis_residual_median_deg: Option<f32>,
    /// p95 of the same residual.
    pub edge_axis_residual_p95_deg: Option<f32>,
    /// Coefficient-of-variation `(stdev / mean)` of per-corner local-step
    /// estimate (average of `step_u`, `step_v`) across corners with non-
    /// zero local-step confidence. A tight lattice should yield a small
    /// value (<10%); larger values indicate the step estimate is fighting
    /// a mix of board-scale and marker-internal-scale populations.
    pub local_step_cv: Option<f32>,
    /// Histogram of graph node degrees, bucketed as `[deg0, deg1, deg2,
    /// deg3, deg4+]`. A healthy interior chessboard has most nodes at
    /// degree 4.
    pub graph_degree_hist: Option<[u32; 5]>,

    // -- Phase B stubs (populated when B4/B5 land) ----------------------
    /// Median absolute residual (pixels) between each corner and the
    /// median of its local-homography predictions from 2×2 patches of
    /// labelled neighbours. `None` until Phase B4 lands.
    pub local_homography_residual_median_px: Option<f32>,
    /// p95 of the same residual.
    pub local_homography_residual_p95_px: Option<f32>,
    /// Fraction of accepted graph edges whose endpoints share the same
    /// orientation-cluster label. `None` until Phase B1/B2 lands.
    pub cluster_polarity_violation_rate: Option<f32>,
}

/// Thresholds for accepting a geometrically consistent visible grid subset.
#[derive(Clone, Copy, Debug)]
pub struct VisibleSubsetGate {
    pub min_corners: usize,
    pub min_extent_i: u32,
    pub min_extent_j: u32,
    pub max_residual_median_px: f32,
    pub max_residual_p95_px: f32,
}

/// Visible-subset quality gate for the 3536119669 ChArUco target sweep.
///
/// The 720x540 snaps are low-resolution, so a correct visible subset can have
/// median homography residuals around 0.4 px. The p95 limit remains the main
/// guard against mixed or false grid components.
pub const VISIBLE_SUBSET_GATE_3536119669: VisibleSubsetGate = VisibleSubsetGate {
    min_corners: 30,
    min_extent_i: 6,
    min_extent_j: 4,
    max_residual_median_px: 0.5,
    max_residual_p95_px: 1.0,
};

impl GridFrameMetrics {
    /// Whether the frame recovers ≥ `ratio` fraction of the expected lattice
    /// corners (G1 in the plan's gate table). `expected_total` is typically
    /// `expected_rows * expected_cols`.
    pub fn passes_detection_rate(&self, expected_total: u32, ratio: f32) -> bool {
        let total = expected_total as f32;
        total > 0.0 && (self.corner_count as f32) >= ratio * total
    }

    /// Whether the bounding-box extent meets a minimum `(ei, ej)` requirement
    /// (G4 in the gate table). Extents are 1-indexed — a 21×21 corner lattice
    /// has extent `(21, 21)`.
    pub fn passes_extent(&self, min_i: u32, min_j: u32) -> bool {
        self.extent_i >= min_i && self.extent_j >= min_j
    }

    /// Whether residuals meet G2/G3 thresholds.
    pub fn passes_residual(&self, median_limit_px: f32, p95_limit_px: f32) -> bool {
        match (self.residual_median_px, self.residual_p95_px) {
            (Some(med), Some(p95)) => med <= median_limit_px && p95 <= p95_limit_px,
            _ => false,
        }
    }

    /// Whether this detection passes a visible-subset gate.
    pub fn passes_visible_subset(&self, gate: VisibleSubsetGate) -> bool {
        self.corner_count >= gate.min_corners
            && self.extent_i >= gate.min_extent_i
            && self.extent_j >= gate.min_extent_j
            && self.passes_residual(gate.max_residual_median_px, gate.max_residual_p95_px)
    }
}

/// Compute per-frame quality metrics for a chessboard detection.
///
/// `expected_rows` and `expected_cols` are interior-corner counts (for a
/// 22×22 ChArUco board, pass `21, 21`). They are only used as a hint for the
/// extent-aspect check; the homography residual is computed directly against
/// the grid labels the detector assigned.
pub fn score_frame(
    detection: &TargetDetection,
    _expected_rows: u32,
    _expected_cols: u32,
) -> GridFrameMetrics {
    let corners = &detection.corners;
    let mut grid_pts = Vec::with_capacity(corners.len());
    let mut image_pts = Vec::with_capacity(corners.len());
    let mut min_i = i32::MAX;
    let mut max_i = i32::MIN;
    let mut min_j = i32::MAX;
    let mut max_j = i32::MIN;

    for c in corners {
        if let Some(g) = c.grid {
            grid_pts.push(Point2::new(g.i as f32, g.j as f32));
            image_pts.push(c.position);
            min_i = min_i.min(g.i);
            max_i = max_i.max(g.i);
            min_j = min_j.min(g.j);
            max_j = max_j.max(g.j);
        }
    }

    let (extent_i, extent_j) = if grid_pts.is_empty() {
        (0, 0)
    } else {
        (
            (max_i - min_i + 1).max(0) as u32,
            (max_j - min_j + 1).max(0) as u32,
        )
    };

    let (residual_median_px, residual_p95_px) = fit_residuals(&grid_pts, &image_pts);

    GridFrameMetrics {
        corner_count: corners.len(),
        extent_i,
        extent_j,
        residual_median_px,
        residual_p95_px,
        ..Default::default()
    }
}

/// Extended [`score_frame`] that also computes the Phase A continuous
/// signals (horizontal coverage, edge-axis residual distribution, local-
/// step CV, graph degree histogram).
///
/// `raw_corners` must contain the corner set fed into the graph builder
/// (i.e. after strength + orientation filters). `graph` must be the graph
/// that produced the detection. `image_width_px` sets the denominator of
/// `horizontal_coverage_frac`; pass the original frame width in pixels.
pub fn score_frame_full(
    detection: &TargetDetection,
    expected_rows: u32,
    expected_cols: u32,
    raw_corners: &[Corner],
    graph: &GridGraph,
    image_width_px: u32,
) -> GridFrameMetrics {
    let mut metrics = score_frame(detection, expected_rows, expected_cols);
    metrics.horizontal_coverage_frac = horizontal_coverage(&detection.corners, image_width_px)
        .or(metrics.horizontal_coverage_frac);
    let (axis_med, axis_p95) = edge_axis_residual_stats(raw_corners, graph);
    metrics.edge_axis_residual_median_deg = axis_med;
    metrics.edge_axis_residual_p95_deg = axis_p95;
    metrics.local_step_cv = local_step_cv(raw_corners);
    metrics.graph_degree_hist = Some(graph_degree_histogram(graph));
    metrics
}

fn horizontal_coverage(
    corners: &[calib_targets_core::LabeledCorner],
    image_width_px: u32,
) -> Option<f32> {
    if image_width_px == 0 {
        return None;
    }
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    for c in corners {
        min_x = min_x.min(c.position.x);
        max_x = max_x.max(c.position.x);
    }
    if !min_x.is_finite() || !max_x.is_finite() {
        return None;
    }
    let span = (max_x - min_x).max(0.0);
    Some((span / image_width_px as f32).clamp(0.0, 1.0))
}

fn edge_axis_residual_stats(corners: &[Corner], graph: &GridGraph) -> (Option<f32>, Option<f32>) {
    let mut residuals: Vec<f32> = Vec::new();
    for (src_idx, neighbors) in graph.neighbors.iter().enumerate() {
        let src = &corners[src_idx];
        for n in neighbors {
            // Count each undirected edge once.
            if n.index <= src_idx {
                continue;
            }
            let dst = &corners[n.index];
            let edge_angle =
                (dst.position.y - src.position.y).atan2(dst.position.x - src.position.x);
            let d_src = nearest_axis_line_diff(&src.axes, edge_angle);
            let d_dst = nearest_axis_line_diff(&dst.axes, edge_angle);
            residuals.push(d_src.to_degrees());
            residuals.push(d_dst.to_degrees());
        }
    }
    if residuals.len() < 2 {
        return (None, None);
    }
    residuals.sort_by(|a, b| a.total_cmp(b));
    (
        Some(percentile(&residuals, 0.5)),
        Some(percentile(&residuals, 0.95)),
    )
}

fn nearest_axis_line_diff(axes: &[calib_targets_core::AxisEstimate; 2], edge_angle: f32) -> f32 {
    let d0 = axis_vec_diff(axes[0].angle, edge_angle);
    let d1 = axis_vec_diff(axes[1].angle, edge_angle);
    d0.min(d1)
}

fn axis_vec_diff(axis_angle: f32, vec_angle: f32) -> f32 {
    use std::f32::consts::PI;
    let two_pi = 2.0 * PI;
    let mut diff = (vec_angle - axis_angle).rem_euclid(two_pi);
    if diff >= PI {
        diff -= two_pi;
    }
    let diff_abs = diff.abs();
    diff_abs.min(PI - diff_abs)
}

fn local_step_cv(corners: &[Corner]) -> Option<f32> {
    if corners.is_empty() {
        return None;
    }
    let steps = estimate_corner_local_steps(corners);
    let mut means: Vec<f32> = Vec::with_capacity(steps.len());
    for s in &steps {
        if s.confidence > 0.0 && s.step_u > 0.0 && s.step_v > 0.0 {
            means.push(0.5 * (s.step_u + s.step_v));
        }
    }
    if means.len() < 2 {
        return None;
    }
    let n = means.len() as f32;
    let mean = means.iter().copied().sum::<f32>() / n;
    if mean <= 0.0 {
        return None;
    }
    let var = means.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / n;
    Some(var.sqrt() / mean)
}

fn graph_degree_histogram(graph: &GridGraph) -> [u32; 5] {
    let mut hist = [0u32; 5];
    for neighbors in &graph.neighbors {
        let bucket = neighbors.len().min(4);
        hist[bucket] = hist[bucket].saturating_add(1);
    }
    hist
}

fn fit_residuals(
    grid_pts: &[Point2<f32>],
    image_pts: &[Point2<f32>],
) -> (Option<f32>, Option<f32>) {
    if grid_pts.len() < 4 {
        return (None, None);
    }
    let Some(h) = estimate_homography_rect_to_img(grid_pts, image_pts) else {
        return (None, None);
    };

    let mut errs: Vec<f32> = grid_pts
        .iter()
        .zip(image_pts.iter())
        .map(|(g, p)| {
            let pred = h.apply(*g);
            ((pred.x - p.x).powi(2) + (pred.y - p.y).powi(2)).sqrt()
        })
        .collect();
    errs.sort_by(|a, b| a.total_cmp(b));

    (Some(percentile(&errs, 0.5)), Some(percentile(&errs, 0.95)))
}

fn percentile(sorted: &[f32], q: f32) -> f32 {
    if sorted.is_empty() {
        return f32::NAN;
    }
    let idx = ((sorted.len() as f32 - 1.0) * q).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use calib_targets_core::{GridCoords, LabeledCorner, TargetKind};

    fn make_detection(corners: Vec<LabeledCorner>) -> TargetDetection {
        TargetDetection {
            kind: TargetKind::Chessboard,
            corners,
        }
    }

    fn labelled(i: i32, j: i32, x: f32, y: f32) -> LabeledCorner {
        LabeledCorner {
            position: Point2::new(x, y),
            grid: Some(GridCoords { i, j }),
            id: None,
            target_position: None,
            score: 1.0,
        }
    }

    #[test]
    fn perfect_grid_has_zero_residual() {
        // Identity homography with 10 px spacing, 4×4 grid.
        let mut corners = Vec::new();
        for j in 0..4 {
            for i in 0..4 {
                corners.push(labelled(i, j, i as f32 * 10.0, j as f32 * 10.0));
            }
        }
        let det = make_detection(corners);
        let m = score_frame(&det, 4, 4);
        assert_eq!(m.corner_count, 16);
        assert_eq!(m.extent_i, 4);
        assert_eq!(m.extent_j, 4);
        assert!(m.residual_median_px.unwrap() < 1e-3);
        assert!(m.residual_p95_px.unwrap() < 1e-3);
    }

    #[test]
    fn mislabelled_corners_inflate_residuals() {
        // Perfect 4×4 grid: residuals should be ~0.
        let perfect: Vec<LabeledCorner> = (0..4)
            .flat_map(|j| (0..4).map(move |i| labelled(i, j, i as f32 * 10.0, j as f32 * 10.0)))
            .collect();
        let perfect_m = score_frame(&make_detection(perfect.clone()), 4, 4);

        // Inject two mislabelled corners (swap labels at (1,2) and (2,2)).
        let mut corrupted = perfect;
        corrupted[2 * 4 + 1] = labelled(2, 2, 10.0, 20.0);
        corrupted[2 * 4 + 2] = labelled(1, 2, 20.0, 20.0);
        let corrupted_m = score_frame(&make_detection(corrupted), 4, 4);

        assert!(perfect_m.residual_median_px.unwrap() < 1e-3);
        assert!(
            corrupted_m.residual_median_px.unwrap() > perfect_m.residual_median_px.unwrap(),
            "median must rise when grid labels are corrupted (perfect={:?}, corrupted={:?})",
            perfect_m,
            corrupted_m,
        );
        assert!(corrupted_m.residual_p95_px.unwrap() > 1.0);
    }

    #[test]
    fn extent_reflects_bounding_box() {
        let corners = vec![
            labelled(2, 3, 0.0, 0.0),
            labelled(4, 3, 10.0, 0.0),
            labelled(2, 7, 0.0, 20.0),
            labelled(4, 7, 10.0, 20.0),
        ];
        let det = make_detection(corners);
        let m = score_frame(&det, 21, 21);
        assert_eq!(m.extent_i, 3);
        assert_eq!(m.extent_j, 5);
    }

    #[test]
    fn gate_helpers() {
        let m = GridFrameMetrics {
            corner_count: 350,
            extent_i: 20,
            extent_j: 21,
            residual_median_px: Some(0.25),
            residual_p95_px: Some(0.9),
            ..Default::default()
        };
        assert!(m.passes_detection_rate(441, 0.6));
        assert!(!m.passes_detection_rate(441, 0.8));
        assert!(m.passes_extent(18, 18));
        assert!(!m.passes_extent(21, 21));
        assert!(m.passes_residual(0.3, 1.0));
        assert!(!m.passes_residual(0.2, 1.0));
    }

    #[test]
    fn visible_subset_gate_accepts_low_res_consistent_grid() {
        let m = GridFrameMetrics {
            corner_count: 30,
            extent_i: 11,
            extent_j: 5,
            residual_median_px: Some(0.49),
            residual_p95_px: Some(0.99),
            ..Default::default()
        };

        assert!(m.passes_visible_subset(VISIBLE_SUBSET_GATE_3536119669));

        let outlier_heavy = GridFrameMetrics {
            residual_p95_px: Some(1.01),
            ..m
        };
        assert!(!outlier_heavy.passes_visible_subset(VISIBLE_SUBSET_GATE_3536119669));
    }

    #[test]
    fn horizontal_coverage_is_none_for_zero_width() {
        // Degenerate image width must not divide by zero.
        let corners = vec![labelled(0, 0, 100.0, 100.0), labelled(1, 0, 200.0, 100.0)];
        assert!(horizontal_coverage(&corners, 0).is_none());
    }

    #[test]
    fn horizontal_coverage_matches_bbox_over_image_width() {
        let corners = vec![
            labelled(0, 0, 100.0, 100.0),
            labelled(1, 0, 400.0, 100.0),
            labelled(2, 0, 700.0, 100.0),
        ];
        let frac = horizontal_coverage(&corners, 720).unwrap();
        // bbox = 600 px, width = 720 → 600/720 ≈ 0.8333
        assert!((frac - 0.8333).abs() < 1e-3);
    }

    #[test]
    fn degree_histogram_buckets_high_degree_into_bucket_4() {
        use projective_grid::{NeighborDirection, NodeNeighbor};
        let mut graph = GridGraph {
            neighbors: vec![vec![], vec![]],
        };
        // Node 0 has five neighbors (unusual: should land in bucket 4).
        for i in 0..5 {
            graph.neighbors[0].push(NodeNeighbor {
                index: i + 1,
                direction: NeighborDirection::Right,
                distance: 10.0,
                score: 0.0,
            });
        }
        let hist = graph_degree_histogram(&graph);
        assert_eq!(hist[4], 1); // node 0 → bucket 4
        assert_eq!(hist[0], 1); // node 1 → bucket 0 (no neighbors)
    }

    #[test]
    fn partial_board_detection_scores_without_full_board_dimensions() {
        let corners: Vec<LabeledCorner> = (0..5)
            .flat_map(|j| (0..8).map(move |i| labelled(i, j, i as f32 * 12.0, j as f32 * 12.0)))
            .collect();
        let det = make_detection(corners);
        let m = score_frame(&det, 21, 21);

        assert_eq!(m.corner_count, 40);
        assert_eq!(m.extent_i, 8);
        assert_eq!(m.extent_j, 5);
        assert!(m.residual_median_px.unwrap() < 1e-3);
        assert!(m.residual_p95_px.unwrap() < 1e-3);
        assert!(m.passes_visible_subset(VISIBLE_SUBSET_GATE_3536119669));
        assert!(!m.passes_detection_rate(441, 0.6));
    }
}
