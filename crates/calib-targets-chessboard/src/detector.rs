use crate::gridgraph::{
    assign_grid_coordinates, build_chessboard_grid_graph, connected_components,
};
use crate::params::ChessboardParams;
use calib_targets_core::{
    cluster_orientations, estimate_grid_axes_from_orientations, Corner, GridCoords, LabeledCorner,
    OrientationHistogram, TargetDetection, TargetKind,
};
use log::{debug, warn};
use projective_grid::{GridGraph, GridIndex, NeighborDirection};
use serde::Serialize;
use std::f32::consts::FRAC_PI_2;

#[cfg(feature = "tracing")]
use tracing::instrument;

/// Simple chessboard detector using ChESS orientations + grid fitting in (u, v) space.
#[derive(Debug)]
pub struct ChessboardDetector {
    pub params: ChessboardParams,
}

#[derive(Debug, Serialize)]
pub struct ChessboardDetectionResult {
    pub detection: TargetDetection,
    pub inliers: Vec<usize>,
    pub orientations: Option<[f32; 2]>,
    pub debug: ChessboardDebug,
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
        // 1. Filter by strength.
        let mut strong: Vec<Corner> = corners
            .iter()
            .filter(|c| c.strength >= self.params.min_corner_strength)
            .cloned()
            .collect();

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
            return None;
        }

        // 2. Estimate grid axes from orientations.
        let mut grid_diagonals = None;
        let mut graph_diagonals = None;
        let mut orientation_histogram = None;

        if self.params.use_orientation_clustering {
            if let Some(clusters) =
                cluster_orientations(&strong, &self.params.orientation_clustering_params)
            {
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
            return None;
        }

        let graph = build_chessboard_grid_graph(&strong, &self.params.graph, graph_diagonals);

        let components = connected_components(&graph);
        log_graph_summary(&graph, &components, self.params.min_corners);
        debug!(
            "found {} connected grid components after orientation filtering",
            components.len()
        );

        let mut best: Option<(TargetDetection, Vec<usize>, usize)> = None;

        for component in &components {
            if component.len() < self.params.min_corners {
                continue;
            }
            let coords = assign_grid_coordinates(&graph, component);
            if coords.is_empty() {
                debug!(
                    "rejecting component with {} nodes because BFS assigned no grid coordinates",
                    component.len()
                );
                continue;
            }
            let Some((detection, inliers)) = self.component_to_board_coords(&coords, &strong)
            else {
                continue;
            };
            let score = detection.corners.len();
            match best {
                None => best = Some((detection, inliers, score)),
                Some((_, _, best_score)) if score > best_score => {
                    best = Some((detection, inliers, score));
                }
                _ => {}
            }
        }

        let (detection, inliers, _) = best?;
        let graph_debug = Some(build_graph_debug(&graph, &strong));
        debug!(
            "accepted chessboard candidate with {} corners and {} inliers",
            detection.corners.len(),
            inliers.len()
        );

        Some(ChessboardDetectionResult {
            detection,
            inliers,
            orientations: grid_diagonals,
            debug: ChessboardDebug {
                orientation_histogram,
                graph: graph_debug,
            },
        })
    }

    fn component_to_board_coords(
        &self,
        coords: &[(usize, GridIndex)],
        corners: &[Corner],
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

        let mut labeled: Vec<LabeledCorner> = by_grid.into_values().collect();

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
    }
}

fn wrap_angle_pi(theta: f32) -> f32 {
    let mut t = theta % std::f32::consts::PI;
    if t < 0.0 {
        t += std::f32::consts::PI;
    }
    t
}
