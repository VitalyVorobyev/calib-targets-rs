use crate::geom::is_aligned_or_orthogonal;
use crate::gridgraph::{assign_grid_coordinates, connected_components, GridGraph, GridGraphParams};
use crate::params::ChessboardParams;
use calib_targets_core::{Corner, GridCoords, LabeledCorner, TargetDetection, TargetKind};
use log::info;
use nalgebra::Vector2;

/// Estimate two orthogonal grid axes from ChESS corner orientations.
///
/// This respects the fact that your orientations are defined modulo π.
/// It uses a "double-angle" trick to get a dominant direction, then
/// constructs the perpendicular as the second axis.
///
/// Returns (u, v) unit vectors in image pixel space.
pub fn estimate_grid_axes_from_orientations(corners: &[Corner]) -> Option<f32> {
    if corners.is_empty() {
        return None;
    }

    // Accumulate in double-angle space to handle θ ≡ θ + π
    let mut sum = Vector2::<f32>::zeros();
    let mut weight_sum = 0.0f32;

    for c in corners {
        let theta = c.orientation;
        // You can weight by strength to favor strong corners.
        let w = c.strength.max(0.0);
        if w <= 0.0 {
            continue;
        }

        let two_theta = 2.0 * theta;
        let v = Vector2::new(two_theta.cos(), two_theta.sin());
        sum += w * v;
        weight_sum += w;
    }

    if weight_sum <= 0.0 {
        return None;
    }

    let mean = sum / weight_sum;
    if mean.norm_squared() < 1e-6 {
        // No dominant orientation.
        return None;
    }

    // Back to single-angle space.
    let mean_two_angle = mean.y.atan2(mean.x);
    let mean_theta = 0.5 * mean_two_angle;
    Some(mean_theta)
}

/// Simple chessboard detector using ChESS orientations + grid fitting in (u, v) space.
pub struct ChessboardDetector {
    pub params: ChessboardParams,
}

impl ChessboardDetector {
    pub fn new(params: ChessboardParams) -> Self {
        Self { params }
    }

    /// Main entry point: find chessboard(s) in a cloud of ChESS corners.
    ///
    /// This function expects corners already computed by your ChESS crate.
    /// For now it returns at most one detection (the best grid).
    pub fn detect_from_corners(&self, corners: &[Corner]) -> Vec<TargetDetection> {
        // 1. Filter by strength.
        let strong: Vec<Corner> = corners
            .iter()
            .cloned()
            .filter(|c| c.strength >= self.params.min_strength)
            .collect();

        info!(
            "found {} raw ChESS corners after strength filter",
            strong.len()
        );

        if strong.len() < self.params.min_corners {
            return Vec::new();
        }

        // 2. Estimate grid axes from orientations.
        let Some(theta_u) = estimate_grid_axes_from_orientations(&strong) else {
            info!("failed to estimate grid axes from orientations");
            return Vec::new();
        };

        let aligned_corners: Vec<Corner> = strong
            .iter()
            .cloned()
            .filter(|c| {
                is_aligned_or_orthogonal(
                    theta_u,
                    c.orientation,
                    self.params.orientation_tolerance_rad,
                )
            })
            .collect();

        info!(
            "kept {} ChESS corners after orientation consistency filter",
            aligned_corners.len()
        );

        if aligned_corners.len() < self.params.grid_search.min_corners {
            return Vec::new();
        }

        let graph_params = GridGraphParams {
            min_spacing_pix: 5.0,
            max_spacing_pix: 100.0,
            k_neighbors: 8,
            orientation_tolerance_rad: self.params.orientation_tolerance_rad,
        };
        let graph = GridGraph::new(&aligned_corners, graph_params);

        let components = connected_components(&graph);
        info!(
            "found {} connected grid components after orientation filtering",
            components.len()
        );

        let mut best: Option<(f32, usize, TargetDetection)> = None;

        for component in components.into_iter() {
            if component.len() < self.params.grid_search.min_corners {
                continue;
            }

            let coords = assign_grid_coordinates(&graph, &component);
            if coords.is_empty() {
                continue;
            }

            let (min_i, max_i, min_j, max_j) = coords.iter().fold(
                (i32::MAX, i32::MIN, i32::MAX, i32::MIN),
                |acc, &(_, i, j)| (acc.0.min(i), acc.1.max(i), acc.2.min(j), acc.3.max(j)),
            );

            if min_i == i32::MAX || min_j == i32::MAX {
                continue;
            }

            let width = (max_i - min_i + 1) as u32;
            let height = (max_j - min_j + 1) as u32;

            let (board_cols, board_rows, swap_axes) =
                match (self.params.expected_cols, self.params.expected_rows) {
                    (Some(expected_cols), Some(expected_rows)) => {
                        let fits_direct = width <= expected_cols && height <= expected_rows;
                        let fits_swapped = width <= expected_rows && height <= expected_cols;

                        if !fits_direct && !fits_swapped {
                            continue;
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

                        (expected_cols, expected_rows, swap_axes)
                    }
                    _ => (width, height, false),
                };

            let grid_area = (board_cols * board_rows) as f32;
            if grid_area <= f32::EPSILON {
                continue;
            }
            let completeness = coords.len() as f32 / grid_area;
            if let (Some(_), Some(_)) = (self.params.expected_cols, self.params.expected_rows) {
                if completeness < self.params.completeness_threshold {
                    continue;
                }
            }

            let mut labeled: Vec<LabeledCorner> = coords
                .iter()
                .map(|(node_idx, i, j)| {
                    let corner = &aligned_corners[*node_idx];
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

            let is_better = match &best {
                None => true,
                Some((best_completeness, best_count, _)) => {
                    completeness > *best_completeness + 1e-6
                        || ((completeness - *best_completeness).abs() <= 1e-6
                            && coords.len() > *best_count)
                }
            };

            if is_better {
                best = Some((completeness, coords.len(), detection));
            }
        }

        best.map(|(_, _, det)| vec![det]).unwrap_or_default()
    }
}
