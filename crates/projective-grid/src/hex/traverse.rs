//! Connected components and BFS coordinate assignment for hex grid graphs.

use crate::hex::graph::HexGridGraph;
use crate::Float;
use crate::GridIndex;

/// Find connected components in the hex grid graph.
///
/// Returns a list of components, each being a list of node indices.
/// Components are found via iterative DFS.
pub fn hex_connected_components<F: Float>(graph: &HexGridGraph<F>) -> Vec<Vec<usize>> {
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

/// Assign axial grid coordinates `(q, r)` to nodes in a connected component via BFS.
///
/// Starts from the first node in `component` at `(0, 0)` and propagates
/// coordinates using hex direction deltas.
///
/// Returns `(node_index, GridIndex)` for each reachable node, where
/// `GridIndex.i` is `q` and `GridIndex.j` is `r`.
pub fn hex_assign_grid_coordinates<F: Float>(
    graph: &HexGridGraph<F>,
    component: &[usize],
) -> Vec<(usize, GridIndex)> {
    let mut coords = Vec::new();
    let mut visited = vec![false; graph.neighbors.len()];
    let mut queue = std::collections::VecDeque::new();

    let start = component[0];
    queue.push_back((start, 0i32, 0i32));

    while let Some((node_idx, q, r)) = queue.pop_front() {
        if visited[node_idx] {
            continue;
        }
        visited[node_idx] = true;
        coords.push((node_idx, GridIndex { i: q, j: r }));

        for neighbor in &graph.neighbors[node_idx] {
            let (dq, dr) = neighbor.direction.delta();
            queue.push_back((neighbor.index, q + dq, r + dr));
        }
    }

    coords
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{GridGraphParams, NeighborCandidate};
    use crate::hex::direction::HexDirection;
    use crate::hex::graph::{HexGridGraph, HexNeighborValidator};
    use nalgebra::Point2;
    use std::collections::HashMap;

    struct AngleValidator;

    impl HexNeighborValidator for AngleValidator {
        type PointData = ();
        fn validate(
            &self,
            _si: usize,
            _sd: &(),
            c: &NeighborCandidate,
            _cd: &(),
        ) -> Option<(HexDirection, f32)> {
            let deg = c.offset.y.atan2(c.offset.x).to_degrees();
            let dir = if (-30.0..30.0).contains(&deg) {
                HexDirection::East
            } else if (30.0..90.0).contains(&deg) {
                HexDirection::SouthEast
            } else if (90.0..150.0).contains(&deg) {
                HexDirection::SouthWest
            } else if !(-150.0..150.0).contains(&deg) {
                HexDirection::West
            } else if (-150.0..-90.0).contains(&deg) {
                HexDirection::NorthWest
            } else {
                HexDirection::NorthEast
            };
            Some((dir, c.distance))
        }
    }

    type HexLattice = (Vec<Point2<f32>>, HashMap<(i32, i32), usize>);

    fn hex_lattice(radius: i32, spacing: f32) -> HexLattice {
        let sqrt3 = 3.0f32.sqrt();
        let mut points = Vec::new();
        let mut idx_map = HashMap::new();
        for q in -radius..=radius {
            for r in -radius..=radius {
                if (q + r).abs() > radius {
                    continue;
                }
                let x = spacing * (q as f32 + r as f32 * 0.5);
                let y = spacing * (r as f32 * sqrt3 / 2.0);
                idx_map.insert((q, r), points.len());
                points.push(Point2::new(x, y));
            }
        }
        (points, idx_map)
    }

    #[test]
    fn single_component_for_connected_hex() {
        let (points, _) = hex_lattice(2, 50.0);
        let data = vec![(); points.len()];
        let params = GridGraphParams {
            k_neighbors: 12,
            max_distance: 75.0,
        };
        let graph = HexGridGraph::build(&points, &data, &AngleValidator, &params);
        let components = hex_connected_components(&graph);
        assert_eq!(components.len(), 1);
        assert_eq!(components[0].len(), points.len());
    }

    #[test]
    fn bfs_assigns_correct_axial_coordinates() {
        let spacing = 50.0;
        let (points, expected) = hex_lattice(2, spacing);
        let data = vec![(); points.len()];
        let params = GridGraphParams {
            k_neighbors: 12,
            max_distance: spacing * 1.5,
        };
        let graph = HexGridGraph::build(&points, &data, &AngleValidator, &params);
        let components = hex_connected_components(&graph);
        let coords = hex_assign_grid_coordinates(&graph, &components[0]);

        assert_eq!(coords.len(), points.len());

        // Find the BFS origin to determine the offset
        let (origin_idx, _) = coords[0];
        let origin_qr = expected
            .iter()
            .find(|(_, &idx)| idx == origin_idx)
            .map(|(&qr, _)| qr)
            .unwrap();

        // All assigned coordinates should differ from expected by the same offset
        let mut coord_map: HashMap<usize, (i32, i32)> = HashMap::new();
        for &(node_idx, ref gi) in &coords {
            coord_map.insert(node_idx, (gi.i, gi.j));
        }

        let offset_q = 0 - origin_qr.0;
        let offset_r = 0 - origin_qr.1;

        for (&(exp_q, exp_r), &node_idx) in &expected {
            let (got_q, got_r) = coord_map[&node_idx];
            assert_eq!(
                (got_q, got_r),
                (exp_q + offset_q, exp_r + offset_r),
                "coordinate mismatch for node {node_idx}"
            );
        }
    }

    #[test]
    fn disconnected_components_found() {
        // Two isolated points
        let points = vec![Point2::new(0.0, 0.0), Point2::new(1000.0, 1000.0)];
        let data = vec![(); 2];
        let params = GridGraphParams {
            k_neighbors: 4,
            max_distance: 100.0,
        };
        let graph = HexGridGraph::build(&points, &data, &AngleValidator, &params);
        let components = hex_connected_components(&graph);
        assert_eq!(components.len(), 2);
    }
}
