use crate::gridgraph::{
    assign_grid_coordinates, build_chessboard_grid_graph_instrumented, connected_components,
    RejectionCounter,
};
use crate::params::ChessboardParams;
use calib_targets_core::{
    cluster_orientations, estimate_grid_axes_from_orientations, estimate_homography_rect_to_img,
    Corner, GridCoords, LabeledCorner, OrientationHistogram, TargetDetection, TargetKind,
};
use log::{debug, warn};
use nalgebra::Point2;
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
        let graph = build_chessboard_grid_graph_instrumented(
            &strong,
            &self.params.graph,
            graph_diagonals,
            Some(&mut rejection_counter),
        );

        counts.graph_nodes = graph.neighbors.len();
        counts.graph_edges = graph.neighbors.iter().map(|n| n.len()).sum();
        counts.edges_by_reject_reason = ChessboardStageCounts::from_counter(&rejection_counter);

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

        for &ci in &sorted_indices {
            let component = &components[ci];
            if component.len() < self.params.min_corners {
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

        // Post-BFS homography-consistency pruning. The BFS coordinate
        // assignment propagates (i, j) through the neighbor graph; any
        // mislabelled corner (typically a false response sitting near but
        // not on the lattice) shows up as a large residual against the
        // best-fit homography from labels → pixels. Drop the top quantile
        // of residuals, refit, repeat. This is what collapses the "large
        // grid, high residual" failure mode into a smaller-but-clean grid
        // that actually passes the visible-subset gate.
        labeled = prune_by_homography_residual(labeled, self.params.min_corners);

        if let Some(counts) = counts {
            counts.assigned_grid_corners = assigned_count;
            counts.after_global_homography_prune = Some(labeled.len());
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

#[cfg(test)]
mod tests {
    use super::*;
    use calib_targets_core::Corner;

    fn make_corner(x: f32, y: f32, orientation: f32, strength: f32) -> Corner {
        Corner {
            position: nalgebra::Point2::new(x, y),
            orientation,
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
