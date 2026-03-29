use crate::direction::NeighborDirection;
use crate::graph::GridGraph;
use crate::grid_index::GridIndex;

/// Find connected components in the grid graph.
///
/// Returns a list of components, each being a list of node indices.
/// Components are found via iterative DFS.
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

/// Assign integer grid coordinates to nodes in a connected component via BFS.
///
/// Starts from the first node in `component` at `(0, 0)` and propagates
/// coordinates using neighbor directions.
///
/// Returns `(node_index, GridIndex)` for each reachable node.
pub fn assign_grid_coordinates(graph: &GridGraph, component: &[usize]) -> Vec<(usize, GridIndex)> {
    let mut coords = Vec::new();
    let mut visited = vec![false; graph.neighbors.len()];
    let mut queue = std::collections::VecDeque::new();

    let start = component[0];
    queue.push_back((start, 0i32, 0i32));

    while let Some((node_idx, i, j)) = queue.pop_front() {
        if visited[node_idx] {
            continue;
        }
        visited[node_idx] = true;
        coords.push((node_idx, GridIndex { i, j }));

        for neighbor in &graph.neighbors[node_idx] {
            let (di, dj) = match neighbor.direction {
                NeighborDirection::Right => (1, 0),
                NeighborDirection::Left => (-1, 0),
                NeighborDirection::Up => (0, -1),
                NeighborDirection::Down => (0, 1),
            };
            queue.push_back((neighbor.index, i + di, j + dj));
        }
    }

    coords
}
