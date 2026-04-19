use crate::Float;
use crate::{NeighborDirection, NodeNeighbor};
use kiddo::{KdTree, SquaredEuclidean};
use nalgebra::{Point2, Vector2};

/// A spatially close candidate found by KD-tree search.
pub struct NeighborCandidate<F: Float = f32> {
    /// Index of the candidate in the input point array.
    pub index: usize,
    /// Vector from source point to this candidate.
    pub offset: Vector2<F>,
    /// Euclidean distance.
    pub distance: F,
}

/// Extension point for pattern-specific neighbor validation.
///
/// Implementors decide whether a spatially close point is a valid
/// grid neighbor, and if so, assign it a direction and quality score.
pub trait NeighborValidator<F: Float = f32> {
    /// Per-point data beyond position (e.g., orientation, cluster label).
    /// Use `()` if no extra data is needed.
    type PointData;

    /// Validate whether `candidate` is a valid grid neighbor of the point
    /// at `source_index`. Returns `(direction, score)` where lower score
    /// is better, or `None` to reject.
    fn validate(
        &self,
        source_index: usize,
        source_data: &Self::PointData,
        candidate: &NeighborCandidate<F>,
        candidate_data: &Self::PointData,
    ) -> Option<(NeighborDirection, F)>;
}

/// Parameters for grid graph construction.
#[derive(Clone, Debug)]
pub struct GridGraphParams<F: Float = f32> {
    /// Number of nearest neighbors to query from the KD-tree.
    pub k_neighbors: usize,
    /// Maximum distance (pixels) for the KD-tree pre-filter.
    pub max_distance: F,
}

impl<F: Float> Default for GridGraphParams<F> {
    fn default() -> Self {
        Self {
            k_neighbors: 8,
            max_distance: F::max_value().unwrap_or_else(|| F::from_subset(&1e30)),
        }
    }
}

/// A 4-connected grid graph over 2D points.
///
/// Each node has at most one neighbor per cardinal direction (Right, Left, Up, Down),
/// selected as the best-scoring candidate from spatial proximity search.
pub struct GridGraph<F: Float = f32> {
    /// Per-node adjacency list. `neighbors[i]` contains up to 4 validated neighbors
    /// of the point at index `i`.
    pub neighbors: Vec<Vec<NodeNeighbor<F>>>,
}

impl<F: Float + kiddo::float::kdtree::Axis> GridGraph<F> {
    /// Build a grid graph from 2D points using a caller-supplied validator.
    ///
    /// - `positions`: 2D point positions for spatial search.
    /// - `point_data`: per-point data passed to the validator (same length as `positions`).
    /// - `validator`: determines which spatial neighbors are valid grid neighbors.
    /// - `params`: controls KD-tree search parameters.
    pub fn build<V: NeighborValidator<F>>(
        positions: &[Point2<F>],
        point_data: &[V::PointData],
        validator: &V,
        params: &GridGraphParams<F>,
    ) -> Self {
        assert_eq!(
            positions.len(),
            point_data.len(),
            "positions and point_data must have the same length"
        );

        let coords: Vec<[F; 2]> = positions.iter().map(|p| [p.x, p.y]).collect();
        let tree: KdTree<F, 2> = (&coords).into();
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
                    candidates.push(NodeNeighbor {
                        direction,
                        index: j,
                        distance,
                        score,
                    });
                }
            }

            neighbors.push(select_neighbors(candidates));
        }

        Self { neighbors }
    }
}

/// Keep at most one neighbor per direction, choosing the lowest-score candidate.
fn select_neighbors<F: Float>(candidates: Vec<NodeNeighbor<F>>) -> Vec<NodeNeighbor<F>> {
    let mut best: [Option<NodeNeighbor<F>>; 4] = [None, None, None, None];

    for candidate in candidates {
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
