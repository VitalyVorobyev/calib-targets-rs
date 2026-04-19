use crate::gridgraph::{
    assign_grid_coordinates, build_chessboard_grid_graph_instrumented, connected_components,
    estimate_corner_local_steps, RejectionCounter,
};
use crate::params::{ChessboardParams, LocalHomographyPruneParams};
use crate::quality::{score_frame_full, GridFrameMetrics};
use calib_targets_core::{
    cluster_orientations, estimate_grid_axes_from_orientations, estimate_homography_rect_to_img,
    AxisEstimate, Corner, GridCoords, LabeledCorner, OrientationHistogram, TargetDetection,
    TargetKind,
};
use log::{debug, warn};
use nalgebra::Point2;
use projective_grid::graph_cleanup::{
    enforce_symmetry, prune_by_edge_straightness, prune_crossing_edges,
};
use projective_grid::{GridGraph, GridIndex, NeighborDirection};
use serde::Serialize;
use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;

#[cfg(feature = "tracing")]
use tracing::instrument;

/// Simple chessboard detector using ChESS orientations + grid fitting in (u, v) space.
#[derive(Debug)]
pub struct ChessboardDetector {
    pub params: ChessboardParams,
}

/// Applies the P1.3 insurance filter: reject corners whose two-axis tanh fit
/// is clearly mismatched with their reported amplitude. Corners whose `contrast`
/// field is missing (legacy pipeline, default-populated) always pass so we do
/// not regress callers that have not migrated to the 0.6 descriptor.
#[inline]
fn passes_fit_quality(c: &Corner, max_ratio: f32) -> bool {
    if !max_ratio.is_finite() {
        return true;
    }
    if c.contrast <= 0.0 {
        return true;
    }
    c.fit_rms <= max_ratio * c.contrast
}

#[derive(Debug, Serialize)]
pub struct ChessboardDetectionResult {
    pub detection: TargetDetection,
    pub inliers: Vec<usize>,
    pub orientations: Option<[f32; 2]>,
    pub debug: ChessboardDebug,
}

/// Per-stage counts collected during a single chessboard detection run.
///
/// Populated by [`ChessboardDetector::detect_instrumented`] and
/// [`ChessboardDetector::detect_all_instrumented`]. The plain
/// `detect_from_corners` / `detect_all_from_corners` entry points do not
/// emit this struct for back-compat.
///
/// Every field reports a `usize` count except
/// [`ChessboardStageCounts::edges_by_reject_reason`] which bucketizes
/// per-reason rejection counts from the graph build.
#[derive(Clone, Debug, Default, Serialize)]
pub struct ChessboardStageCounts {
    /// Corners passed into the detector.
    pub raw_corners: usize,
    /// Corners surviving the strength + fit-quality filter.
    pub after_strength_filter: usize,
    /// Corners surviving the orientation-cluster filter. `None` when
    /// clustering was disabled or fell through to the fallback estimate.
    pub after_orientation_cluster_filter: Option<usize>,
    /// Number of nodes in the constructed graph (== `after_strength_filter`
    /// or `after_orientation_cluster_filter` depending on pipeline path).
    pub graph_nodes: usize,
    /// Number of directed edges retained in the graph (each undirected edge
    /// counts twice).
    pub graph_edges: usize,
    /// Per-reason counts of candidate edges the validator rejected during
    /// graph build. Keys match [`EdgeRejectReason::as_str`].
    pub edges_by_reject_reason: HashMap<String, u64>,
    /// Number of connected components with at least one node.
    pub num_components: usize,
    /// Size of the largest connected component (in nodes).
    pub largest_component_size: usize,
    /// Number of corners for which BFS assigned an `(i, j)` coordinate in
    /// the primary (largest qualifying) component.
    pub assigned_grid_corners: usize,
    /// Phase B stub: corners surviving the local-homography consistency
    /// prune. `None` until B5 lands.
    pub after_local_homography_prune: Option<usize>,
    /// Corners surviving the global-homography residual prune. `None` when
    /// the pipeline took an early-exit branch before pruning ran.
    pub after_global_homography_prune: Option<usize>,
    /// Corners in the final [`TargetDetection`]. Zero when detection failed.
    pub final_labeled_corners: usize,

    /// Phase 2 cleanup: directed edges dropped because the reverse was missing.
    #[serde(default)]
    pub cleanup_asymmetric_edges: usize,
    /// Phase 2 cleanup: directed edges dropped because the Right/Left or
    /// Up/Down pair at the source bent by more than `max_straightness_deg`.
    #[serde(default)]
    pub cleanup_bent_edges: usize,
    /// Phase 2 cleanup: directed edges dropped because the undirected edge
    /// crosses another undirected edge in the graph.
    #[serde(default)]
    pub cleanup_crossing_edges: usize,

    /// Phase 5 gap-fill: corners attached by predicting missing `(i, j)`
    /// positions from labelled neighbors.
    #[serde(default)]
    pub gap_filled_corners: usize,
}

impl ChessboardStageCounts {
    fn from_counter(counter: &RejectionCounter) -> HashMap<String, u64> {
        counter
            .counts
            .iter()
            .map(|(reason, &count)| (reason.as_str().to_string(), count))
            .collect()
    }
}

/// Pair returned by the instrumented detector entry points: optional
/// detection result plus per-stage counters (always populated).
#[derive(Debug, Serialize)]
pub struct ChessboardInstrumentedResult {
    pub result: Option<ChessboardDetectionResult>,
    pub counts: ChessboardStageCounts,
}

/// Pair returned by the instrumented multi-component entry point: list of
/// detection results plus per-stage counters (always populated).
#[derive(Debug, Serialize)]
pub struct ChessboardInstrumentedResults {
    pub results: Vec<ChessboardDetectionResult>,
    pub counts: ChessboardStageCounts,
}

/// A single strong corner enriched with every signal needed by the Python
/// overlay script: both axis estimates, orientation-cluster label,
/// per-corner local step.
///
/// Index `i` in [`ChessboardDebugFrame::strong_corners`] corresponds to the
/// same index in [`ChessboardDebugFrame::graph_neighbors`].
#[derive(Clone, Debug, Serialize)]
pub struct DebugCorner {
    pub x: f32,
    pub y: f32,
    pub axes: [AxisEstimate; 2],
    /// Single-axis summary of the corner's orientation, derived at debug
    /// emission time as `axes[0].angle` (mod π). Kept as a JSON field so
    /// overlay scripts written against older schemas continue to work.
    pub orientation: f32,
    pub orientation_cluster: Option<usize>,
    pub strength: f32,
    pub contrast: f32,
    pub fit_rms: f32,
    pub local_step_u: f32,
    pub local_step_v: f32,
    pub local_step_confidence: f32,
}

/// A graph neighbor entry (flat, serde-friendly).
#[derive(Clone, Debug, Serialize)]
pub struct DebugGraphEdge {
    pub dst: usize,
    pub direction: &'static str,
    pub distance: f32,
    pub score: f32,
}

/// Full debug payload for a single chessboard detection.
///
/// Emitted by [`ChessboardDetector::detect_debug_from_corners`] and the
/// facade function `detect_chessboard_debug`. Consumed by the Python
/// overlay script and stored as JSON alongside sweep output.
///
/// The payload is **flat and self-contained** — every index in
/// [`Self::strong_corners`] is shared by [`Self::graph_neighbors`] and
/// [`Self::cluster_labels`]. Labelled coordinates (`grid`) live inside
/// `result.detection.corners`; the `strong_corner_index` map resolves
/// each labelled corner back to its position in `strong_corners`.
#[derive(Debug, Serialize)]
pub struct ChessboardDebugFrame {
    /// Source image dimensions — needed to compute horizontal coverage
    /// and render overlays at the right scale.
    pub image_width: u32,
    pub image_height: u32,

    /// Strong corners (post strength + fit-quality filter and, when
    /// clustering was applied, post orientation-cluster filter). Every
    /// index in this list is a node in the graph.
    pub strong_corners: Vec<DebugCorner>,
    /// Per-node adjacency list, parallel to [`Self::strong_corners`].
    pub graph_neighbors: Vec<Vec<DebugGraphEdge>>,

    /// Grid diagonals estimated by orientation clustering (or the
    /// fallback dominant-axis estimator).
    pub orientations: Option<[f32; 2]>,
    /// Smoothed orientation histogram used by the clusterer.
    pub orientation_histogram: Option<OrientationHistogram>,

    /// Per-stage counts (incl. per-reason rejection map).
    pub stage_counts: ChessboardStageCounts,
    /// Continuous quality metrics (Phase A set, plus stubs for Phase B).
    pub metrics: GridFrameMetrics,

    /// Successful detection result (if any). Contains labelled corners
    /// with `(i, j)` coordinates in `detection.corners[k].grid`.
    pub result: Option<ChessboardDetectionResult>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ChessboardDebug {
    pub orientation_histogram: Option<OrientationHistogram>,
    pub graph: Option<GridGraphDebug>,
}

#[derive(Clone, Debug, Serialize)]
pub struct GridGraphDebug {
    pub nodes: Vec<GridGraphNodeDebug>,
}

#[derive(Clone, Debug, Serialize)]
pub struct GridGraphNodeDebug {
    pub position: [f32; 2],
    pub neighbors: Vec<GridGraphNeighborDebug>,
}

#[derive(Clone, Debug, Serialize)]
pub struct GridGraphNeighborDebug {
    pub index: usize,
    pub direction: &'static str,
    pub distance: f32,
}

impl ChessboardDetector {
    pub fn new(params: ChessboardParams) -> Self {
        Self { params }
    }

    /// Main entry point: find chessboard(s) in a cloud of ChESS corners.
    ///
    /// This function expects corners already computed by your ChESS crate.
    /// For now it returns at most one detection (the best-scoring grid component).
    #[cfg_attr(feature = "tracing", instrument(level = "info", skip(self, corners), fields(num_corners=corners.len())))]
    pub fn detect_from_corners(&self, corners: &[Corner]) -> Option<ChessboardDetectionResult> {
        self.detect_instrumented(corners).result
    }

    /// Instrumented variant of [`Self::detect_from_corners`].
    ///
    /// Returns the optional detection result alongside per-stage counters
    /// ([`ChessboardStageCounts`]). Counters are populated even when the
    /// detector bails out early (e.g. below `min_corners`), so callers can
    /// diagnose *why* detection failed.
    #[cfg_attr(feature = "tracing", instrument(level = "info", skip(self, corners), fields(num_corners=corners.len())))]
    pub fn detect_instrumented(&self, corners: &[Corner]) -> ChessboardInstrumentedResult {
        let mut all = self.detect_all_instrumented(corners);
        let result = all.results.drain(..).next();
        ChessboardInstrumentedResult {
            result,
            counts: all.counts,
        }
    }

    /// Return detections for **all** qualifying grid components, sorted by
    /// corner count (largest first).
    ///
    /// This is the multi-component counterpart of [`Self::detect_from_corners`].
    /// Callers that can merge multiple components (e.g. the ChArUco detector)
    /// should prefer this method.
    #[cfg_attr(feature = "tracing", instrument(level = "info", skip(self, corners), fields(num_corners=corners.len())))]
    pub fn detect_all_from_corners(&self, corners: &[Corner]) -> Vec<ChessboardDetectionResult> {
        self.detect_all_instrumented(corners).results
    }

    /// Instrumented variant of [`Self::detect_all_from_corners`].
    #[cfg_attr(feature = "tracing", instrument(level = "info", skip(self, corners), fields(num_corners=corners.len())))]
    pub fn detect_all_instrumented(&self, corners: &[Corner]) -> ChessboardInstrumentedResults {
        let mut counts = ChessboardStageCounts {
            raw_corners: corners.len(),
            ..Default::default()
        };

        // 1. Filter by strength and by the optional fit-quality guard.
        let mut strong: Vec<Corner> = corners
            .iter()
            .filter(|c| c.strength >= self.params.min_corner_strength)
            .filter(|c| passes_fit_quality(c, self.params.max_fit_rms_ratio))
            .cloned()
            .collect();

        counts.after_strength_filter = strong.len();
        debug!(
            "found {} raw ChESS corners after strength filter",
            strong.len()
        );

        if strong.len() < self.params.min_corners {
            debug!(
                "rejecting chessboard before graph build: {} corners < min_corners={}",
                strong.len(),
                self.params.min_corners
            );
            return ChessboardInstrumentedResults {
                results: Vec::new(),
                counts,
            };
        }

        // 2. Estimate grid axes from orientations.
        let mut grid_diagonals = None;
        let mut graph_diagonals = None;
        let mut orientation_histogram = None;
        let mut clustering_ran = false;

        if self.params.use_orientation_clustering {
            if let Some(clusters) =
                cluster_orientations(&strong, &self.params.orientation_clustering_params)
            {
                clustering_ran = true;
                orientation_histogram = clusters.histogram;
                grid_diagonals = Some(clusters.centers);
                graph_diagonals = grid_diagonals;
                strong = strong
                    .into_iter()
                    .zip(clusters.labels)
                    .filter_map(|(mut corner, label)| {
                        label.map(|cluster| {
                            corner.orientation_cluster = Some(cluster);
                            corner
                        })
                    })
                    .collect();
            }
        }
        if clustering_ran {
            counts.after_orientation_cluster_filter = Some(strong.len());
        }

        if grid_diagonals.is_none() {
            warn!("Orientation clustering failed. Fallback to a simple estimate");
            if let Some(theta) = estimate_grid_axes_from_orientations(&strong) {
                let c0 = wrap_angle_pi(theta);
                let c1 = wrap_angle_pi(theta + FRAC_PI_2);
                grid_diagonals = Some([c0, c1]);
            }
        }

        if let Some(diagonals) = grid_diagonals {
            let mut cluster_counts = [0usize; 2];
            for corner in &strong {
                if let Some(cluster) = corner.orientation_cluster {
                    if let Some(slot) = cluster_counts.get_mut(cluster) {
                        *slot += 1;
                    }
                }
            }
            debug!(
                "grid diagonals estimated at {:.1} deg / {:.1} deg; orientation cluster counts = [{}, {}]",
                diagonals[0].to_degrees(),
                diagonals[1].to_degrees(),
                cluster_counts[0],
                cluster_counts[1]
            );
        }

        debug!(
            "kept {} ChESS corners after orientation consistency filter",
            strong.len()
        );

        if strong.len() < self.params.min_corners {
            debug!(
                "rejecting chessboard after orientation filtering: {} corners < min_corners={}",
                strong.len(),
                self.params.min_corners
            );
            return ChessboardInstrumentedResults {
                results: Vec::new(),
                counts,
            };
        }

        let mut rejection_counter = RejectionCounter::default();
        let mut graph = build_chessboard_grid_graph_instrumented(
            &strong,
            &self.params.graph,
            graph_diagonals,
            Some(&mut rejection_counter),
        );

        counts.graph_nodes = graph.neighbors.len();
        counts.graph_edges = graph.neighbors.iter().map(|n| n.len()).sum();
        counts.edges_by_reject_reason = ChessboardStageCounts::from_counter(&rejection_counter);

        // Phase 2: geometric-sanity graph cleanup.
        self.run_graph_cleanup(&mut graph, &strong, &mut counts);

        let components = connected_components(&graph);
        counts.num_components = components.len();
        counts.largest_component_size = components.iter().map(|c| c.len()).max().unwrap_or(0);

        log_graph_summary(&graph, &components, self.params.min_corners);
        debug!(
            "found {} connected grid components after orientation filtering",
            components.len()
        );

        let results = self.collect_components(
            &graph,
            &components,
            &strong,
            grid_diagonals,
            orientation_histogram,
            &mut counts,
        );

        counts.final_labeled_corners = results
            .first()
            .map(|r| r.detection.corners.len())
            .unwrap_or(0);

        ChessboardInstrumentedResults { results, counts }
    }

    /// Run the instrumented pipeline and collect a full debug frame.
    ///
    /// Unlike [`Self::detect_instrumented`], this entry point also captures
    /// the strong-corner list (with both axes, cluster label, per-corner
    /// local step) and the full neighbor graph in a flat, JSON-friendly
    /// shape. Intended for the Python overlay script and for per-frame
    /// debug dumps.
    ///
    /// `image_width_px` and `image_height_px` are the source image
    /// dimensions; they set the denominator for the `horizontal_coverage`
    /// metric and are copied into the debug frame for overlay rendering.
    #[cfg_attr(feature = "tracing", instrument(level = "info", skip(self, corners), fields(num_corners=corners.len())))]
    pub fn detect_debug_from_corners(
        &self,
        corners: &[Corner],
        image_width_px: u32,
        image_height_px: u32,
    ) -> ChessboardDebugFrame {
        let mut counts = ChessboardStageCounts {
            raw_corners: corners.len(),
            ..Default::default()
        };

        // Stage 1: strength + fit-quality filter.
        let mut strong: Vec<Corner> = corners
            .iter()
            .filter(|c| c.strength >= self.params.min_corner_strength)
            .filter(|c| passes_fit_quality(c, self.params.max_fit_rms_ratio))
            .cloned()
            .collect();
        counts.after_strength_filter = strong.len();

        // Stage 2: orientation clustering (optional).
        let mut grid_diagonals = None;
        let mut graph_diagonals = None;
        let mut orientation_histogram = None;
        let mut clustering_ran = false;
        if strong.len() >= self.params.min_corners && self.params.use_orientation_clustering {
            if let Some(clusters) =
                cluster_orientations(&strong, &self.params.orientation_clustering_params)
            {
                clustering_ran = true;
                orientation_histogram = clusters.histogram;
                grid_diagonals = Some(clusters.centers);
                graph_diagonals = grid_diagonals;
                strong = strong
                    .into_iter()
                    .zip(clusters.labels)
                    .filter_map(|(mut corner, label)| {
                        label.map(|cluster| {
                            corner.orientation_cluster = Some(cluster);
                            corner
                        })
                    })
                    .collect();
            }
        }
        if clustering_ran {
            counts.after_orientation_cluster_filter = Some(strong.len());
        }
        if grid_diagonals.is_none() && !strong.is_empty() {
            if let Some(theta) = estimate_grid_axes_from_orientations(&strong) {
                let c0 = wrap_angle_pi(theta);
                let c1 = wrap_angle_pi(theta + FRAC_PI_2);
                grid_diagonals = Some([c0, c1]);
            }
        }

        // Stage 3: compute per-corner local step (drives the two-axis
        // validator's step window + the overlay). Cheap to compute even
        // when the graph builder will discard the result, so we always
        // run it for the debug frame.
        let local_steps = if strong.is_empty() {
            Vec::new()
        } else {
            estimate_corner_local_steps(&strong)
        };

        // Stage 4: build the graph with rejection counters. Skipped when
        // not enough corners to form a board — still emit the debug frame.
        let mut rejection_counter = RejectionCounter::default();
        let mut graph = if strong.len() >= self.params.min_corners {
            build_chessboard_grid_graph_instrumented(
                &strong,
                &self.params.graph,
                graph_diagonals,
                Some(&mut rejection_counter),
            )
        } else {
            GridGraph {
                neighbors: (0..strong.len()).map(|_| Vec::new()).collect(),
            }
        };
        counts.graph_nodes = graph.neighbors.len();
        counts.graph_edges = graph.neighbors.iter().map(|n| n.len()).sum();
        counts.edges_by_reject_reason = ChessboardStageCounts::from_counter(&rejection_counter);

        // Phase 2: geometric-sanity graph cleanup (mirrors non-debug path).
        self.run_graph_cleanup(&mut graph, &strong, &mut counts);

        // Stage 5: components + BFS coord assignment via the normal path.
        let components = connected_components(&graph);
        counts.num_components = components.len();
        counts.largest_component_size = components.iter().map(|c| c.len()).max().unwrap_or(0);

        // Re-use the production component pipeline so the debug frame
        // matches what the non-debug entry points would produce.
        let results = self.collect_components(
            &graph,
            &components,
            &strong,
            grid_diagonals,
            orientation_histogram.clone(),
            &mut counts,
        );
        let result = results.into_iter().next();
        counts.final_labeled_corners = result
            .as_ref()
            .map(|r| r.detection.corners.len())
            .unwrap_or(0);

        // Build the flat debug frame.
        let strong_corners = build_debug_corners(&strong, &local_steps);
        let graph_neighbors = build_debug_edges(&graph);
        let metrics = match result.as_ref() {
            Some(r) => score_frame_full(
                &r.detection,
                self.params.expected_rows.unwrap_or(0),
                self.params.expected_cols.unwrap_or(0),
                &strong,
                &graph,
                image_width_px,
            ),
            None => {
                // No detection: still compute the partial metrics that do
                // not need labelled corners (graph degree histogram,
                // local-step CV, edge-axis residuals).
                let (med, p95) = edge_axis_residual_stats_raw(&strong, &graph);
                GridFrameMetrics {
                    graph_degree_hist: Some(degree_histogram(&graph)),
                    local_step_cv: local_step_cv_from_steps(&local_steps),
                    edge_axis_residual_median_deg: med,
                    edge_axis_residual_p95_deg: p95,
                    ..Default::default()
                }
            }
        };

        ChessboardDebugFrame {
            image_width: image_width_px,
            image_height: image_height_px,
            strong_corners,
            graph_neighbors,
            orientations: grid_diagonals,
            orientation_histogram,
            stage_counts: counts,
            metrics,
            result,
        }
    }

    /// Phase 2: Run the configured post-graph geometric-sanity cleanups.
    ///
    /// Drops directed edges that pass every per-edge validator in
    /// isolation but violate graph-global invariants of a planar grid:
    /// asymmetric Right/Left/Up/Down edges, bent chord pairs at a node,
    /// and crossing edges. Counts for each pass flow into `counts` so
    /// they appear in the instrumentation output.
    ///
    /// Only runs under [`ChessboardGraphMode::TwoAxis`]; under
    /// `Legacy` the cleanups can drop legitimate edges (the Simple /
    /// Cluster validators allow a broad distance window that makes
    /// Right/Left pairs legitimately non-antiparallel, and the
    /// per-direction scoring admits asymmetric choices). The caller
    /// flipped to `TwoAxis` opts into the stricter geometry.
    fn run_graph_cleanup(
        &self,
        graph: &mut GridGraph,
        strong: &[Corner],
        counts: &mut ChessboardStageCounts,
    ) {
        if !matches!(
            self.params.graph.mode,
            crate::params::ChessboardGraphMode::TwoAxis
        ) {
            return;
        }
        let cleanup = &self.params.graph_cleanup;
        let positions: Vec<nalgebra::Point2<f32>> = strong.iter().map(|c| c.position).collect();

        // Straightness first: a bent pair means one of the two edges is
        // spurious. Dropping it frees up the other edge to be part of a
        // real chain. Symmetry enforcement is run after straightness so
        // the dangling reverse of a dropped bent edge is cleaned up.
        if cleanup.enforce_straightness {
            let dropped =
                prune_by_edge_straightness(graph, &positions, cleanup.max_straightness_deg);
            counts.cleanup_bent_edges += dropped;
        }

        if cleanup.enforce_planarity {
            let dropped = prune_crossing_edges(graph, &positions);
            counts.cleanup_crossing_edges += dropped;
        }

        if cleanup.enforce_symmetry {
            let dropped = enforce_symmetry(graph);
            counts.cleanup_asymmetric_edges += dropped;
        }

        // Re-count after cleanup so downstream sees the truth.
        counts.graph_edges = graph.neighbors.iter().map(|n| n.len()).sum();
    }

    /// Shared logic: iterate components, convert to board coords, return all qualifying results.
    fn collect_components(
        &self,
        graph: &GridGraph,
        components: &[Vec<usize>],
        strong: &[Corner],
        grid_diagonals: Option<[f32; 2]>,
        orientation_histogram: Option<OrientationHistogram>,
        counts: &mut ChessboardStageCounts,
    ) -> Vec<ChessboardDetectionResult> {
        let mut results: Vec<(TargetDetection, Vec<usize>, usize)> = Vec::new();
        let mut found_primary = false;

        // Sort components by size descending so primary is processed first.
        let mut sorted_indices: Vec<usize> = (0..components.len()).collect();
        sorted_indices.sort_unstable_by(|&a, &b| components[b].len().cmp(&components[a].len()));

        let min_component = self
            .params
            .min_component_size
            .unwrap_or(self.params.min_corners);
        for &ci in &sorted_indices {
            let component = &components[ci];
            if component.len() < min_component {
                continue;
            }
            let coords = assign_grid_coordinates(graph, component);
            if coords.is_empty() {
                debug!(
                    "rejecting component with {} nodes because BFS assigned no grid coordinates",
                    component.len()
                );
                continue;
            }
            // Only record counts for the primary (largest) component; follow-ups
            // reuse the same pipeline stages but are diagnostic-only.
            let component_counts_sink: Option<&mut ChessboardStageCounts> =
                if !found_primary { Some(counts) } else { None };
            let skip_completeness = found_primary;
            let Some((detection, inliers)) = self.component_to_board_coords(
                &coords,
                strong,
                skip_completeness,
                component_counts_sink,
            ) else {
                continue;
            };
            let score = detection.corners.len();
            debug!(
                "accepted chessboard component with {} corners and {} inliers (primary={})",
                detection.corners.len(),
                inliers.len(),
                !found_primary
            );
            results.push((detection, inliers, score));
            found_primary = true;
        }

        // Sort by corner count descending.
        results.sort_unstable_by_key(|r| std::cmp::Reverse(r.2));

        let graph_debug = Some(build_graph_debug(graph, strong));
        results
            .into_iter()
            .map(|(detection, inliers, _)| ChessboardDetectionResult {
                detection,
                inliers,
                orientations: grid_diagonals,
                debug: ChessboardDebug {
                    orientation_histogram: orientation_histogram.clone(),
                    graph: graph_debug.clone(),
                },
            })
            .collect()
    }

    fn component_to_board_coords(
        &self,
        coords: &[(usize, GridIndex)],
        corners: &[Corner],
        skip_completeness: bool,
        counts: Option<&mut ChessboardStageCounts>,
    ) -> Option<(TargetDetection, Vec<usize>)> {
        let (min_i, max_i, min_j, max_j) =
            coords
                .iter()
                .fold((i32::MAX, i32::MIN, i32::MAX, i32::MIN), |acc, &(_, g)| {
                    (
                        acc.0.min(g.i),
                        acc.1.max(g.i),
                        acc.2.min(g.j),
                        acc.3.max(g.j),
                    )
                });

        if min_i == i32::MAX || min_j == i32::MAX {
            return None;
        }

        let width = (max_i - min_i + 1) as u32;
        let height = (max_j - min_j + 1) as u32;

        let Some((board_cols, board_rows, swap_axes)) =
            select_board_size(width, height, &self.params)
        else {
            debug!(
                "rejecting component with {} nodes: grid span {}x{} does not fit expected board cols={:?} rows={:?}",
                coords.len(),
                width,
                height,
                self.params.expected_cols,
                self.params.expected_rows
            );
            return None;
        };

        let grid_area = (board_cols * board_rows) as f32;
        if grid_area <= f32::EPSILON {
            debug!(
                "rejecting component with {} nodes: degenerate grid area for board {}x{}",
                coords.len(),
                board_cols,
                board_rows
            );
            return None;
        }

        // De-duplicate by grid coordinate: in noisy graphs, a component can contain
        // multiple corners that get mapped to the same (i,j). Keep the strongest one.
        let mut by_grid: std::collections::HashMap<GridCoords, LabeledCorner> =
            std::collections::HashMap::new();
        for &(node_idx, g) in coords {
            let corner = &corners[node_idx];
            let (gi, gj) = if swap_axes {
                (g.j - min_j, g.i - min_i)
            } else {
                (g.i - min_i, g.j - min_j)
            };
            let grid = GridCoords { i: gi, j: gj };
            let candidate = LabeledCorner {
                position: corner.position,
                grid: Some(grid),
                id: None,
                target_position: None,
                score: corner.strength,
            };

            match by_grid.get(&grid) {
                None => {
                    by_grid.insert(grid, candidate);
                }
                Some(prev) => {
                    if candidate.score > prev.score {
                        by_grid.insert(grid, candidate);
                    }
                }
            }
        }

        let completeness = by_grid.len() as f32 / grid_area;
        if !skip_completeness {
            if let (Some(_), Some(_)) = (self.params.expected_cols, self.params.expected_rows) {
                if completeness < self.params.completeness_threshold {
                    debug!(
                        "rejecting component with {} nodes: completeness {:.3} below threshold {:.3} for board {}x{} ({} unique corners)",
                        coords.len(),
                        completeness,
                        self.params.completeness_threshold,
                        board_cols,
                        board_rows,
                        by_grid.len()
                    );
                    return None;
                }
            }
        }

        let mut labeled: Vec<LabeledCorner> = by_grid.into_values().collect();
        let assigned_count = labeled.len();

        // Phase B: local-homography residual prune (distortion-tolerant).
        // Runs BEFORE the global prune so the global-prune residuals are
        // computed on a cleaner set. Both are individually gateable.
        let mut local_h_dropped = 0usize;
        if self.params.local_homography.enable {
            let before = labeled.len();
            labeled = prune_by_local_homography_residual(
                labeled,
                &self.params.local_homography,
                self.params.min_corners,
            );
            local_h_dropped = before.saturating_sub(labeled.len());
        }

        // Post-BFS homography-consistency pruning. The BFS coordinate
        // assignment propagates (i, j) through the neighbor graph; any
        // mislabelled corner (typically a false response sitting near but
        // not on the lattice) shows up as a large residual against the
        // best-fit homography from labels → pixels. Drop the top quantile
        // of residuals, refit, repeat. This is what collapses the "large
        // grid, high residual" failure mode into a smaller-but-clean grid
        // that actually passes the visible-subset gate.
        //
        // The global homography assumption breaks under non-trivial lens
        // distortion: true board corners end up with large residuals and
        // get dropped together with the mislabelled ones. Gated behind
        // `enable_global_homography_prune` so distorted captures can run
        // without it.
        if self.params.enable_global_homography_prune {
            labeled = prune_by_homography_residual(labeled, self.params.min_corners);
        }

        // Post-prune quality gate: reject detections whose p95 local-
        // homography residual is still above threshold. Catches the
        // "pruning bottomed out at min_corners but the residue is all
        // mis-shifted" failure mode.
        if let Some(max_p95) = self.params.max_local_homography_p95_px {
            if let Some(p95) = local_homography_residual_p95(&labeled) {
                if p95 > max_p95 {
                    debug!(
                        "rejecting detection: local-homography p95 residual {:.2} px exceeds gate {:.2} px ({} corners)",
                        p95,
                        max_p95,
                        labeled.len()
                    );
                    if let Some(counts) = counts {
                        counts.assigned_grid_corners = assigned_count;
                        counts.after_local_homography_prune = if self.params.local_homography.enable
                        {
                            Some(assigned_count.saturating_sub(local_h_dropped))
                        } else {
                            None
                        };
                        counts.after_global_homography_prune =
                            if self.params.enable_global_homography_prune {
                                Some(labeled.len())
                            } else {
                                None
                            };
                    }
                    return None;
                }
            }
        }

        // Phase 5: gap-fill. For each (i, j) in the bounding box with
        // no labelled corner but ≥min_neighbors labelled neighbors in a
        // window_half-cell window, predict the missing pixel position
        // via a local affine fit and attach the nearest unlabelled
        // strong corner within search_rel × local_step.
        let gap_filled = if self.params.gap_fill.enable {
            let before = labeled.len();
            labeled = fill_gaps_via_local_affine(labeled, corners, &self.params.gap_fill);
            labeled.len().saturating_sub(before)
        } else {
            0
        };

        if let Some(counts) = counts {
            counts.assigned_grid_corners = assigned_count;
            counts.after_local_homography_prune = if self.params.local_homography.enable {
                Some(assigned_count.saturating_sub(local_h_dropped))
            } else {
                None
            };
            counts.after_global_homography_prune = if self.params.enable_global_homography_prune {
                Some(labeled.len())
            } else {
                None
            };
            counts.gap_filled_corners = gap_filled;
        }

        labeled.sort_by(|a, b| {
            let ga = a.grid.as_ref().unwrap();
            let gb = b.grid.as_ref().unwrap();
            (ga.j, ga.i).cmp(&(gb.j, gb.i))
        });

        let detection = TargetDetection {
            kind: TargetKind::Chessboard,
            corners: labeled,
        };

        let inliers = (0..detection.corners.len()).collect();
        debug!(
            "component with {} nodes produced board {}x{} (swap_axes={swap_axes}) with {} unique corners and completeness {:.3}",
            coords.len(),
            board_cols,
            board_rows,
            detection.corners.len(),
            completeness
        );

        Some((detection, inliers))
    }
}

/// Remove labelled corners whose pixel position disagrees with the best-fit
/// ideal-grid homography. Two-tier:
///
/// 1. **Hard tier.** While the per-frame residual median exceeds
///    `HARD_CUTOFF_PX`, drop the top `HARD_TIER_DROP_FRAC` of residuals and
///    refit. This attacks the failure mode where BFS propagation produced a
///    globally-skewed grid — MAD-based pruning cannot fix that by itself
///    because every corner has a large residual and the MAD stays small.
/// 2. **MAD tier.** Once the median is under the hard cutoff, switch to a
///    MAD-based outlier reject (`OUTLIER_MAD_FACTOR × MAD`) with a small
///    floor so tight inlier sets are preserved.
///
/// Both tiers stop if removing more corners would drop below `min_keep`.
fn prune_by_homography_residual(
    mut labeled: Vec<LabeledCorner>,
    min_keep: usize,
) -> Vec<LabeledCorner> {
    const MAX_HARD_ITERS: u32 = 10;
    const MAX_MAD_ITERS: u32 = 5;
    // Pruning targets intentionally match the visible-subset gate; we do not
    // want to over-prune already-clean frames whose corner count drops below
    // `min_keep` just to chase sub-gate residuals.
    const HARD_CUTOFF_PX: f32 = 0.5;
    const HARD_TIER_DROP_FRAC: f32 = 0.05;
    const OUTLIER_MAD_FACTOR: f32 = 3.0;
    const HARD_FLOOR_PX: f32 = 0.5;
    const P95_TARGET_PX: f32 = 1.0;

    if labeled.len() < 4 || labeled.len() <= min_keep {
        return labeled;
    }

    // Hard tier — strip off high-residual corners until BOTH median and p95
    // collapse below the gate targets. Stops early when dropping more would
    // fall below `min_keep`, to avoid over-pruning clean frames.
    for _ in 0..MAX_HARD_ITERS {
        let Some((residuals, median)) = compute_residuals(&labeled) else {
            break;
        };
        let mut sorted = residuals.clone();
        sorted.sort_by(|a, b| a.total_cmp(b));
        let p95_idx = ((sorted.len() as f32 - 1.0) * 0.95).round() as usize;
        let p95 = sorted[p95_idx.min(sorted.len() - 1)];
        if median <= HARD_CUTOFF_PX && p95 <= P95_TARGET_PX {
            break;
        }

        let mut indices: Vec<usize> = (0..residuals.len()).collect();
        indices.sort_by(|&a, &b| residuals[b].total_cmp(&residuals[a]));

        let drop_count = ((labeled.len() as f32) * HARD_TIER_DROP_FRAC).ceil() as usize;
        let drop_count = drop_count.max(1);
        let after_drop = labeled.len().saturating_sub(drop_count);
        if after_drop < min_keep {
            break;
        }

        let drop_set: std::collections::HashSet<usize> =
            indices.into_iter().take(drop_count).collect();
        labeled = labeled
            .into_iter()
            .enumerate()
            .filter_map(|(idx, lc)| {
                if drop_set.contains(&idx) {
                    None
                } else {
                    Some(lc)
                }
            })
            .collect();
    }

    // MAD tier — final refinement once the median is already well-behaved.
    for _ in 0..MAX_MAD_ITERS {
        let Some((residuals, median)) = compute_residuals(&labeled) else {
            break;
        };
        let abs_dev: Vec<f32> = residuals.iter().map(|r| (r - median).abs()).collect();
        let mut mad_sorted = abs_dev.clone();
        mad_sorted.sort_by(|a, b| a.total_cmp(b));
        let mad = mad_sorted[mad_sorted.len() / 2].max(1e-3);
        let threshold = (median + OUTLIER_MAD_FACTOR * mad * 1.4826).max(HARD_FLOOR_PX);

        let mut keep_mask = vec![true; labeled.len()];
        let mut pruned = 0usize;
        for (idx, r) in residuals.iter().enumerate() {
            if *r > threshold {
                keep_mask[idx] = false;
                pruned += 1;
            }
        }
        if pruned == 0 || labeled.len() - pruned < min_keep {
            break;
        }
        let mut kept = Vec::with_capacity(labeled.len() - pruned);
        for (idx, keep) in keep_mask.into_iter().enumerate() {
            if keep {
                kept.push(labeled[idx].clone());
            }
        }
        labeled = kept;
    }

    labeled
}

/// Local-homography residual prune.
///
/// For each labelled corner at `(i, j)`, collect **other** labelled
/// corners inside a `window_half`-cell grid window (`|di|,|dj| <= window_half`),
/// fit a homography from those neighbors' `(i, j) → (x, y)` pairs, predict
/// the current corner via that homography, and drop corners whose observed
/// position disagrees by more than
/// `max(threshold_rel × local_step, threshold_px_floor)` pixels.
///
/// Iterates up to `max_iters` times (refit after each pass) or until no
/// further corners are dropped. Stops early if removing more would drop
/// below `min_keep`.
///
/// The local step per corner is computed from the currently-labelled
/// neighbors' median pixel distance — we cannot reuse the validator's
/// per-corner step because it is indexed by raw-corner index, not grid
/// index.
fn prune_by_local_homography_residual(
    mut labeled: Vec<LabeledCorner>,
    params: &LocalHomographyPruneParams,
    min_keep: usize,
) -> Vec<LabeledCorner> {
    if labeled.len() < min_keep || params.min_neighbors < 4 {
        return labeled;
    }
    let threshold_px_floor = params.threshold_px_floor.max(0.0);
    let threshold_rel = params.threshold_rel.max(0.0);

    // Iterative one-at-a-time pruning: one corner per iteration is the
    // worst current outlier; its removal refits every subsequent window,
    // which prevents the single label error from "contaminating" neighbor
    // predictions and cascading into false positives.
    for _ in 0..params.max_iters {
        if labeled.len() <= min_keep {
            break;
        }

        // Build (grid, idx) index map.
        let mut idx_by_grid: std::collections::HashMap<(i32, i32), usize> =
            std::collections::HashMap::with_capacity(labeled.len());
        for (idx, lc) in labeled.iter().enumerate() {
            if let Some(g) = lc.grid {
                idx_by_grid.insert((g.i, g.j), idx);
            }
        }

        let mut worst: Option<(usize, f32)> = None;
        for (idx, lc) in labeled.iter().enumerate() {
            let Some(g) = lc.grid else { continue };

            // Collect labelled neighbors inside the grid window, excluding self.
            let mut grid_pts: Vec<Point2<f32>> = Vec::new();
            let mut img_pts: Vec<Point2<f32>> = Vec::new();
            let mut nbr_distances: Vec<f32> = Vec::new();
            for di in -params.window_half..=params.window_half {
                for dj in -params.window_half..=params.window_half {
                    if di == 0 && dj == 0 {
                        continue;
                    }
                    let key = (g.i + di, g.j + dj);
                    if let Some(&nidx) = idx_by_grid.get(&key) {
                        let nlc = &labeled[nidx];
                        grid_pts.push(Point2::new(key.0 as f32, key.1 as f32));
                        img_pts.push(nlc.position);
                        if di.abs() <= 1 && dj.abs() <= 1 {
                            let dx = nlc.position.x - lc.position.x;
                            let dy = nlc.position.y - lc.position.y;
                            nbr_distances.push((dx * dx + dy * dy).sqrt());
                        }
                    }
                }
            }

            if grid_pts.len() < params.min_neighbors {
                continue;
            }
            let Some(h) = estimate_homography_rect_to_img(&grid_pts, &img_pts) else {
                continue;
            };
            let pred = h.apply(Point2::new(g.i as f32, g.j as f32));
            let dx = pred.x - lc.position.x;
            let dy = pred.y - lc.position.y;
            let residual = (dx * dx + dy * dy).sqrt();

            // Per-corner local step: median of adjacent-neighbor distances.
            let step_est = if nbr_distances.is_empty() {
                None
            } else {
                nbr_distances.sort_by(|a, b| a.total_cmp(b));
                Some(nbr_distances[nbr_distances.len() / 2])
            };
            let threshold = match step_est {
                Some(s) => (threshold_rel * s).max(threshold_px_floor),
                None => threshold_px_floor,
            };
            if residual > threshold {
                let margin = residual / threshold;
                if worst.map(|w| margin > w.1).unwrap_or(true) {
                    worst = Some((idx, margin));
                }
            }
        }

        match worst {
            Some((idx, _)) => {
                labeled.remove(idx);
            }
            None => break,
        }
    }

    labeled
}

/// p95 of the local-homography residual over the labelled corners,
/// mirroring the metric from [`crate::quality::score_frame_full`]. Shares
/// the structural invariants of the prune above (2-cell window, ≥5
/// neighbors, DLT fit). Used by the post-prune quality gate.
fn local_homography_residual_p95(labeled: &[LabeledCorner]) -> Option<f32> {
    if labeled.len() < 6 {
        return None;
    }
    let mut idx_by_grid: std::collections::HashMap<(i32, i32), usize> =
        std::collections::HashMap::with_capacity(labeled.len());
    for (i, c) in labeled.iter().enumerate() {
        if let Some(g) = c.grid {
            idx_by_grid.insert((g.i, g.j), i);
        }
    }
    let mut residuals: Vec<f32> = Vec::new();
    for (idx, c) in labeled.iter().enumerate() {
        let Some(g) = c.grid else { continue };
        let mut grid_pts: Vec<Point2<f32>> = Vec::new();
        let mut img_pts: Vec<Point2<f32>> = Vec::new();
        for di in -2i32..=2 {
            for dj in -2i32..=2 {
                if di == 0 && dj == 0 {
                    continue;
                }
                let key = (g.i + di, g.j + dj);
                if let Some(&nidx) = idx_by_grid.get(&key) {
                    if nidx == idx {
                        continue;
                    }
                    grid_pts.push(Point2::new(key.0 as f32, key.1 as f32));
                    img_pts.push(labeled[nidx].position);
                }
            }
        }
        if grid_pts.len() < 5 {
            continue;
        }
        let Some(h) = estimate_homography_rect_to_img(&grid_pts, &img_pts) else {
            continue;
        };
        let pred = h.apply(Point2::new(g.i as f32, g.j as f32));
        let dx = pred.x - c.position.x;
        let dy = pred.y - c.position.y;
        residuals.push((dx * dx + dy * dy).sqrt());
    }
    if residuals.len() < 2 {
        return None;
    }
    residuals.sort_by(|a, b| a.total_cmp(b));
    let idx = ((residuals.len() as f32 - 1.0) * 0.95).round() as usize;
    Some(residuals[idx.min(residuals.len() - 1)])
}

fn compute_residuals(labeled: &[LabeledCorner]) -> Option<(Vec<f32>, f32)> {
    if labeled.len() < 4 {
        return None;
    }
    let grid_pts: Vec<Point2<f32>> = labeled
        .iter()
        .map(|lc| {
            let g = lc.grid.unwrap();
            Point2::new(g.i as f32, g.j as f32)
        })
        .collect();
    let image_pts: Vec<Point2<f32>> = labeled.iter().map(|lc| lc.position).collect();

    let h = estimate_homography_rect_to_img(&grid_pts, &image_pts)?;
    let residuals: Vec<f32> = grid_pts
        .iter()
        .zip(image_pts.iter())
        .map(|(g, p)| {
            let pred = h.apply(*g);
            ((pred.x - p.x).powi(2) + (pred.y - p.y).powi(2)).sqrt()
        })
        .collect();
    let mut sorted = residuals.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let median = sorted[sorted.len() / 2];
    Some((residuals, median))
}

/// Phase 5: Recover missing `(i, j)` positions whose labelled neighbors
/// permit a local affine prediction.
///
/// Iterates over integer `(i, j)` inside the bounding box of the
/// labelled set. For each position not yet labelled that has
/// `≥ min_neighbors` labelled neighbors inside a
/// `window_half`-cell window, fits an affine map `[x, y] = A·[i, j] + b`
/// by least-squares, predicts the missing pixel position, and attaches
/// the nearest unlabelled strong corner within
/// `search_rel × median_local_step`.
///
/// The pass iterates up to `params.max_iters` times because newly-
/// attached corners can unlock further predictions. Stops early when a
/// full pass attaches nothing.
fn fill_gaps_via_local_affine(
    mut labeled: Vec<LabeledCorner>,
    strong: &[Corner],
    params: &crate::params::GapFillParams,
) -> Vec<LabeledCorner> {
    use std::collections::HashSet;

    if labeled.len() < params.min_neighbors {
        return labeled;
    }

    // Keyed strong-corner set: track which corners are already labelled by
    // exact position — labelled positions are copied from `strong` without
    // modification, so equality holds.
    let pos_key = |p: nalgebra::Point2<f32>| -> (u32, u32) { (p.x.to_bits(), p.y.to_bits()) };

    for _iter in 0..params.max_iters.max(1) {
        let mut used: HashSet<(u32, u32)> = HashSet::new();
        for lc in &labeled {
            used.insert(pos_key(lc.position));
        }

        // Build (i, j) -> pixel lookup.
        let mut by_ij: std::collections::HashMap<GridCoords, Point2<f32>> =
            std::collections::HashMap::new();
        for lc in &labeled {
            if let Some(g) = lc.grid {
                by_ij.insert(g, lc.position);
            }
        }

        let (min_i, max_i, min_j, max_j) = by_ij.keys().fold(
            (i32::MAX, i32::MIN, i32::MAX, i32::MIN),
            |(ai0, ai1, aj0, aj1), g| (ai0.min(g.i), ai1.max(g.i), aj0.min(g.j), aj1.max(g.j)),
        );
        if min_i == i32::MAX {
            return labeled;
        }

        let mut attached: Vec<LabeledCorner> = Vec::new();

        for j in min_j..=max_j {
            for i in min_i..=max_i {
                let target = GridCoords { i, j };
                if by_ij.contains_key(&target) {
                    continue;
                }

                // Collect labelled neighbors in the window.
                let wh = params.window_half;
                let mut ns: Vec<(GridCoords, Point2<f32>)> = Vec::new();
                for dj in -wh..=wh {
                    for di in -wh..=wh {
                        if di == 0 && dj == 0 {
                            continue;
                        }
                        let nij = GridCoords {
                            i: i + di,
                            j: j + dj,
                        };
                        if let Some(&p) = by_ij.get(&nij) {
                            ns.push((nij, p));
                        }
                    }
                }
                if ns.len() < params.min_neighbors {
                    continue;
                }

                // Fit affine [x, y] = A·[i, j] + b  by least squares:
                // Solve the normal equations for 3 unknowns per axis.
                let Some((ax, bx, cx, ay, by_, cy)) = fit_affine_ij_to_xy(&ns) else {
                    continue;
                };
                let ii = i as f32;
                let jj = j as f32;
                let pred = Point2::new(ax * ii + bx * jj + cx, ay * ii + by_ * jj + cy);

                // Median labelled-neighbor chord as local cell size.
                let mut chords: Vec<f32> = Vec::new();
                for k in 0..ns.len() {
                    for m in (k + 1)..ns.len() {
                        let dij_i = ns[k].0.i - ns[m].0.i;
                        let dij_j = ns[k].0.j - ns[m].0.j;
                        let gd = ((dij_i * dij_i + dij_j * dij_j) as f32).sqrt();
                        if gd < 0.5 {
                            continue;
                        }
                        let pd = (ns[k].1 - ns[m].1).norm();
                        chords.push(pd / gd);
                    }
                }
                if chords.is_empty() {
                    continue;
                }
                chords.sort_by(|a, b| a.total_cmp(b));
                let local_step = chords[chords.len() / 2];
                let search_px = params.search_rel * local_step;

                // Find nearest unlabelled strong corner within the search
                // radius.
                let mut best: Option<(usize, f32)> = None;
                for (ci, c) in strong.iter().enumerate() {
                    if used.contains(&pos_key(c.position)) {
                        continue;
                    }
                    let d = (c.position - pred).norm();
                    if d <= search_px && best.map(|b| d < b.1).unwrap_or(true) {
                        best = Some((ci, d));
                    }
                }
                let Some((ci, _d)) = best else { continue };

                let corner = &strong[ci];
                let new_lc = LabeledCorner {
                    position: corner.position,
                    grid: Some(target),
                    id: None,
                    target_position: None,
                    score: corner.strength,
                };
                used.insert(pos_key(corner.position));
                attached.push(new_lc);
            }
        }

        if attached.is_empty() {
            return labeled;
        }
        labeled.extend(attached);
    }

    labeled
}

/// Least-squares fit of an affine map `[x, y] = A·[i, j] + b`.
/// Returns `(A[0,0], A[0,1], b[0], A[1,0], A[1,1], b[1])` or `None`
/// when the 3×3 normal matrix is singular.
fn fit_affine_ij_to_xy(
    samples: &[(GridCoords, Point2<f32>)],
) -> Option<(f32, f32, f32, f32, f32, f32)> {
    // Design matrix rows [i, j, 1]. Normal matrix M = X^T X (3×3).
    let mut m = [[0.0f64; 3]; 3];
    let mut rhs_x = [0.0f64; 3];
    let mut rhs_y = [0.0f64; 3];
    for (g, p) in samples {
        let i = g.i as f64;
        let j = g.j as f64;
        let row = [i, j, 1.0];
        for a in 0..3 {
            for b in 0..3 {
                m[a][b] += row[a] * row[b];
            }
            rhs_x[a] += row[a] * p.x as f64;
            rhs_y[a] += row[a] * p.y as f64;
        }
    }

    let inv = invert_3x3(&m)?;
    let bx = [
        inv[0][0] * rhs_x[0] + inv[0][1] * rhs_x[1] + inv[0][2] * rhs_x[2],
        inv[1][0] * rhs_x[0] + inv[1][1] * rhs_x[1] + inv[1][2] * rhs_x[2],
        inv[2][0] * rhs_x[0] + inv[2][1] * rhs_x[1] + inv[2][2] * rhs_x[2],
    ];
    let by_ = [
        inv[0][0] * rhs_y[0] + inv[0][1] * rhs_y[1] + inv[0][2] * rhs_y[2],
        inv[1][0] * rhs_y[0] + inv[1][1] * rhs_y[1] + inv[1][2] * rhs_y[2],
        inv[2][0] * rhs_y[0] + inv[2][1] * rhs_y[1] + inv[2][2] * rhs_y[2],
    ];

    Some((
        bx[0] as f32,
        bx[1] as f32,
        bx[2] as f32,
        by_[0] as f32,
        by_[1] as f32,
        by_[2] as f32,
    ))
}

/// Invert a 3×3 matrix. Returns `None` on near-singular input.
fn invert_3x3(m: &[[f64; 3]; 3]) -> Option<[[f64; 3]; 3]> {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv_det = 1.0 / det;
    let mut inv = [[0.0f64; 3]; 3];
    inv[0][0] = (m[1][1] * m[2][2] - m[1][2] * m[2][1]) * inv_det;
    inv[0][1] = (m[0][2] * m[2][1] - m[0][1] * m[2][2]) * inv_det;
    inv[0][2] = (m[0][1] * m[1][2] - m[0][2] * m[1][1]) * inv_det;
    inv[1][0] = (m[1][2] * m[2][0] - m[1][0] * m[2][2]) * inv_det;
    inv[1][1] = (m[0][0] * m[2][2] - m[0][2] * m[2][0]) * inv_det;
    inv[1][2] = (m[0][2] * m[1][0] - m[0][0] * m[1][2]) * inv_det;
    inv[2][0] = (m[1][0] * m[2][1] - m[1][1] * m[2][0]) * inv_det;
    inv[2][1] = (m[0][1] * m[2][0] - m[0][0] * m[2][1]) * inv_det;
    inv[2][2] = (m[0][0] * m[1][1] - m[0][1] * m[1][0]) * inv_det;
    Some(inv)
}

fn select_board_size(
    width: u32,
    height: u32,
    params: &ChessboardParams,
) -> Option<(u32, u32, bool)> {
    match (params.expected_cols, params.expected_rows) {
        (Some(expected_cols), Some(expected_rows)) => {
            let fits_direct = width <= expected_cols && height <= expected_rows;
            let fits_swapped = width <= expected_rows && height <= expected_cols;

            if !fits_direct && !fits_swapped {
                return None;
            }

            let swap_axes = if fits_direct && !fits_swapped {
                false
            } else if !fits_direct && fits_swapped {
                true
            } else {
                let gap_direct = (expected_cols - width) + (expected_rows - height);
                let gap_swapped = (expected_rows - width) + (expected_cols - height);
                gap_swapped < gap_direct
            };

            Some((expected_cols, expected_rows, swap_axes))
        }
        _ => Some((width, height, false)),
    }
}

fn build_graph_debug(graph: &GridGraph, corners: &[Corner]) -> GridGraphDebug {
    let nodes = graph
        .neighbors
        .iter()
        .enumerate()
        .map(|(idx, neighs)| {
            let neighbors = neighs
                .iter()
                .map(|n| GridGraphNeighborDebug {
                    index: n.index,
                    direction: neighbor_dir_name(n.direction),
                    distance: n.distance,
                })
                .collect();
            GridGraphNodeDebug {
                position: [corners[idx].position.x, corners[idx].position.y],
                neighbors,
            }
        })
        .collect();

    GridGraphDebug { nodes }
}

fn log_graph_summary(graph: &GridGraph, components: &[Vec<usize>], min_corners: usize) {
    let mut component_sizes: Vec<usize> =
        components.iter().map(|component| component.len()).collect();
    component_sizes.sort_unstable_by(|a, b| b.cmp(a));

    let degrees: Vec<usize> = graph
        .neighbors
        .iter()
        .map(|neighbors| neighbors.len())
        .collect();
    let isolated_nodes = degrees.iter().filter(|&&degree| degree == 0).count();
    let nodes_with_neighbors = degrees.len().saturating_sub(isolated_nodes);
    let directed_edges: usize = degrees.iter().sum();
    let min_degree = degrees.iter().copied().min().unwrap_or(0);
    let max_degree = degrees.iter().copied().max().unwrap_or(0);
    let avg_degree = if degrees.is_empty() {
        0.0
    } else {
        directed_edges as f32 / degrees.len() as f32
    };
    let candidate_components = component_sizes
        .iter()
        .filter(|&&size| size >= min_corners)
        .count();
    let top_n = component_sizes.len().min(8);

    debug!(
        "grid graph summary: nodes={}, nodes_with_neighbors={}, isolated_nodes={}, directed_edges={}, degree[min/avg/max]={}/{:.2}/{}, components={}, candidate_components={}, largest_components={:?}",
        degrees.len(),
        nodes_with_neighbors,
        isolated_nodes,
        directed_edges,
        min_degree,
        avg_degree,
        max_degree,
        component_sizes.len(),
        candidate_components,
        &component_sizes[..top_n]
    );
}

fn neighbor_dir_name(dir: NeighborDirection) -> &'static str {
    match dir {
        NeighborDirection::Right => "right",
        NeighborDirection::Left => "left",
        NeighborDirection::Up => "up",
        NeighborDirection::Down => "down",
        _ => "unknown",
    }
}

fn wrap_angle_pi(theta: f32) -> f32 {
    let mut t = theta % std::f32::consts::PI;
    if t < 0.0 {
        t += std::f32::consts::PI;
    }
    t
}

fn build_debug_corners(
    strong: &[Corner],
    local_steps: &[projective_grid::local_step::LocalStep<f32>],
) -> Vec<DebugCorner> {
    strong
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let step = local_steps.get(i).copied().unwrap_or_default();
            // `orientation` is a debug-only convenience derived from
            // `axes[0].angle`, folded into [0, π). No π/4 shift: the new
            // convention is that `axes` are the grid axes directly.
            let orientation = wrap_angle_pi(c.axes[0].angle);
            DebugCorner {
                x: c.position.x,
                y: c.position.y,
                axes: c.axes,
                orientation,
                orientation_cluster: c.orientation_cluster,
                strength: c.strength,
                contrast: c.contrast,
                fit_rms: c.fit_rms,
                local_step_u: step.step_u,
                local_step_v: step.step_v,
                local_step_confidence: step.confidence,
            }
        })
        .collect()
}

fn build_debug_edges(graph: &GridGraph) -> Vec<Vec<DebugGraphEdge>> {
    graph
        .neighbors
        .iter()
        .map(|neighbors| {
            neighbors
                .iter()
                .map(|n| DebugGraphEdge {
                    dst: n.index,
                    direction: neighbor_dir_name(n.direction),
                    distance: n.distance,
                    score: n.score,
                })
                .collect()
        })
        .collect()
}

fn degree_histogram(graph: &GridGraph) -> [u32; 5] {
    let mut hist = [0u32; 5];
    for neighbors in &graph.neighbors {
        let bucket = neighbors.len().min(4);
        hist[bucket] = hist[bucket].saturating_add(1);
    }
    hist
}

fn local_step_cv_from_steps(steps: &[projective_grid::local_step::LocalStep<f32>]) -> Option<f32> {
    let means: Vec<f32> = steps
        .iter()
        .filter(|s| s.confidence > 0.0 && s.step_u > 0.0 && s.step_v > 0.0)
        .map(|s| 0.5 * (s.step_u + s.step_v))
        .collect();
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

fn edge_axis_residual_stats_raw(
    corners: &[Corner],
    graph: &GridGraph,
) -> (Option<f32>, Option<f32>) {
    let mut residuals: Vec<f32> = Vec::new();
    for (src_idx, neighbors) in graph.neighbors.iter().enumerate() {
        let src = &corners[src_idx];
        for n in neighbors {
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
    let percentile = |q: f32| {
        let idx = ((residuals.len() as f32 - 1.0) * q).round() as usize;
        residuals[idx.min(residuals.len() - 1)]
    };
    (Some(percentile(0.5)), Some(percentile(0.95)))
}

fn nearest_axis_line_diff(axes: &[AxisEstimate; 2], edge_angle: f32) -> f32 {
    use std::f32::consts::PI;
    let two_pi = 2.0 * PI;
    let mut best = f32::INFINITY;
    for axis in axes {
        let mut diff = (edge_angle - axis.angle).rem_euclid(two_pi);
        if diff >= PI {
            diff -= two_pi;
        }
        let diff_abs = diff.abs();
        let folded = diff_abs.min(PI - diff_abs);
        if folded < best {
            best = folded;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use calib_targets_core::Corner;

    fn make_corner(x: f32, y: f32, axis0: f32, strength: f32) -> Corner {
        Corner {
            position: nalgebra::Point2::new(x, y),
            axes: [
                AxisEstimate {
                    angle: axis0,
                    sigma: 0.05,
                },
                AxisEstimate {
                    angle: axis0 + std::f32::consts::FRAC_PI_2,
                    sigma: 0.05,
                },
            ],
            strength,
            ..Corner::default()
        }
    }

    #[test]
    fn stage_counts_populated_on_early_exit() {
        // min_corners defaults to 16, so 4 corners cannot form a board.
        // Verify the instrumented entry point still populates raw/filter counts.
        let corners = vec![
            make_corner(0.0, 0.0, std::f32::consts::FRAC_PI_4, 1.0),
            make_corner(10.0, 0.0, 3.0 * std::f32::consts::FRAC_PI_4, 1.0),
            make_corner(0.0, 10.0, 3.0 * std::f32::consts::FRAC_PI_4, 1.0),
            make_corner(10.0, 10.0, std::f32::consts::FRAC_PI_4, 1.0),
        ];

        let detector = ChessboardDetector::new(ChessboardParams::default());
        let instrumented = detector.detect_instrumented(&corners);

        assert!(instrumented.result.is_none());
        assert_eq!(instrumented.counts.raw_corners, 4);
        assert_eq!(instrumented.counts.after_strength_filter, 4);
        assert_eq!(instrumented.counts.graph_nodes, 0);
        assert_eq!(instrumented.counts.final_labeled_corners, 0);
        assert!(instrumented.counts.after_global_homography_prune.is_none());
    }

    #[test]
    fn local_homography_prune_drops_injected_mislabel() {
        use calib_targets_core::{GridCoords, LabeledCorner};

        // Build a clean 5×5 lattice at 20-px spacing plus a single corner
        // whose GRID label puts it at (0,0) but whose PIXEL position sits
        // 8 px away from the neighbor-predicted position. The local-H
        // predictor should flag that corner; the clean 5×5 stays intact.
        let spacing = 20.0f32;
        let mut labeled = Vec::new();
        for j in 0..5 {
            for i in 0..5 {
                labeled.push(LabeledCorner {
                    position: nalgebra::Point2::new(
                        i as f32 * spacing + 100.0,
                        j as f32 * spacing + 100.0,
                    ),
                    grid: Some(GridCoords { i, j }),
                    id: None,
                    target_position: None,
                    score: 1.0,
                });
            }
        }

        // Displace corner at (2,2) by (8px, 8px) so local-H prediction
        // is far from its observation. Local residual: ~8 * sqrt(2) ≈ 11.3 px.
        let center_idx = 2 * 5 + 2;
        labeled[center_idx].position =
            nalgebra::Point2::new(100.0 + 2.0 * spacing + 8.0, 100.0 + 2.0 * spacing + 8.0);

        let params = LocalHomographyPruneParams {
            enable: true,
            window_half: 2,
            min_neighbors: 5,
            threshold_rel: 0.15,
            threshold_px_floor: 1.5,
            max_iters: 4,
        };

        let pruned = prune_by_local_homography_residual(labeled.clone(), &params, 4);
        // The mislabelled corner must be removed; the rest stay.
        assert_eq!(pruned.len(), labeled.len() - 1);
        let kept_grid: std::collections::HashSet<(i32, i32)> = pruned
            .iter()
            .filter_map(|c| c.grid.map(|g| (g.i, g.j)))
            .collect();
        assert!(!kept_grid.contains(&(2, 2)));
    }

    #[test]
    fn local_homography_prune_preserves_clean_grid() {
        use calib_targets_core::{GridCoords, LabeledCorner};

        // A perfect 5×5 grid — nothing should be dropped.
        let spacing = 20.0f32;
        let mut labeled = Vec::new();
        for j in 0..5 {
            for i in 0..5 {
                labeled.push(LabeledCorner {
                    position: nalgebra::Point2::new(
                        i as f32 * spacing + 50.0,
                        j as f32 * spacing + 50.0,
                    ),
                    grid: Some(GridCoords { i, j }),
                    id: None,
                    target_position: None,
                    score: 1.0,
                });
            }
        }

        let params = LocalHomographyPruneParams {
            enable: true,
            window_half: 2,
            min_neighbors: 5,
            threshold_rel: 0.15,
            threshold_px_floor: 1.5,
            max_iters: 4,
        };

        let pruned = prune_by_local_homography_residual(labeled.clone(), &params, 4);
        assert_eq!(pruned.len(), labeled.len());
    }

    #[test]
    fn stage_counts_graph_shape_visible_under_strength_filter() {
        // All below strength threshold => strength filter drops everything.
        let corners: Vec<Corner> = (0..20)
            .map(|i| {
                make_corner(
                    i as f32 * 10.0,
                    0.0,
                    std::f32::consts::FRAC_PI_4,
                    0.1, // weak
                )
            })
            .collect();

        let params = ChessboardParams {
            min_corner_strength: 0.5, // above everything
            ..Default::default()
        };
        let detector = ChessboardDetector::new(params);
        let out = detector.detect_instrumented(&corners);

        assert!(out.result.is_none());
        assert_eq!(out.counts.raw_corners, 20);
        assert_eq!(out.counts.after_strength_filter, 0);
        assert_eq!(out.counts.graph_nodes, 0);
        assert_eq!(out.counts.num_components, 0);
    }
}
