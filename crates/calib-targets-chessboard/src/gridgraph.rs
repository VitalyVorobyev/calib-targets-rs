use crate::geom::{angle_diff_abs, is_orthogonal};
use crate::params::GridGraphParams;
use calib_targets_core::Corner;
use nalgebra::Vector2;
use projective_grid::{GridGraph, NeighborCandidate, NeighborDirection, NeighborValidator};

pub use projective_grid::{assign_grid_coordinates, connected_components};

/// Small helper: angle between an undirected axis `axis_angle`
/// (defined modulo π) and a directed vector angle `vec_angle`.
/// Returns a value in `[0, π/2]`.
fn axis_vec_diff(axis_angle: f32, vec_angle: f32) -> f32 {
    let two_pi = 2.0 * std::f32::consts::PI;

    // Difference in [-π, π).
    let mut diff = (vec_angle - axis_angle).rem_euclid(two_pi);
    if diff >= std::f32::consts::PI {
        diff -= two_pi;
    }
    let diff_abs = diff.abs();

    // Axis is undirected: θ and θ+π describe the same line.
    diff_abs.min(std::f32::consts::PI - diff_abs)
}

/// Convert angle (radians) to unit 2D vector.
fn angle_to_unit(theta: f32) -> nalgebra::Vector2<f32> {
    nalgebra::Vector2::new(theta.cos(), theta.sin())
}

fn direction_quadrant(vec_to_neighbor: &Vector2<f32>) -> NeighborDirection {
    if vec_to_neighbor.x.abs() > vec_to_neighbor.y.abs() {
        if vec_to_neighbor.x >= 0.0 {
            NeighborDirection::Right
        } else {
            NeighborDirection::Left
        }
    } else if vec_to_neighbor.y >= 0.0 {
        NeighborDirection::Down
    } else {
        NeighborDirection::Up
    }
}

/// Per-corner data needed for chessboard neighbor validation.
pub struct ChessboardPointData {
    pub orientation: f32,
    pub orientation_cluster: Option<usize>,
}

impl ChessboardPointData {
    pub fn from_corners(corners: &[Corner]) -> Vec<Self> {
        corners
            .iter()
            .map(|c| Self {
                orientation: c.orientation,
                orientation_cluster: c.orientation_cluster,
            })
            .collect()
    }
}

/// Validator that uses only diagonal-to-edge angle relationship (no orientation clustering).
pub struct ChessboardSimpleValidator {
    pub min_spacing_pix: f32,
    pub max_spacing_pix: f32,
    pub orientation_tolerance_deg: f32,
}

impl NeighborValidator for ChessboardSimpleValidator {
    type PointData = ChessboardPointData;

    fn validate(
        &self,
        _source_index: usize,
        source_data: &Self::PointData,
        candidate: &NeighborCandidate,
        candidate_data: &Self::PointData,
    ) -> Option<(NeighborDirection, f32)> {
        // 1. Corner directions should be approximately orthogonal.
        let tol = self.orientation_tolerance_deg.to_radians();
        if !is_orthogonal(source_data.orientation, candidate_data.orientation, tol) {
            return None;
        }

        // 2. Check distance.
        if candidate.distance < self.min_spacing_pix || candidate.distance > self.max_spacing_pix {
            return None;
        }

        // 3. Relationship between corner directions and edge direction.
        let edge_angle = candidate.offset.y.atan2(candidate.offset.x);
        let diff_corner = axis_vec_diff(source_data.orientation, edge_angle);
        let diff_neighbor = axis_vec_diff(candidate_data.orientation, edge_angle);
        let expected = std::f32::consts::FRAC_PI_4; // 45°

        let score_corner = (diff_corner - expected).abs();
        let score_neighbor = (diff_neighbor - expected).abs();

        if score_corner > tol || score_neighbor > tol {
            return None;
        }

        // 4. Classify neighbor direction in image space.
        let direction = direction_quadrant(&candidate.offset);

        let score_orientation = (std::f32::consts::FRAC_PI_2
            - angle_diff_abs(source_data.orientation, candidate_data.orientation))
        .abs();

        let score = score_corner + score_neighbor + score_orientation;
        Some((direction, score))
    }
}

/// Validator that uses orientation clustering and canonical grid axes.
pub struct ChessboardClusterValidator {
    pub min_spacing_pix: f32,
    pub max_spacing_pix: f32,
    pub orientation_tolerance_deg: f32,
    pub grid_diagonals: [f32; 2],
}

impl NeighborValidator for ChessboardClusterValidator {
    type PointData = ChessboardPointData;

    fn validate(
        &self,
        _source_index: usize,
        source_data: &Self::PointData,
        candidate: &NeighborCandidate,
        candidate_data: &Self::PointData,
    ) -> Option<(NeighborDirection, f32)> {
        // 0. Need valid orientation clusters for both corners.
        let (Some(_ci), Some(_cj)) = (
            source_data.orientation_cluster,
            candidate_data.orientation_cluster,
        ) else {
            return None;
        };
        if _ci == _cj {
            return None; // Same axis cluster; not a valid neighbor.
        }

        // 2. Check distance.
        if candidate.distance < self.min_spacing_pix || candidate.distance > self.max_spacing_pix {
            return None;
        }

        // 3. Relationship between corner diagonals and edge direction.
        let e = candidate.offset / candidate.distance;

        let o0 = angle_to_unit(self.grid_diagonals[0]);
        let o1 = angle_to_unit(self.grid_diagonals[1]);

        let v_plus = o0 + o1;
        let mut v_minus = o0 - o1;

        if v_plus.norm_squared() < 1e-6 || v_minus.norm_squared() < 1e-6 {
            return None;
        }

        // Canonicalize v_minus sign for right-handed frame.
        let det = v_minus.x * v_plus.y - v_minus.y * v_plus.x;
        if det < 0.0 {
            v_minus = -v_minus;
        }

        let v_plus_unit = v_plus.normalize();
        let v_minus_unit = v_minus.normalize();

        let dot_h = v_minus_unit.dot(&e);
        let dot_v = v_plus_unit.dot(&e);

        let best_alignment = dot_h.abs().max(dot_v.abs());

        if best_alignment < self.orientation_tolerance_deg.to_radians().cos() {
            return None;
        }

        // 4. Classify neighbor direction using canonical grid axes.
        let direction = if dot_h.abs() > dot_v.abs() {
            if dot_h >= 0.0 {
                NeighborDirection::Right
            } else {
                NeighborDirection::Left
            }
        } else if dot_v >= 0.0 {
            NeighborDirection::Down
        } else {
            NeighborDirection::Up
        };

        let score = 1.0 - best_alignment;
        Some((direction, score))
    }
}

/// Build a chessboard grid graph from corners.
pub fn build_chessboard_grid_graph(
    corners: &[Corner],
    params: &GridGraphParams,
    grid_diagonals: Option<[f32; 2]>,
) -> GridGraph {
    let positions: Vec<_> = corners.iter().map(|c| c.position).collect();
    let point_data = ChessboardPointData::from_corners(corners);

    let graph_params = projective_grid::GridGraphParams {
        k_neighbors: params.k_neighbors,
        max_distance: params.max_spacing_pix,
    };

    if let Some(diags) = grid_diagonals {
        let validator = ChessboardClusterValidator {
            min_spacing_pix: params.min_spacing_pix,
            max_spacing_pix: params.max_spacing_pix,
            orientation_tolerance_deg: params.orientation_tolerance_deg,
            grid_diagonals: diags,
        };
        GridGraph::build(&positions, &point_data, &validator, &graph_params)
    } else {
        let validator = ChessboardSimpleValidator {
            min_spacing_pix: params.min_spacing_pix,
            max_spacing_pix: params.max_spacing_pix,
            orientation_tolerance_deg: params.orientation_tolerance_deg,
        };
        GridGraph::build(&positions, &point_data, &validator, &graph_params)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use calib_targets_core::Corner;
    use nalgebra::Point2;
    use projective_grid::NodeNeighbor;
    use std::collections::HashMap;
    use std::f32::consts::FRAC_PI_4;

    fn make_corner(x: f32, y: f32, orientation: f32) -> Corner {
        Corner {
            position: Point2::new(x, y),
            orientation,
            orientation_cluster: None,
            strength: 1.0,
        }
    }

    fn neighbor_map(neighbors: &[NodeNeighbor]) -> HashMap<NeighborDirection, &NodeNeighbor> {
        neighbors.iter().map(|n| (n.direction, n)).collect()
    }

    #[test]
    fn finds_axis_neighbors_in_regular_grid() {
        let spacing = 10.0;
        let cols = 3;
        let rows = 3;

        let mut corners = Vec::new();
        for j in 0..rows {
            for i in 0..cols {
                let orientation = if (i + j) % 2 == 0 {
                    FRAC_PI_4
                } else {
                    3.0 * FRAC_PI_4
                };
                corners.push(make_corner(
                    i as f32 * spacing,
                    j as f32 * spacing,
                    orientation,
                ));
            }
        }

        let params = GridGraphParams {
            min_spacing_pix: 5.0,
            max_spacing_pix: 15.0,
            ..Default::default()
        };
        let graph = build_chessboard_grid_graph(&corners, &params, None);

        let idx = |i: usize, j: usize| j * cols + i;

        let center = neighbor_map(&graph.neighbors[idx(1, 1)]);
        assert_eq!(4, center.len());
        assert_eq!(idx(0, 1), center[&NeighborDirection::Left].index);
        assert_eq!(idx(2, 1), center[&NeighborDirection::Right].index);
        assert_eq!(idx(1, 0), center[&NeighborDirection::Up].index);
        assert_eq!(idx(1, 2), center[&NeighborDirection::Down].index);
        for dir in [
            NeighborDirection::Left,
            NeighborDirection::Right,
            NeighborDirection::Up,
            NeighborDirection::Down,
        ] {
            assert!((center[&dir].distance - spacing).abs() < 1e-4);
        }

        let top_left = neighbor_map(&graph.neighbors[idx(0, 0)]);
        assert_eq!(2, top_left.len());
        assert!(top_left.contains_key(&NeighborDirection::Right));
        assert!(top_left.contains_key(&NeighborDirection::Down));

        let top_mid = neighbor_map(&graph.neighbors[idx(1, 0)]);
        assert_eq!(3, top_mid.len());
        assert!(top_mid.contains_key(&NeighborDirection::Left));
        assert!(top_mid.contains_key(&NeighborDirection::Right));
        assert!(top_mid.contains_key(&NeighborDirection::Down));
    }

    #[test]
    fn rejects_neighbors_when_orientation_relation_invalid() {
        let spacing = 10.0;
        let corners = vec![
            make_corner(0.0, 0.0, FRAC_PI_4),
            make_corner(spacing, 0.0, FRAC_PI_4),
        ];

        let params = GridGraphParams {
            min_spacing_pix: 5.0,
            max_spacing_pix: 15.0,
            k_neighbors: 2,
            ..Default::default()
        };
        let graph = build_chessboard_grid_graph(&corners, &params, None);

        assert!(graph.neighbors[0].is_empty());
        assert!(graph.neighbors[1].is_empty());
    }

    #[test]
    fn rejects_neighbors_outside_distance_window() {
        let spacing = 30.0;
        let corners = vec![
            make_corner(0.0, 0.0, FRAC_PI_4),
            make_corner(spacing, 0.0, 3.0 * FRAC_PI_4),
        ];

        let params = GridGraphParams {
            min_spacing_pix: 5.0,
            max_spacing_pix: 15.0,
            k_neighbors: 2,
            ..Default::default()
        };
        let graph = build_chessboard_grid_graph(&corners, &params, None);

        assert!(graph.neighbors[0].is_empty());
        assert!(graph.neighbors[1].is_empty());
    }

    fn make_clustered_corner(x: f32, y: f32, orientation: f32, cluster: usize) -> Corner {
        Corner {
            position: Point2::new(x, y),
            orientation,
            orientation_cluster: Some(cluster),
            strength: 1.0,
        }
    }

    #[test]
    fn rotated_grid_forms_single_component() {
        let spacing = 20.0;
        let angle = 40.0f32.to_radians();
        let cols = 4;
        let rows = 4;

        let ax = Vector2::new(angle.cos(), angle.sin());
        let ay = Vector2::new(-angle.sin(), angle.cos());

        let diag0 = angle + FRAC_PI_4;
        let diag1 = angle + 3.0 * FRAC_PI_4;
        let grid_diagonals = [diag0, diag1];

        let mut corners = Vec::new();
        for j in 0..rows {
            for i in 0..cols {
                let pos = ax * (i as f32 * spacing) + ay * (j as f32 * spacing);
                let cluster = (i + j) % 2;
                let ori = if cluster == 0 { diag0 } else { diag1 };
                corners.push(make_clustered_corner(
                    pos.x + 100.0,
                    pos.y + 100.0,
                    ori,
                    cluster,
                ));
            }
        }

        let params = GridGraphParams {
            min_spacing_pix: spacing * 0.5,
            max_spacing_pix: spacing * 1.5,
            k_neighbors: 8,
            ..Default::default()
        };
        let graph = build_chessboard_grid_graph(&corners, &params, Some(grid_diagonals));

        let components = connected_components(&graph);
        assert_eq!(
            1,
            components.len(),
            "Rotated grid should form a single connected component, got {}",
            components.len()
        );
        assert_eq!(cols * rows, components[0].len());

        let coords = assign_grid_coordinates(&graph, &components[0]);
        assert_eq!(cols * rows, coords.len());
        let coord_set: std::collections::HashSet<(i32, i32)> =
            coords.iter().map(|&(_, g)| (g.i, g.j)).collect();
        assert_eq!(
            cols * rows,
            coord_set.len(),
            "All grid coords must be unique"
        );
    }

    #[test]
    fn direction_symmetry_on_rotated_grid() {
        let spacing = 20.0;
        let angle = 55.0f32.to_radians();
        let ax = Vector2::new(angle.cos(), angle.sin());
        let ay = Vector2::new(-angle.sin(), angle.cos());

        let diag0 = angle + FRAC_PI_4;
        let diag1 = angle + 3.0 * FRAC_PI_4;
        let grid_diagonals = [diag0, diag1];

        let mut corners = Vec::new();
        for j in 0..3 {
            for i in 0..3 {
                let pos = ax * (i as f32 * spacing) + ay * (j as f32 * spacing);
                let cluster = (i + j) % 2;
                let ori = if cluster == 0 { diag0 } else { diag1 };
                corners.push(make_clustered_corner(
                    pos.x + 50.0,
                    pos.y + 50.0,
                    ori,
                    cluster,
                ));
            }
        }

        let params = GridGraphParams {
            min_spacing_pix: spacing * 0.5,
            max_spacing_pix: spacing * 1.5,
            k_neighbors: 8,
            ..Default::default()
        };
        let graph = build_chessboard_grid_graph(&corners, &params, Some(grid_diagonals));

        for (a, neighbors) in graph.neighbors.iter().enumerate() {
            for n in neighbors {
                let b = n.index;
                let b_neighbors = &graph.neighbors[b];
                let back = b_neighbors.iter().find(|nn| nn.index == a);
                assert!(
                    back.is_some(),
                    "Edge {a}->{b} exists but reverse {b}->{a} does not"
                );
                assert_eq!(
                    n.direction.opposite(),
                    back.unwrap().direction,
                    "Edge {a}->{b} is {:?} but {b}->{a} is {:?}, expected {:?}",
                    n.direction,
                    back.unwrap().direction,
                    n.direction.opposite(),
                );
            }
        }
    }

    #[test]
    fn grid_at_45_degrees_forms_single_component() {
        let spacing = 15.0;
        let angle = 45.0f32.to_radians();
        let ax = Vector2::new(angle.cos(), angle.sin());
        let ay = Vector2::new(-angle.sin(), angle.cos());

        let diag0 = angle + FRAC_PI_4;
        let diag1 = angle + 3.0 * FRAC_PI_4;
        let grid_diagonals = [diag0, diag1];

        let mut corners = Vec::new();
        for j in 0..5 {
            for i in 0..5 {
                let pos = ax * (i as f32 * spacing) + ay * (j as f32 * spacing);
                let cluster = (i + j) % 2;
                let ori = if cluster == 0 { diag0 } else { diag1 };
                corners.push(make_clustered_corner(
                    pos.x + 80.0,
                    pos.y + 80.0,
                    ori,
                    cluster,
                ));
            }
        }

        let params = GridGraphParams {
            min_spacing_pix: spacing * 0.5,
            max_spacing_pix: spacing * 1.5,
            k_neighbors: 8,
            ..Default::default()
        };
        let graph = build_chessboard_grid_graph(&corners, &params, Some(grid_diagonals));

        let components = connected_components(&graph);
        assert_eq!(1, components.len());
        assert_eq!(25, components[0].len());
    }

    #[test]
    fn keeps_best_candidate_per_direction() {
        let spacing = 10.0;
        let worse_spacing = 12.0;

        let corners = vec![
            make_corner(0.0, 0.0, FRAC_PI_4),           // center (idx 0)
            make_corner(spacing, 0.0, 3.0 * FRAC_PI_4), // better right (idx 1)
            make_corner(
                worse_spacing,
                0.0,
                3.0 * FRAC_PI_4 + 0.1, // slightly worse orientation
            ), // worse right (idx 2)
            make_corner(-spacing, 0.0, 3.0 * FRAC_PI_4), // left (idx 3)
        ];

        let params = GridGraphParams {
            min_spacing_pix: 5.0,
            max_spacing_pix: 15.0,
            k_neighbors: 4,
            ..Default::default()
        };
        let graph = build_chessboard_grid_graph(&corners, &params, None);

        let map = neighbor_map(&graph.neighbors[0]);
        assert_eq!(2, map.len()); // left + right only
        assert_eq!(1, map[&NeighborDirection::Right].index); // best right chosen
        assert_eq!(3, map[&NeighborDirection::Left].index);
    }
}
