use crate::gridgraph::{
    assign_grid_coordinates, connected_components, GridGraph, NeighborDirection,
};
use crate::params::{ChessboardParams, GridGraphParams};
use calib_targets_core::{
    cluster_orientations, estimate_grid_axes_from_orientations, Corner, GridCoords, LabeledCorner,
    OrientationHistogram, TargetDetection, TargetKind,
};
use log::{debug, warn};
use serde::{Deserialize, Serialize};
use std::f32::consts::FRAC_PI_2;
use std::time::Instant;

#[cfg(feature = "tracing")]
use tracing::instrument;

/// Simple chessboard detector using ChESS orientations + grid fitting in (u, v) space.
#[derive(Debug)]
pub struct ChessboardDetector {
    pub params: ChessboardParams,
    pub grid_search: GridGraphParams,
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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ChessboardStageTimings {
    pub filter_ms: f64,
    pub orientation_ms: f64,
    pub graph_components_ms: f64,
    pub select_ms: f64,
    pub total_ms: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ChessboardDiagnostics {
    pub input_corner_count: usize,
    pub strong_corner_count: usize,
    pub orientation_filtered_count: usize,
    pub component_count: usize,
    pub selected_grid_width: Option<u32>,
    pub selected_grid_height: Option<u32>,
    pub selected_grid_completeness: Option<f32>,
    pub final_corner_count: usize,
    pub timings: ChessboardStageTimings,
}

#[derive(Debug)]
pub struct ChessboardDetectionRun {
    pub detection: Option<ChessboardDetectionResult>,
    pub diagnostics: ChessboardDiagnostics,
}

#[derive(Debug)]
struct SelectedComponent {
    detection: TargetDetection,
    inliers: Vec<usize>,
    grid_width: u32,
    grid_height: u32,
    completeness: f32,
}

impl ChessboardDetector {
    pub fn new(params: ChessboardParams) -> Self {
        Self {
            grid_search: GridGraphParams::default(),
            params,
        }
    }

    pub fn with_grid_search(mut self, grid_search: GridGraphParams) -> Self {
        self.grid_search = grid_search;
        self
    }

    /// Main entry point: find chessboard(s) in a cloud of ChESS corners.
    ///
    /// This function expects corners already computed by your ChESS crate.
    /// For now it returns at most one detection (the best-scoring grid component).
    #[cfg_attr(feature = "tracing", instrument(level = "info", skip(self, corners), fields(num_corners=corners.len())))]
    pub fn detect_from_corners(&self, corners: &[Corner]) -> Option<ChessboardDetectionResult> {
        self.detect_from_corners_with_diagnostics(corners).detection
    }

    #[cfg_attr(feature = "tracing", instrument(level = "info", skip(self, corners), fields(num_corners=corners.len())))]
    pub fn detect_from_corners_with_diagnostics(
        &self,
        corners: &[Corner],
    ) -> ChessboardDetectionRun {
        let total_start = Instant::now();
        let mut diagnostics = ChessboardDiagnostics {
            input_corner_count: corners.len(),
            ..ChessboardDiagnostics::default()
        };

        let filter_start = Instant::now();
        let mut strong: Vec<Corner> = corners
            .iter()
            .filter(|c| c.strength >= self.params.min_corner_strength)
            .cloned()
            .collect();
        diagnostics.strong_corner_count = strong.len();
        diagnostics.timings.filter_ms = elapsed_ms(filter_start);

        debug!(
            "found {} raw ChESS corners after strength filter",
            strong.len()
        );

        if strong.len() < self.params.min_corners {
            diagnostics.orientation_filtered_count = strong.len();
            diagnostics.timings.total_ms = elapsed_ms(total_start);
            return ChessboardDetectionRun {
                detection: None,
                diagnostics,
            };
        }

        let orientation_start = Instant::now();
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

        diagnostics.orientation_filtered_count = strong.len();
        diagnostics.timings.orientation_ms = elapsed_ms(orientation_start);

        debug!(
            "kept {} ChESS corners after orientation consistency filter",
            strong.len()
        );

        if strong.len() < self.params.min_corners {
            diagnostics.timings.total_ms = elapsed_ms(total_start);
            return ChessboardDetectionRun {
                detection: None,
                diagnostics,
            };
        }

        let graph_start = Instant::now();
        let graph = GridGraph::new(&strong, self.grid_search.clone(), graph_diagonals);
        let components = connected_components(&graph);
        diagnostics.component_count = components.len();
        diagnostics.timings.graph_components_ms = elapsed_ms(graph_start);

        debug!(
            "found {} connected grid components after orientation filtering",
            components.len()
        );

        let select_start = Instant::now();
        let mut best: Option<(SelectedComponent, usize)> = None;

        for component in &components {
            if component.len() < self.params.min_corners {
                continue;
            }
            let coords = assign_grid_coordinates(&graph, component);
            if coords.is_empty() {
                continue;
            }
            let Some(selected) = self.component_to_board_coords(&coords, &strong) else {
                continue;
            };
            let score = selected.detection.corners.len();
            match best {
                None => best = Some((selected, score)),
                Some((_, best_score)) if score > best_score => {
                    best = Some((selected, score));
                }
                _ => {}
            }
        }
        diagnostics.timings.select_ms = elapsed_ms(select_start);

        let detection = if let Some((selected, _)) = best {
            diagnostics.selected_grid_width = Some(selected.grid_width);
            diagnostics.selected_grid_height = Some(selected.grid_height);
            diagnostics.selected_grid_completeness = Some(selected.completeness);
            diagnostics.final_corner_count = selected.detection.corners.len();

            let graph_debug = Some(build_graph_debug(&graph, &strong));
            Some(ChessboardDetectionResult {
                detection: selected.detection,
                inliers: selected.inliers,
                orientations: grid_diagonals,
                debug: ChessboardDebug {
                    orientation_histogram,
                    graph: graph_debug,
                },
            })
        } else {
            None
        };

        diagnostics.timings.total_ms = elapsed_ms(total_start);
        ChessboardDetectionRun {
            detection,
            diagnostics,
        }
    }

    fn component_to_board_coords(
        &self,
        coords: &[(usize, i32, i32)],
        corners: &[Corner],
    ) -> Option<SelectedComponent> {
        let (min_i, max_i, min_j, max_j) = coords.iter().fold(
            (i32::MAX, i32::MIN, i32::MAX, i32::MIN),
            |acc, &(_, i, j)| (acc.0.min(i), acc.1.max(i), acc.2.min(j), acc.3.max(j)),
        );

        if min_i == i32::MAX || min_j == i32::MAX {
            return None;
        }

        let width = (max_i - min_i + 1) as u32;
        let height = (max_j - min_j + 1) as u32;
        let (board_cols, board_rows, swap_axes) = select_board_size(width, height, &self.params)?;

        let grid_area = (board_cols * board_rows) as f32;
        if grid_area <= f32::EPSILON {
            return None;
        }

        let mut by_grid: std::collections::HashMap<GridCoords, LabeledCorner> =
            std::collections::HashMap::new();
        for &(node_idx, i, j) in coords {
            let corner = &corners[node_idx];
            let (gi, gj) = if swap_axes {
                (j - min_j, i - min_i)
            } else {
                (i - min_i, j - min_j)
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

        Some(SelectedComponent {
            detection,
            inliers,
            grid_width: board_cols,
            grid_height: board_rows,
            completeness,
        })
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

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1_000.0
}
