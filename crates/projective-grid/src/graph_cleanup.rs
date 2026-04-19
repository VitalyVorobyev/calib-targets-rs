//! Post-build geometric sanity passes on a [`GridGraph`].
//!
//! A freshly-built 4-connected grid graph can contain edges that pass every
//! per-edge validator in isolation yet violate graph-global invariants of a
//! true chessboard lattice:
//!
//! 1. **Asymmetry.** The per-direction "best candidate" selection at each
//!    node can leave `A.Right = B` without `B.Left = A` (B found a better
//!    Left candidate elsewhere).
//! 2. **Non-straight axis chains.** At a true grid node `C`, the vectors
//!    `C→Right` and `C→Left` must be nearly antiparallel (and similarly
//!    `Up`/`Down`). A false edge inserted on one side bends the chain.
//! 3. **Edge crossings.** Edges of a planar grid never cross except at
//!    shared endpoints. A spurious cross-cell edge produces a topological
//!    X with another real edge.
//!
//! This module cleans the graph after construction, **before** connected
//! components are computed. Each pass takes an `&mut GridGraph` and removes
//! offending edges in place. All passes are pure geometry — none need the
//! pattern-specific `PointData` that the original validators used.

use crate::graph::GridGraph;
use crate::Float;
use crate::NeighborDirection;
use nalgebra::Point2;

/// Remove every directed edge `A→B` whose reverse `B→A` is missing.
///
/// Returns the number of edges removed. Each undirected edge counts once
/// (if an edge was asymmetric, removing its single present direction is
/// one removal).
pub fn enforce_symmetry<F: Float>(graph: &mut GridGraph<F>) -> usize {
    let n = graph.neighbors.len();
    // Mark which (src, dst) directed edges are present.
    let present: Vec<Vec<usize>> = graph
        .neighbors
        .iter()
        .map(|ns| ns.iter().map(|n| n.index).collect())
        .collect();

    let has_reverse = |a: usize, b: usize| -> bool { present[b].contains(&a) };

    let mut removed = 0usize;
    for a in 0..n {
        let before = graph.neighbors[a].len();
        graph.neighbors[a].retain(|n| has_reverse(a, n.index));
        removed += before - graph.neighbors[a].len();
    }
    removed
}

/// Drop edges at each node whose Right/Left or Up/Down pair fails the
/// straight-line test.
///
/// At a true grid node `C`, the vectors `C→Right` and `C→Left` should
/// have opposite direction (angle ≈ π between them). We measure the
/// deviation as `deg(angle_between(v_R, v_L)) - 180°` and drop the
/// worse-scoring edge (higher `score`) when the deviation exceeds
/// `max_deviation_deg`. Same for `C→Up` and `C→Down`.
///
/// Edges removed here are NOT symmetry-enforced — the caller should run
/// [`enforce_symmetry`] afterwards to kill the now-dangling reverse edges.
///
/// Returns the number of directed edges removed.
pub fn prune_by_edge_straightness<F: Float>(
    graph: &mut GridGraph<F>,
    positions: &[Point2<F>],
    max_deviation_deg: F,
) -> usize {
    assert_eq!(
        positions.len(),
        graph.neighbors.len(),
        "positions and graph node count must match"
    );

    // Antiparallel pair has cos(angle) = -1. Allowable deviation is
    // `max_deviation_deg`: reject if cos(angle) > cos(π - max_dev) = -cos(max_dev).
    let deg_to_rad: F = F::pi() / F::from_subset(&180.0);
    let max_dev_rad: F = max_deviation_deg * deg_to_rad;
    let min_cos_antiparallel: F = -(max_dev_rad.cos());

    let mut removed = 0usize;
    let mut to_drop: Vec<(usize, usize)> = Vec::new();

    for node in 0..graph.neighbors.len() {
        let pos_c = positions[node];
        let ns = &graph.neighbors[node];
        // For each opposing pair (Right, Left) and (Up, Down):
        for pair in [
            (NeighborDirection::Right, NeighborDirection::Left),
            (NeighborDirection::Up, NeighborDirection::Down),
        ] {
            let a = ns.iter().find(|n| n.direction == pair.0);
            let b = ns.iter().find(|n| n.direction == pair.1);
            let (Some(a), Some(b)) = (a, b) else { continue };
            let pa = positions[a.index];
            let pb = positions[b.index];
            let va = pa - pos_c;
            let vb = pb - pos_c;
            let na = va.norm();
            let nb = vb.norm();
            if na <= F::default_epsilon() || nb <= F::default_epsilon() {
                continue;
            }
            let cos_angle = va.dot(&vb) / (na * nb);
            // Antiparallel: cos ≈ -1. Bent: cos > -cos(max_dev).
            if cos_angle > min_cos_antiparallel {
                // Drop the worse-scoring one (ties broken by larger distance).
                let drop_idx =
                    if a.score > b.score || (a.score == b.score && a.distance > b.distance) {
                        a.index
                    } else {
                        b.index
                    };
                to_drop.push((node, drop_idx));
            }
        }
    }

    for (src, dst) in to_drop {
        let before = graph.neighbors[src].len();
        graph.neighbors[src].retain(|n| n.index != dst);
        removed += before - graph.neighbors[src].len();
    }
    removed
}

/// Drop edges that properly cross another edge in the graph.
///
/// "Properly cross" = the two segments share a strictly interior
/// intersection point. Shared endpoints (edges meeting at a common
/// corner) do not count as a crossing.
///
/// For each crossing pair, we drop the edge with the higher combined
/// score (both directions of the undirected edge, whichever are present).
///
/// O(E²) in the graph edge count. Our frames have E ≤ 400 so this is
/// ≲ 160k segment tests — fine.
///
/// Returns the number of directed edges removed.
pub fn prune_crossing_edges<F: Float>(graph: &mut GridGraph<F>, positions: &[Point2<F>]) -> usize {
    assert_eq!(
        positions.len(),
        graph.neighbors.len(),
        "positions and graph node count must match"
    );

    // Gather undirected edges: one representative (src,dst) per pair.
    // Each undirected edge is represented by the pair (min, max).
    #[derive(Clone)]
    struct UndirectedEdge<F: Float> {
        a: usize,
        b: usize,
        score: F,
    }

    let mut seen: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();
    let mut edges: Vec<UndirectedEdge<F>> = Vec::new();
    for (src, ns) in graph.neighbors.iter().enumerate() {
        for n in ns {
            let (a, b) = if src < n.index {
                (src, n.index)
            } else {
                (n.index, src)
            };
            if seen.insert((a, b)) {
                // Compute combined score if both directions present; else use the single.
                let rev_score = graph.neighbors[n.index]
                    .iter()
                    .find(|rn| rn.index == src)
                    .map(|rn| rn.score);
                let combined_score = rev_score.map(|s| s + n.score).unwrap_or(n.score);
                edges.push(UndirectedEdge {
                    a,
                    b,
                    score: combined_score,
                });
            }
        }
    }

    // Find crossings.
    let mut drop_set: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();
    for i in 0..edges.len() {
        if drop_set.contains(&(edges[i].a, edges[i].b)) {
            continue;
        }
        for j in (i + 1)..edges.len() {
            if drop_set.contains(&(edges[j].a, edges[j].b)) {
                continue;
            }
            let (p1, p2) = (positions[edges[i].a], positions[edges[i].b]);
            let (p3, p4) = (positions[edges[j].a], positions[edges[j].b]);
            if segments_properly_cross(p1, p2, p3, p4) {
                // Drop the worse (higher score) edge.
                let drop_idx = if edges[i].score >= edges[j].score {
                    i
                } else {
                    j
                };
                drop_set.insert((edges[drop_idx].a, edges[drop_idx].b));
            }
        }
    }

    let mut removed = 0usize;
    for (a, b) in drop_set {
        let before_a = graph.neighbors[a].len();
        graph.neighbors[a].retain(|n| n.index != b);
        removed += before_a - graph.neighbors[a].len();
        let before_b = graph.neighbors[b].len();
        graph.neighbors[b].retain(|n| n.index != a);
        removed += before_b - graph.neighbors[b].len();
    }
    removed
}

/// Drop low-degree dangling edges.
///
/// After symmetry + straightness + planarity, it's safe to drop any
/// directed edge `A→B` where node `A` has only that one neighbor and
/// `B` also has very few neighbors — such "pendant" edges rarely
/// correspond to genuine grid connectivity and their survival produces
/// tiny chaff components that the downstream component-size filter
/// would drop anyway. Returns the number of edges removed.
///
/// `min_node_degree` sets the floor; edges from nodes whose degree is
/// below this floor are candidates for removal, but we only drop the
/// edge when BOTH endpoints are at the floor — that's the "isolated
/// pair" case.
pub fn prune_isolated_pairs<F: Float>(graph: &mut GridGraph<F>, min_node_degree: usize) -> usize {
    let degrees: Vec<usize> = graph.neighbors.iter().map(|ns| ns.len()).collect();
    let mut removed = 0usize;
    let mut to_drop: Vec<(usize, usize)> = Vec::new();
    for (a, ns) in graph.neighbors.iter().enumerate() {
        if degrees[a] > min_node_degree {
            continue;
        }
        for n in ns {
            if degrees[n.index] <= min_node_degree {
                to_drop.push((a, n.index));
            }
        }
    }
    for (a, b) in to_drop {
        let before = graph.neighbors[a].len();
        graph.neighbors[a].retain(|n| n.index != b);
        removed += before - graph.neighbors[a].len();
    }
    removed
}

// --- Segment intersection --------------------------------------------------

fn cross2<F: Float>(a: nalgebra::Vector2<F>, b: nalgebra::Vector2<F>) -> F {
    a.x * b.y - a.y * b.x
}

/// Return `true` iff segments `(p1,p2)` and `(p3,p4)` share a strictly
/// interior intersection point. Shared endpoints (within a small
/// tolerance) do **not** count as a crossing — they are the legitimate
/// meeting points of a planar graph.
pub fn segments_properly_cross<F: Float>(
    p1: Point2<F>,
    p2: Point2<F>,
    p3: Point2<F>,
    p4: Point2<F>,
) -> bool {
    let eps = F::from_subset(&1e-6);
    let same = |a: Point2<F>, b: Point2<F>| -> bool {
        (a.x - b.x).abs() <= eps && (a.y - b.y).abs() <= eps
    };
    if same(p1, p3) || same(p1, p4) || same(p2, p3) || same(p2, p4) {
        return false;
    }
    let d12 = p2 - p1;
    let d34 = p4 - p3;

    let d1 = cross2::<F>(d34, p1 - p3);
    let d2 = cross2::<F>(d34, p2 - p3);
    let d3 = cross2::<F>(d12, p3 - p1);
    let d4 = cross2::<F>(d12, p4 - p1);

    let zero: F = F::zero();
    ((d1 > zero) != (d2 > zero)) && ((d3 > zero) != (d4 > zero)) && d1 != zero && d2 != zero
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodeNeighbor;

    fn make_node<F: Float>(
        direction: NeighborDirection,
        index: usize,
        distance: F,
        score: F,
    ) -> NodeNeighbor<F> {
        NodeNeighbor {
            direction,
            index,
            distance,
            score,
        }
    }

    #[test]
    fn symmetry_drops_one_sided_edges() {
        // 3 nodes in a row. A has right=B, B has left=A (good), but A has
        // another "Left=C" that C does not reciprocate.
        let _positions = [
            Point2::new(10.0f32, 0.0),
            Point2::new(20.0, 0.0),
            Point2::new(0.0, 0.0),
        ];
        let mut graph = GridGraph::<f32> {
            neighbors: vec![
                vec![
                    make_node(NeighborDirection::Right, 1, 10.0, 0.1),
                    make_node(NeighborDirection::Left, 2, 10.0, 0.2),
                ],
                vec![make_node(NeighborDirection::Left, 0, 10.0, 0.1)],
                vec![], // C has no reverse to A
            ],
        };
        let removed = enforce_symmetry(&mut graph);
        assert_eq!(removed, 1);
        assert_eq!(graph.neighbors[0].len(), 1);
    }

    #[test]
    fn planarity_drops_the_crossing_edge() {
        // 4 nodes at the vertices of a unit square, with diagonal edges.
        let positions = vec![
            Point2::new(0.0f32, 0.0), // 0: TL
            Point2::new(1.0, 0.0),    // 1: TR
            Point2::new(1.0, 1.0),    // 2: BR
            Point2::new(0.0, 1.0),    // 3: BL
        ];
        // Edges: diagonal 0-2 (score 1.0), diagonal 1-3 (score 0.5). Also
        // side 0-1 to ensure side edges are not dropped.
        let mut graph = GridGraph::<f32> {
            neighbors: vec![
                vec![
                    make_node(NeighborDirection::Right, 1, 1.0, 0.1),
                    make_node(NeighborDirection::Down, 2, 1.41, 1.0),
                ],
                vec![
                    make_node(NeighborDirection::Left, 0, 1.0, 0.1),
                    make_node(NeighborDirection::Down, 3, 1.41, 0.5),
                ],
                vec![make_node(NeighborDirection::Up, 0, 1.41, 1.0)],
                vec![make_node(NeighborDirection::Up, 1, 1.41, 0.5)],
            ],
        };
        let removed = prune_crossing_edges(&mut graph, &positions);
        // The worse diagonal (0–2, score 1.0) should be dropped.
        assert!(removed >= 2, "expected at least 2 directed edges removed");
        assert!(!graph.neighbors[0].iter().any(|n| n.index == 2));
        assert!(!graph.neighbors[2].iter().any(|n| n.index == 0));
        // Better diagonal should remain.
        assert!(graph.neighbors[1].iter().any(|n| n.index == 3));
        assert!(graph.neighbors[3].iter().any(|n| n.index == 1));
    }

    #[test]
    fn straightness_drops_bent_pair() {
        // C at origin. Right is at (10, 0). A false "Left" at (-3, 10)
        // — the angle between (10,0) and (-3,10) is about 180° − 72° =
        // 108°, way below the 15° straightness tolerance.
        let positions = vec![
            Point2::new(0.0f32, 0.0), // 0: C
            Point2::new(10.0, 0.0),   // 1: Right
            Point2::new(-3.0, 10.0),  // 2: "Left" (bent)
        ];
        let mut graph = GridGraph::<f32> {
            neighbors: vec![
                vec![
                    make_node(NeighborDirection::Right, 1, 10.0, 0.1),
                    make_node(NeighborDirection::Left, 2, 10.44, 0.9), // worse score
                ],
                vec![make_node(NeighborDirection::Left, 0, 10.0, 0.1)],
                vec![make_node(NeighborDirection::Right, 0, 10.44, 0.9)],
            ],
        };
        let removed = prune_by_edge_straightness(&mut graph, &positions, 15.0);
        assert_eq!(removed, 1);
        // The bent Left (idx 2) should have been removed from node 0.
        assert!(!graph.neighbors[0].iter().any(|n| n.index == 2));
        // Reverse is still present until enforce_symmetry runs.
        assert!(graph.neighbors[2].iter().any(|n| n.index == 0));
    }

    #[test]
    fn straightness_keeps_colinear_pair() {
        let positions = vec![
            Point2::new(0.0f32, 0.0),
            Point2::new(10.0, 0.0),
            Point2::new(-10.0, 0.0),
        ];
        let mut graph = GridGraph::<f32> {
            neighbors: vec![
                vec![
                    make_node(NeighborDirection::Right, 1, 10.0, 0.1),
                    make_node(NeighborDirection::Left, 2, 10.0, 0.1),
                ],
                vec![make_node(NeighborDirection::Left, 0, 10.0, 0.1)],
                vec![make_node(NeighborDirection::Right, 0, 10.0, 0.1)],
            ],
        };
        let removed = prune_by_edge_straightness(&mut graph, &positions, 15.0);
        assert_eq!(removed, 0);
        assert_eq!(graph.neighbors[0].len(), 2);
    }

    #[test]
    fn segments_proper_cross_test() {
        // Cross: X shape.
        assert!(segments_properly_cross(
            Point2::new(0.0f32, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(0.0, 1.0),
            Point2::new(1.0, 0.0),
        ));
        // Shared endpoint — NOT a crossing.
        assert!(!segments_properly_cross(
            Point2::new(0.0f32, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(0.0, 0.0),
            Point2::new(0.0, 1.0),
        ));
        // Parallel, no intersection.
        assert!(!segments_properly_cross(
            Point2::new(0.0f32, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(0.0, 1.0),
            Point2::new(1.0, 1.0),
        ));
    }

    #[test]
    fn isolated_pair_prune_drops_dangling_pair() {
        // 4 nodes: 0—1 is a pair; 2—3 is a pair. No edges between.
        let mut graph = GridGraph::<f32> {
            neighbors: vec![
                vec![make_node(NeighborDirection::Right, 1, 10.0, 0.1)],
                vec![make_node(NeighborDirection::Left, 0, 10.0, 0.1)],
                vec![make_node(NeighborDirection::Right, 3, 10.0, 0.1)],
                vec![make_node(NeighborDirection::Left, 2, 10.0, 0.1)],
            ],
        };
        // min_node_degree=1 => any node with ≤1 neighbor connected to another
        // ≤1-neighbor node is dropped.
        let removed = prune_isolated_pairs(&mut graph, 1);
        assert_eq!(removed, 4);
        assert!(graph.neighbors.iter().all(|ns| ns.is_empty()));
    }
}
