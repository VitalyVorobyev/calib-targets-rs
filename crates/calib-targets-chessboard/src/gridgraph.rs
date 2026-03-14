use crate::geom::{angle_diff_abs, is_orthogonal};
use crate::params::GridGraphParams;
use calib_targets_core::Corner;
use kiddo::{KdTree, SquaredEuclidean};
use nalgebra::Vector2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NeighborDirection {
    Right,
    Left,
    Up,
    Down,
}

#[derive(Debug)]
pub struct NodeNeighbor {
    pub direction: NeighborDirection,
    pub index: usize,
    pub distance: f32,
    pub score: f32,
}

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

fn is_good_neighbor_with_orientation(
    corner: &Corner,
    neighbor: &Corner,
    neighbor_index: usize,
    params: &GridGraphParams,
    grid_diagonals: [f32; 2],
) -> Option<NodeNeighbor> {
    // 0. Need valid orientation clusters for both corners.
    let (Some(ci), Some(cj)) = (corner.orientation_cluster, neighbor.orientation_cluster) else {
        return None;
    };
    if ci == cj {
        return None; // Same axis cluster; not a valid neighbor.
    }

    // 2. Check distance between corners is within expected spacing.
    let vec_to_neighbor = neighbor.position - corner.position;
    let distance_sq = vec_to_neighbor.norm_squared();
    let distance = distance_sq.sqrt();

    if distance < params.min_spacing_pix || distance > params.max_spacing_pix {
        return None;
    }

    // 3. Relationship between corner diagonals and edge direction.
    //
    // Given two diagonal directions o0, o1 (roughly orthogonal), the local
    // *grid axes* at this edge are approximated by:
    //
    //   v_plus  = o0 + o1
    //   v_minus = o0 - o1
    //
    // In an ideal chessboard, these point along the row/column directions.
    // Under perspective, they are still the two principal directions of the
    // local grid in the image, up to scale. A valid neighbor edge should
    // be nearly collinear with one of them.
    //
    // IMPORTANT: Use canonical axis order `grid_diagonals[0]` and `[1]`
    // (not `[ci]`/`[cj]`) so that all edges share the same reference frame.
    // With per-edge ci/cj, swapping corner/neighbor would negate v_minus,
    // breaking direction symmetry (A->B Right iff B->A Left).
    //
    let e = nalgebra::Vector2::new(vec_to_neighbor.x, vec_to_neighbor.y) / distance;

    let o0 = angle_to_unit(grid_diagonals[0]);
    let o1 = angle_to_unit(grid_diagonals[1]);

    let v_plus = o0 + o1;
    let mut v_minus = o0 - o1;

    // Both axes must be well-defined (non-degenerate diagonals).
    if v_plus.norm_squared() < 1e-6 || v_minus.norm_squared() < 1e-6 {
        return None;
    }

    // Canonicalize v_minus sign: the order of grid_diagonals[0] vs [1] is
    // arbitrary (depends on clustering), so v_minus = o0 - o1 may point in
    // either direction along its axis. Fix the sign so that (v_minus, v_plus)
    // form a right-handed frame in image coordinates (y-down). This ensures
    // the BFS grid coordinates produce correctly-wound (CW) cell quads for
    // marker sampling, regardless of diagonal ordering.
    let det = v_minus.x * v_plus.y - v_minus.y * v_plus.x;
    if det < 0.0 {
        v_minus = -v_minus;
    }

    let v_plus_unit = v_plus.normalize();
    let v_minus_unit = v_minus.normalize();

    // v_minus is the grid "horizontal" axis (Right/Left) and v_plus is the
    // grid "vertical" axis (Down/Up). For an unrotated board in image space:
    //   v_minus ≈ (+1, 0)  →  image-right
    //   v_plus  ≈ (0, +1)  →  image-down
    let dot_h = v_minus_unit.dot(&e);
    let dot_v = v_plus_unit.dot(&e);

    let best_alignment = dot_h.abs().max(dot_v.abs());

    // Require decent alignment with at least one of the axes.
    if best_alignment < params.orientation_tolerance_deg.to_radians().cos() {
        return None;
    }

    // 4. Classify neighbor direction using canonical grid axes.
    //
    // Use the local grid geometry (v_minus, v_plus) instead of image x/y
    // axes. This makes direction labels perspective-invariant and ensures
    // that edge A->B and B->A always get opposite directions (since
    // flipping the edge vector negates both dot products).
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

    let score = 1.0 - best_alignment; // want small
    Some(NodeNeighbor {
        direction,
        index: neighbor_index,
        distance,
        score,
    })
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

/// Convert angle (radians) to unit 2D vector.
fn angle_to_unit(theta: f32) -> nalgebra::Vector2<f32> {
    nalgebra::Vector2::new(theta.cos(), theta.sin())
}

fn is_good_neighbor(
    corner: &Corner,
    neighbor: &Corner,
    neighbor_index: usize,
    params: &GridGraphParams,
) -> Option<NodeNeighbor> {
    // 1. Corner directions should be approximately orthogonal.
    if !is_orthogonal(
        corner.orientation,
        neighbor.orientation,
        params.orientation_tolerance_deg.to_radians(),
    ) {
        return None;
    }

    // 2. Check distance between corners is within expected spacing.
    let vec_to_neighbor = neighbor.position - corner.position;
    let distance_sq = vec_to_neighbor.norm_squared();
    let distance = distance_sq.sqrt();

    if distance < params.min_spacing_pix || distance > params.max_spacing_pix {
        return None;
    }

    // 3. Relationship between corner directions and edge direction.
    //
    // Corner orientation is defined as the diagonal along the white
    // squares, i.e. rotated 45° from the grid directions. For a valid
    // neighbor relation, the direction vector between the corners
    // should be at ~45° to *each* corner orientation.
    let edge_angle = vec_to_neighbor.y.atan2(vec_to_neighbor.x);
    let diff_corner = axis_vec_diff(corner.orientation, edge_angle);
    let diff_neighbor = axis_vec_diff(neighbor.orientation, edge_angle);
    let expected = std::f32::consts::FRAC_PI_4; // 45°
    let tol = params.orientation_tolerance_deg.to_radians();

    let score_corner = (diff_corner - expected).abs();
    let score_neighbor = (diff_neighbor - expected).abs();

    if score_corner > tol || score_neighbor > tol {
        return None;
    }

    // 4. Classify neighbor direction in image space.
    let direction = direction_quadrant(&vec_to_neighbor);

    let score_orientation = (std::f32::consts::FRAC_PI_2
        - angle_diff_abs(corner.orientation, neighbor.orientation))
    .abs();

    let score = score_corner + score_neighbor + score_orientation;

    Some(NodeNeighbor {
        direction,
        index: neighbor_index,
        distance,
        score,
    })
}

/// Keep at most one neighbor per direction, choosing the lowest-score candidate.
fn select_neighbors(candidates: Vec<NodeNeighbor>) -> Vec<NodeNeighbor> {
    let mut best: [Option<NodeNeighbor>; 4] = [None, None, None, None];

    for candidate in candidates.into_iter() {
        let slot = match candidate.direction {
            NeighborDirection::Right => &mut best[0],
            NeighborDirection::Left => &mut best[1],
            NeighborDirection::Up => &mut best[2],
            NeighborDirection::Down => &mut best[3],
        };

        let replace = match slot {
            None => true,
            Some(current) => {
                candidate.score < current.score
                    || (candidate.score == current.score && candidate.distance < current.distance)
            }
        };

        if replace {
            *slot = Some(candidate);
        }
    }

    best.into_iter().flatten().collect()
}

pub struct GridGraph {
    pub neighbors: Vec<Vec<NodeNeighbor>>, // For each node, list of neighbors
}

pub fn connected_components(graph: &GridGraph) -> Vec<Vec<usize>> {
    let mut visited = vec![false; graph.neighbors.len()];
    let mut components = Vec::new();

    for start in 0..graph.neighbors.len() {
        if visited[start] {
            continue;
        }

        let mut component = Vec::new();
        let mut stack = vec![start];

        while let Some(node) = stack.pop() {
            if visited[node] {
                continue;
            }
            visited[node] = true;
            component.push(node);

            for neighbor in &graph.neighbors[node] {
                if !visited[neighbor.index] {
                    stack.push(neighbor.index);
                }
            }
        }

        components.push(component);
    }

    components
}

pub fn assign_grid_coordinates(graph: &GridGraph, component: &[usize]) -> Vec<(usize, i32, i32)> {
    let mut coords = Vec::new();
    let mut visited = vec![false; graph.neighbors.len()];
    let mut queue = std::collections::VecDeque::new();

    // Start from the first node in the component.
    let start = component[0];
    queue.push_back((start, 0, 0)); // (node index, i, j)

    while let Some((node_idx, i, j)) = queue.pop_front() {
        if visited[node_idx] {
            continue;
        }
        visited[node_idx] = true;
        coords.push((node_idx, i, j));

        for neighbor in &graph.neighbors[node_idx] {
            let (di, dj) = match neighbor.direction {
                NeighborDirection::Right => (1, 0),
                NeighborDirection::Left => (-1, 0),
                NeighborDirection::Up => (0, -1),
                NeighborDirection::Down => (0, 1),
            };
            let neighbor_i = i + di;
            let neighbor_j = j + dj;
            queue.push_back((neighbor.index, neighbor_i, neighbor_j));
        }
    }

    coords
}

impl GridGraph {
    pub fn new(
        corners: &[Corner],
        params: GridGraphParams,
        grid_diagonals: Option<[f32; 2]>,
    ) -> Self {
        let coords = corners
            .iter()
            .map(|c| [c.position.x, c.position.y])
            .collect::<Vec<_>>();
        let tree: KdTree<f32, 2> = (&coords).into();
        let mut neighbors = Vec::with_capacity(corners.len());

        for (i, corner) in corners.iter().enumerate() {
            let mut node_neighbors = Vec::new();

            let query_point = [corner.position.x, corner.position.y];
            let results = tree.nearest_n::<SquaredEuclidean>(&query_point, params.k_neighbors);

            for nn in results.into_iter() {
                let neighbor_index = nn.item as usize;
                if neighbor_index == i {
                    continue; // Skip self
                }

                let neighbor = &corners[neighbor_index];
                if let Some(grid_diags) = grid_diagonals {
                    if let Some(nn_entry) = is_good_neighbor_with_orientation(
                        corner,
                        neighbor,
                        neighbor_index,
                        &params,
                        grid_diags,
                    ) {
                        node_neighbors.push(nn_entry);
                    }
                } else if let Some(nn_entry) =
                    is_good_neighbor(corner, neighbor, neighbor_index, &params)
                {
                    node_neighbors.push(nn_entry);
                }
            }

            neighbors.push(select_neighbors(node_neighbors));
        }

        Self { neighbors }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use calib_targets_core::Corner;
    use nalgebra::Point2;
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
        let graph = GridGraph::new(&corners, params, None);

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
        let graph = GridGraph::new(&corners, params, None);

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
        let graph = GridGraph::new(&corners, params, None);

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

    /// Grid rotated ~40° from image axes. With old image-axis direction_quadrant,
    /// this would cause inconsistent labels and fragmentation. With canonical-axis
    /// classification, all corners should form a single connected component with
    /// correct BFS coordinates.
    #[test]
    fn rotated_grid_forms_single_component() {
        let spacing = 20.0;
        let angle = 40.0f32.to_radians(); // grid rotation from image x-axis
        let cols = 4;
        let rows = 4;

        // Grid axes in image space
        let ax = Vector2::new(angle.cos(), angle.sin());
        let ay = Vector2::new(-angle.sin(), angle.cos());

        // Diagonal orientations: 45° offset from grid axes
        let diag0 = angle + FRAC_PI_4; // cluster 0
        let diag1 = angle + 3.0 * FRAC_PI_4; // cluster 1

        let grid_diagonals = [diag0, diag1];

        let mut corners = Vec::new();
        for j in 0..rows {
            for i in 0..cols {
                let pos = ax * (i as f32 * spacing) + ay * (j as f32 * spacing);
                let cluster = (i + j) % 2; // alternating clusters
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
        let graph = GridGraph::new(&corners, params, Some(grid_diagonals));

        let components = connected_components(&graph);
        assert_eq!(
            1,
            components.len(),
            "Rotated grid should form a single connected component, got {}",
            components.len()
        );
        assert_eq!(cols * rows, components[0].len());

        // Verify BFS assigns unique (i,j) to every corner
        let coords = assign_grid_coordinates(&graph, &components[0]);
        assert_eq!(cols * rows, coords.len());
        let coord_set: std::collections::HashSet<(i32, i32)> =
            coords.iter().map(|&(_, i, j)| (i, j)).collect();
        assert_eq!(
            cols * rows,
            coord_set.len(),
            "All grid coords must be unique"
        );
    }

    /// Verify that for every edge A->B with direction D, B->A has the opposite direction.
    #[test]
    fn direction_symmetry_on_rotated_grid() {
        let spacing = 20.0;
        let angle = 55.0f32.to_radians(); // arbitrary rotation
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
        let graph = GridGraph::new(&corners, params, Some(grid_diagonals));

        let opposite = |d: &NeighborDirection| match d {
            NeighborDirection::Right => NeighborDirection::Left,
            NeighborDirection::Left => NeighborDirection::Right,
            NeighborDirection::Up => NeighborDirection::Down,
            NeighborDirection::Down => NeighborDirection::Up,
        };

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
                    opposite(&n.direction),
                    back.unwrap().direction,
                    "Edge {a}->{b} is {:?} but {b}->{a} is {:?}, expected {:?}",
                    n.direction,
                    back.unwrap().direction,
                    opposite(&n.direction),
                );
            }
        }
    }

    /// Grid at exactly 45° to image axes — maximum ambiguity for image-axis classification.
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
        let graph = GridGraph::new(&corners, params, Some(grid_diagonals));

        let components = connected_components(&graph);
        assert_eq!(1, components.len());
        assert_eq!(25, components[0].len());
    }

    #[test]
    fn keeps_best_candidate_per_direction() {
        let spacing = 10.0;
        let worse_spacing = 12.0;

        // Center at origin; two right candidates with slightly different orientation,
        // and a left candidate to ensure other directions remain intact.
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
        let graph = GridGraph::new(&corners, params, None);

        let map = neighbor_map(&graph.neighbors[0]);
        assert_eq!(2, map.len()); // left + right only
        assert_eq!(1, map[&NeighborDirection::Right].index); // best right chosen
        assert_eq!(3, map[&NeighborDirection::Left].index);
    }
}
