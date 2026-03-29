//! 6-connected hex grid graph construction via KD-tree spatial search.

use crate::graph::{GridGraphParams, NeighborCandidate};
use crate::hex::direction::{HexDirection, HexNodeNeighbor};
use kiddo::{KdTree, SquaredEuclidean};
use nalgebra::{Point2, Vector2};

/// Extension point for hex-pattern-specific neighbor validation.
///
/// Implementors decide whether a spatially close point is a valid hex grid
/// neighbor, and if so, assign it a direction and quality score.
pub trait HexNeighborValidator {
    /// Per-point data beyond position (e.g., orientation angle).
    /// Use `()` if no extra data is needed.
    type PointData;

    /// Validate whether `candidate` is a valid hex grid neighbor of the point
    /// at `source_index`. Returns `(direction, score)` where lower score
    /// is better, or `None` to reject.
    fn validate(
        &self,
        source_index: usize,
        source_data: &Self::PointData,
        candidate: &NeighborCandidate,
        candidate_data: &Self::PointData,
    ) -> Option<(HexDirection, f32)>;
}

/// A 6-connected hex grid graph over 2D points.
///
/// Each node has at most one neighbor per hex direction,
/// selected as the best-scoring candidate from spatial proximity search.
pub struct HexGridGraph {
    /// Per-node adjacency list. `neighbors[i]` contains up to 6 validated neighbors.
    pub neighbors: Vec<Vec<HexNodeNeighbor>>,
}

impl HexGridGraph {
    /// Build a hex grid graph from 2D points using a caller-supplied validator.
    ///
    /// - `positions`: 2D point positions for spatial search.
    /// - `point_data`: per-point data passed to the validator (same length as `positions`).
    /// - `validator`: determines which spatial neighbors are valid hex grid neighbors.
    /// - `params`: controls KD-tree search parameters.
    pub fn build<V: HexNeighborValidator>(
        positions: &[Point2<f32>],
        point_data: &[V::PointData],
        validator: &V,
        params: &GridGraphParams,
    ) -> Self {
        assert_eq!(
            positions.len(),
            point_data.len(),
            "positions and point_data must have the same length"
        );

        let coords: Vec<[f32; 2]> = positions.iter().map(|p| [p.x, p.y]).collect();
        let tree: KdTree<f32, 2> = (&coords).into();
        let max_dist_sq = params.max_distance * params.max_distance;

        let mut neighbors = Vec::with_capacity(positions.len());

        for (i, pos) in positions.iter().enumerate() {
            let query = [pos.x, pos.y];
            let results = tree.nearest_n::<SquaredEuclidean>(&query, params.k_neighbors);

            let mut candidates = Vec::new();

            for nn in results {
                let j = nn.item as usize;
                if j == i {
                    continue;
                }

                let dist_sq = nn.distance;
                if dist_sq > max_dist_sq {
                    continue;
                }

                let neighbor_pos = positions[j];
                let offset = Vector2::new(neighbor_pos.x - pos.x, neighbor_pos.y - pos.y);
                let distance = dist_sq.sqrt();

                let candidate = NeighborCandidate {
                    index: j,
                    offset,
                    distance,
                };

                if let Some((direction, score)) =
                    validator.validate(i, &point_data[i], &candidate, &point_data[j])
                {
                    candidates.push(HexNodeNeighbor {
                        direction,
                        index: j,
                        distance,
                        score,
                    });
                }
            }

            neighbors.push(select_hex_neighbors(candidates));
        }

        Self { neighbors }
    }
}

/// Keep at most one neighbor per direction, choosing the lowest-score candidate.
fn select_hex_neighbors(candidates: Vec<HexNodeNeighbor>) -> Vec<HexNodeNeighbor> {
    let mut best: [Option<HexNodeNeighbor>; 6] = [None, None, None, None, None, None];

    for candidate in candidates {
        let slot = &mut best[candidate.direction.slot_index()];

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

#[cfg(test)]
mod tests {
    use super::*;

    /// Trivial validator that assigns direction by sextant angle.
    struct AngleValidator;

    impl HexNeighborValidator for AngleValidator {
        type PointData = ();

        fn validate(
            &self,
            _source_index: usize,
            _source_data: &(),
            candidate: &NeighborCandidate,
            _candidate_data: &(),
        ) -> Option<(HexDirection, f32)> {
            let angle = candidate.offset.y.atan2(candidate.offset.x);
            let deg = angle.to_degrees();

            // Map angle to nearest hex direction (pointy-top, 60° sectors)
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

            Some((dir, candidate.distance))
        }
    }

    /// Generate a regular hex lattice (pointy-top) with given radius.
    fn hex_lattice(radius: i32, spacing: f32) -> Vec<Point2<f32>> {
        let mut points = Vec::new();
        let sqrt3 = 3.0f32.sqrt();
        for q in -radius..=radius {
            for r in -radius..=radius {
                if (q + r).abs() > radius {
                    continue;
                }
                let x = spacing * (q as f32 + r as f32 * 0.5);
                let y = spacing * (r as f32 * sqrt3 / 2.0);
                points.push(Point2::new(x, y));
            }
        }
        points
    }

    #[test]
    fn center_node_has_six_neighbors() {
        let spacing = 50.0;
        let points = hex_lattice(2, spacing);
        let data = vec![(); points.len()];

        let params = GridGraphParams {
            k_neighbors: 12,
            max_distance: spacing * 1.5,
        };

        let graph = HexGridGraph::build(&points, &data, &AngleValidator, &params);

        // Find the center node (0, 0) -> (x=0, y=0)
        let center = points
            .iter()
            .position(|p| p.x.abs() < 0.01 && p.y.abs() < 0.01)
            .unwrap();

        assert_eq!(graph.neighbors[center].len(), 6);
    }

    #[test]
    fn edge_nodes_have_fewer_neighbors() {
        let spacing = 50.0;
        let points = hex_lattice(1, spacing);
        let data = vec![(); points.len()];

        let params = GridGraphParams {
            k_neighbors: 12,
            max_distance: spacing * 1.5,
        };

        let graph = HexGridGraph::build(&points, &data, &AngleValidator, &params);

        // All non-center nodes in radius-1 hex have exactly 3 neighbors
        for (i, p) in points.iter().enumerate() {
            if p.x.abs() < 0.01 && p.y.abs() < 0.01 {
                assert_eq!(graph.neighbors[i].len(), 6);
            } else {
                assert_eq!(
                    graph.neighbors[i].len(),
                    3,
                    "edge node {i} at ({}, {}) has {} neighbors",
                    p.x,
                    p.y,
                    graph.neighbors[i].len()
                );
            }
        }
    }

    #[test]
    fn select_keeps_best_per_direction() {
        let candidates = vec![
            HexNodeNeighbor {
                direction: HexDirection::East,
                index: 1,
                distance: 50.0,
                score: 0.9,
            },
            HexNodeNeighbor {
                direction: HexDirection::East,
                index: 2,
                distance: 55.0,
                score: 0.5,
            },
            HexNodeNeighbor {
                direction: HexDirection::West,
                index: 3,
                distance: 50.0,
                score: 0.3,
            },
        ];

        let selected = select_hex_neighbors(candidates);
        assert_eq!(selected.len(), 2);

        let east = selected
            .iter()
            .find(|n| n.direction == HexDirection::East)
            .unwrap();
        assert_eq!(east.index, 2); // lower score wins
    }
}
