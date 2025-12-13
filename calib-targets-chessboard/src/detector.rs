use crate::gridgraph::{assign_grid_coordinates, connected_components, GridGraph};
use crate::params::{ChessboardParams, GridGraphParams};
use calib_targets_core::{
    cluster_orientations, Corner, GridCoords, LabeledCorner, TargetDetection, TargetKind,
};
use log::{info, warn};

/// Simple chessboard detector using ChESS orientations + grid fitting in (u, v) space.
pub struct ChessboardDetector {
    pub params: ChessboardParams,
    pub grid_search: GridGraphParams,
}

pub struct ChessboardDetectionResult {
    pub detection: TargetDetection,
    pub inliers: Vec<usize>,
    pub orientations: Option<[f32; 2]>,
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
    /// For now it returns at most one detection (the largest grid component).
    pub fn detect_from_corners(&self, corners: &[Corner]) -> Option<ChessboardDetectionResult> {
        // 1. Filter by strength.
        let mut strong: Vec<Corner> = corners
            .iter()
            .cloned()
            .filter(|c| c.strength >= self.params.min_strength)
            .collect();

        info!(
            "found {} raw ChESS corners after strength filter",
            strong.len()
        );

        if strong.len() < self.params.min_corners {
            return None;
        }

        // 2. Estimate grid axes from orientations.
        let mut grid_diagonals = None;
        if self.params.use_orientation_clustering {
            if let Some(clusters) =
                cluster_orientations(&strong, &self.params.orientation_clustering_params)
            {
                grid_diagonals = Some(clusters.centers);
                strong = strong
                    .into_iter()
                    .zip(clusters.labels.into_iter())
                    .filter_map(|(mut corner, label)| {
                        label.map(|cluster| {
                            corner.orientation_cluster = Some(cluster);
                            corner
                        })
                    })
                    .collect();
            }
        }

        info!(
            "kept {} ChESS corners after orientation consistency filter",
            strong.len()
        );

        if strong.len() < self.params.min_corners {
            return None;
        }

        let graph = GridGraph::new(&strong, self.grid_search.clone(), grid_diagonals);

        let components = connected_components(&graph);
        info!(
            "found {} connected grid components after orientation filtering",
            components.len()
        );

        let largest_component = components
            .into_iter()
            .max_by_key(|c| c.len())
            .filter(|c| c.len() >= self.params.min_corners)?;

        let coords = assign_grid_coordinates(&graph, &largest_component);
        if coords.is_empty() {
            return None;
        }

        let (detection, inliers) = match self.component_to_board_coords(&coords, &strong) {
            Some(res) => res,
            None => {
                warn!("no valid board coordinates found for largest component");
                return None;
            }
        };

        Some(ChessboardDetectionResult {
            detection,
            inliers,
            orientations: grid_diagonals,
        })
    }

    fn component_to_board_coords(
        &self,
        coords: &[(usize, i32, i32)],
        corners: &[Corner],
    ) -> Option<(TargetDetection, Vec<usize>)> {
        let (min_i, max_i, min_j, max_j) = coords.iter().fold(
            (i32::MAX, i32::MIN, i32::MAX, i32::MIN),
            |acc, &(_, i, j)| (acc.0.min(i), acc.1.max(i), acc.2.min(j), acc.3.max(j)),
        );

        if min_i == i32::MAX || min_j == i32::MAX {
            return None;
        }

        let width = (max_i - min_i + 1) as u32;
        let height = (max_j - min_j + 1) as u32;

        let (board_cols, board_rows, swap_axes) =
            match select_board_size(width, height, &self.params) {
                Some(dim) => dim,
                None => return None,
            };

        let grid_area = (board_cols * board_rows) as f32;
        if grid_area <= f32::EPSILON {
            return None;
        }
        let completeness = coords.len() as f32 / grid_area;
        if let (Some(_), Some(_)) = (self.params.expected_cols, self.params.expected_rows) {
            if completeness < self.params.completeness_threshold {
                return None;
            }
        }

        let mut labeled: Vec<LabeledCorner> = coords
            .iter()
            .map(|(node_idx, i, j)| {
                let corner = &corners[*node_idx];
                let (gi, gj) = if swap_axes {
                    (j - min_j, i - min_i)
                } else {
                    (i - min_i, j - min_j)
                };
                LabeledCorner {
                    position: corner.position,
                    grid: Some(GridCoords { i: gi, j: gj }),
                    id: None,
                    confidence: 1.0,
                }
            })
            .collect();

        labeled.sort_by(|a, b| {
            let ga = a.grid.as_ref().unwrap();
            let gb = b.grid.as_ref().unwrap();
            (ga.j, ga.i).cmp(&(gb.j, gb.i))
        });

        let detection = TargetDetection {
            kind: TargetKind::Chessboard,
            corners: labeled,
        };

        let inliers = coords.iter().map(|(idx, _, _)| *idx).collect();

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
