//! Ready-to-use [`NeighborValidator`] and [`HexNeighborValidator`] implementations
//! for common grid detection scenarios.
//!
//! | Validator | Grid | PointData | Use case |
//! |-----------|------|-----------|----------|
//! | [`XJunctionValidator`] | Square | `F` (orientation mod π) | ChESS corners, chessboard X-junctions |
//! | [`SpatialSquareValidator`] | Square | `()` | Unoriented features on a square lattice |
//! | [`SpatialHexValidator`] | Hex | `()` | Unoriented features on a hex lattice (ringgrid, etc.) |

use crate::direction::NeighborDirection;
use crate::float_helpers::{lit, rem_euclid, to_degrees};
use crate::graph::{NeighborCandidate, NeighborValidator};
use crate::hex::direction::HexDirection;
use crate::hex::graph::HexNeighborValidator;
use crate::Float;
use nalgebra::Vector2;

// ---------------------------------------------------------------------------
// Geometry helpers
// ---------------------------------------------------------------------------

/// Absolute angle difference in `[0, π]`.
fn angle_diff_abs<F: Float>(a: F, b: F) -> F {
    let two_pi: F = lit::<F>(2.0) * F::pi();
    let mut diff = rem_euclid(b - a, two_pi);
    if diff >= F::pi() {
        diff -= two_pi;
    }
    diff.abs()
}

/// Check whether two undirected axes (angles mod π) are approximately orthogonal.
fn is_orthogonal<F: Float>(a: F, b: F, tolerance: F) -> bool {
    let diff = angle_diff_abs(a, b);
    (F::frac_pi_2() - diff).abs() <= tolerance.abs()
}

/// Angle between an undirected axis `axis_angle` (mod π) and a directed vector
/// `vec_angle`. Returns a value in `[0, π/2]`.
fn axis_vec_diff<F: Float>(axis_angle: F, vec_angle: F) -> F {
    let two_pi: F = lit::<F>(2.0) * F::pi();
    let mut diff = rem_euclid(vec_angle - axis_angle, two_pi);
    if diff >= F::pi() {
        diff -= two_pi;
    }
    let diff_abs = diff.abs();
    diff_abs.min(F::pi() - diff_abs)
}

/// Classify an offset vector into one of 4 cardinal directions by quadrant.
fn direction_quadrant<F: Float>(offset: &Vector2<F>) -> NeighborDirection {
    if offset.x.abs() > offset.y.abs() {
        if offset.x >= F::zero() {
            NeighborDirection::Right
        } else {
            NeighborDirection::Left
        }
    } else if offset.y >= F::zero() {
        NeighborDirection::Down
    } else {
        NeighborDirection::Up
    }
}

/// Classify an offset vector into one of 6 hex directions by sextant (60° sectors).
fn direction_sextant<F: Float>(offset: &Vector2<F>) -> HexDirection {
    let deg = to_degrees(offset.y.atan2(offset.x));
    if deg >= lit(-30.0) && deg < lit(30.0) {
        HexDirection::East
    } else if deg >= lit(30.0) && deg < lit(90.0) {
        HexDirection::SouthEast
    } else if deg >= lit(90.0) && deg < lit(150.0) {
        HexDirection::SouthWest
    } else if deg < lit(-150.0) || deg >= lit(150.0) {
        HexDirection::West
    } else if deg >= lit(-150.0) && deg < lit(-90.0) {
        HexDirection::NorthWest
    } else {
        HexDirection::NorthEast
    }
}

// ---------------------------------------------------------------------------
// XJunctionValidator
// ---------------------------------------------------------------------------

/// Validator for **square grids** of X-junction corners with known orientation.
///
/// Designed for ChESS-like features where each corner has an orientation angle
/// (mod π) and adjacent corners on a chessboard pattern have orthogonal
/// orientations. The edge between two neighbors is at ~45° to both orientations.
///
/// # PointData
///
/// `F` — the corner's orientation angle in radians (mod π, undirected axis).
///
/// # Example
///
/// ```
/// use projective_grid::{GridGraph, GridGraphParams, NeighborCandidate};
/// use projective_grid::validators::XJunctionValidator;
/// use nalgebra::Point2;
/// use std::f32::consts::FRAC_PI_4;
///
/// let positions = vec![
///     Point2::new(0.0f32, 0.0), Point2::new(10.0, 0.0),
///     Point2::new(0.0, 10.0),   Point2::new(10.0, 10.0),
/// ];
/// // Alternating orientations (π/4 and 3π/4) like a chessboard
/// let orientations = vec![FRAC_PI_4, 3.0 * FRAC_PI_4, 3.0 * FRAC_PI_4, FRAC_PI_4];
///
/// let validator = XJunctionValidator {
///     min_spacing: 5.0,
///     max_spacing: 15.0,
///     tolerance_rad: 15.0f32.to_radians(),
/// };
/// let graph = GridGraph::build(
///     &positions, &orientations, &validator, &GridGraphParams::default(),
/// );
/// ```
pub struct XJunctionValidator<F: Float = f32> {
    /// Minimum acceptable neighbor distance (pixels).
    pub min_spacing: F,
    /// Maximum acceptable neighbor distance (pixels).
    pub max_spacing: F,
    /// Angular tolerance (radians) for orthogonality and 45° edge alignment checks.
    pub tolerance_rad: F,
}

impl<F: Float> NeighborValidator<F> for XJunctionValidator<F> {
    type PointData = F;

    fn validate(
        &self,
        _source_index: usize,
        source_data: &F,
        candidate: &NeighborCandidate<F>,
        candidate_data: &F,
    ) -> Option<(NeighborDirection, F)> {
        // 1. Orientations must be approximately orthogonal.
        if !is_orthogonal(*source_data, *candidate_data, self.tolerance_rad) {
            return None;
        }

        // 2. Distance within range.
        if candidate.distance < self.min_spacing || candidate.distance > self.max_spacing {
            return None;
        }

        // 3. Edge direction should be at ~45° to both corner orientations.
        let edge_angle = candidate.offset.y.atan2(candidate.offset.x);
        let expected = F::frac_pi_4();

        let score_src = (axis_vec_diff(*source_data, edge_angle) - expected).abs();
        let score_cand = (axis_vec_diff(*candidate_data, edge_angle) - expected).abs();

        if score_src > self.tolerance_rad || score_cand > self.tolerance_rad {
            return None;
        }

        // 4. Direction by image-space quadrant.
        let direction = direction_quadrant(&candidate.offset);

        // 5. Score: angular deviations + orientation orthogonality residual.
        let score_ortho = (F::frac_pi_2() - angle_diff_abs(*source_data, *candidate_data)).abs();
        let score = score_src + score_cand + score_ortho;

        Some((direction, score))
    }
}

// ---------------------------------------------------------------------------
// SpatialSquareValidator
// ---------------------------------------------------------------------------

/// Validator for **square grids** with no orientation information.
///
/// Classifies neighbors by image-space quadrant and filters by distance.
/// Suitable for any approximately regular square lattice of detected features.
///
/// # PointData
///
/// `()` — no per-point data needed.
pub struct SpatialSquareValidator<F: Float = f32> {
    /// Minimum acceptable neighbor distance (pixels).
    pub min_spacing: F,
    /// Maximum acceptable neighbor distance (pixels).
    pub max_spacing: F,
}

impl<F: Float> NeighborValidator<F> for SpatialSquareValidator<F> {
    type PointData = ();

    fn validate(
        &self,
        _source_index: usize,
        _source_data: &(),
        candidate: &NeighborCandidate<F>,
        _candidate_data: &(),
    ) -> Option<(NeighborDirection, F)> {
        if candidate.distance < self.min_spacing || candidate.distance > self.max_spacing {
            return None;
        }

        let direction = direction_quadrant(&candidate.offset);
        Some((direction, candidate.distance))
    }
}

// ---------------------------------------------------------------------------
// SpatialHexValidator
// ---------------------------------------------------------------------------

/// Validator for **hex grids** with no orientation information.
///
/// Classifies neighbors into 6 sextant directions and filters by distance.
/// Suitable for hex lattices of circular markers (ringgrid), blob detections,
/// or any approximately regular hex arrangement.
///
/// # PointData
///
/// `()` — no per-point data needed.
pub struct SpatialHexValidator<F: Float = f32> {
    /// Minimum acceptable neighbor distance (pixels).
    pub min_spacing: F,
    /// Maximum acceptable neighbor distance (pixels).
    pub max_spacing: F,
}

impl<F: Float> HexNeighborValidator<F> for SpatialHexValidator<F> {
    type PointData = ();

    fn validate(
        &self,
        _source_index: usize,
        _source_data: &(),
        candidate: &NeighborCandidate<F>,
        _candidate_data: &(),
    ) -> Option<(HexDirection, F)> {
        if candidate.distance < self.min_spacing || candidate.distance > self.max_spacing {
            return None;
        }

        let direction = direction_sextant(&candidate.offset);
        Some((direction, candidate.distance))
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::GridGraphParams;
    use crate::hex::graph::HexGridGraph;
    use crate::hex::traverse::{hex_assign_grid_coordinates, hex_connected_components};
    use crate::traverse::{assign_grid_coordinates, connected_components};
    use crate::{GridGraph, NodeNeighbor};
    use nalgebra::Point2;
    use std::collections::HashMap;
    use std::f32::consts::FRAC_PI_4;

    fn neighbor_map(neighbors: &[NodeNeighbor]) -> HashMap<NeighborDirection, &NodeNeighbor> {
        neighbors.iter().map(|n| (n.direction, n)).collect()
    }

    // -----------------------------------------------------------------------
    // XJunctionValidator tests
    // -----------------------------------------------------------------------

    fn make_chess_grid(rows: usize, cols: usize, spacing: f32) -> (Vec<Point2<f32>>, Vec<f32>) {
        let mut positions = Vec::new();
        let mut orientations = Vec::new();
        for j in 0..rows {
            for i in 0..cols {
                positions.push(Point2::new(i as f32 * spacing, j as f32 * spacing));
                orientations.push(if (i + j) % 2 == 0 {
                    FRAC_PI_4
                } else {
                    3.0 * FRAC_PI_4
                });
            }
        }
        (positions, orientations)
    }

    #[test]
    fn xjunction_regular_grid_center_has_four_neighbors() {
        let spacing = 10.0;
        let (positions, orientations) = make_chess_grid(3, 3, spacing);

        let validator = XJunctionValidator {
            min_spacing: 5.0,
            max_spacing: 15.0,
            tolerance_rad: 15.0f32.to_radians(),
        };
        let graph = GridGraph::build(
            &positions,
            &orientations,
            &validator,
            &GridGraphParams::default(),
        );

        let idx = |i: usize, j: usize| j * 3 + i;
        let center = neighbor_map(&graph.neighbors[idx(1, 1)]);
        assert_eq!(4, center.len());
        assert_eq!(idx(0, 1), center[&NeighborDirection::Left].index);
        assert_eq!(idx(2, 1), center[&NeighborDirection::Right].index);
        assert_eq!(idx(1, 0), center[&NeighborDirection::Up].index);
        assert_eq!(idx(1, 2), center[&NeighborDirection::Down].index);
    }

    #[test]
    fn xjunction_rejects_parallel_orientations() {
        let spacing = 10.0;
        let positions = vec![Point2::new(0.0, 0.0), Point2::new(spacing, 0.0)];
        // Same orientation — should be rejected
        let orientations = vec![FRAC_PI_4, FRAC_PI_4];

        let validator = XJunctionValidator {
            min_spacing: 5.0,
            max_spacing: 15.0,
            tolerance_rad: 15.0f32.to_radians(),
        };
        let graph = GridGraph::build(
            &positions,
            &orientations,
            &validator,
            &GridGraphParams {
                k_neighbors: 2,
                ..Default::default()
            },
        );

        assert!(graph.neighbors[0].is_empty());
        assert!(graph.neighbors[1].is_empty());
    }

    #[test]
    fn xjunction_rejects_out_of_range_distance() {
        let spacing = 30.0;
        let positions = vec![Point2::new(0.0, 0.0), Point2::new(spacing, 0.0)];
        let orientations = vec![FRAC_PI_4, 3.0 * FRAC_PI_4];

        let validator = XJunctionValidator {
            min_spacing: 5.0,
            max_spacing: 15.0, // 30 > 15, rejected
            tolerance_rad: 15.0f32.to_radians(),
        };
        let graph = GridGraph::build(
            &positions,
            &orientations,
            &validator,
            &GridGraphParams::default(),
        );

        assert!(graph.neighbors[0].is_empty());
        assert!(graph.neighbors[1].is_empty());
    }

    #[test]
    fn xjunction_rotated_grid_forms_single_component() {
        let spacing = 20.0;
        let angle = 40.0f32.to_radians();
        let ax = Vector2::new(angle.cos(), angle.sin());
        let ay = Vector2::new(-angle.sin(), angle.cos());
        let cols = 4usize;
        let rows = 4usize;

        let diag0 = angle + FRAC_PI_4;
        let diag1 = angle + 3.0 * FRAC_PI_4;

        let mut positions = Vec::new();
        let mut orientations = Vec::new();
        for j in 0..rows {
            for i in 0..cols {
                let pos = ax * (i as f32 * spacing) + ay * (j as f32 * spacing);
                positions.push(Point2::new(pos.x + 100.0, pos.y + 100.0));
                orientations.push(if (i + j) % 2 == 0 { diag0 } else { diag1 });
            }
        }

        let validator = XJunctionValidator {
            min_spacing: spacing * 0.5,
            max_spacing: spacing * 1.5,
            tolerance_rad: 20.0f32.to_radians(),
        };
        let graph = GridGraph::build(
            &positions,
            &orientations,
            &validator,
            &GridGraphParams {
                k_neighbors: 8,
                ..Default::default()
            },
        );

        let components = connected_components(&graph);
        assert_eq!(1, components.len());
        assert_eq!(cols * rows, components[0].len());

        let coords = assign_grid_coordinates(&graph, &components[0]);
        let coord_set: std::collections::HashSet<(i32, i32)> =
            coords.iter().map(|&(_, g)| (g.i, g.j)).collect();
        assert_eq!(cols * rows, coord_set.len());
    }

    #[test]
    fn xjunction_direction_symmetry() {
        let spacing = 20.0;
        let (positions, orientations) = make_chess_grid(3, 3, spacing);

        let validator = XJunctionValidator {
            min_spacing: spacing * 0.5,
            max_spacing: spacing * 1.5,
            tolerance_rad: 15.0f32.to_radians(),
        };
        let graph = GridGraph::build(
            &positions,
            &orientations,
            &validator,
            &GridGraphParams::default(),
        );

        for (a, neighbors) in graph.neighbors.iter().enumerate() {
            for n in neighbors {
                let b = n.index;
                let back = graph.neighbors[b].iter().find(|nn| nn.index == a);
                assert!(
                    back.is_some(),
                    "Edge {a}->{b} exists but reverse {b}->{a} does not"
                );
                assert_eq!(n.direction.opposite(), back.unwrap().direction,);
            }
        }
    }

    // -----------------------------------------------------------------------
    // SpatialSquareValidator tests
    // -----------------------------------------------------------------------

    #[test]
    fn spatial_square_regular_grid_center_has_four() {
        let spacing = 10.0;
        let mut positions = Vec::new();
        for j in 0..3 {
            for i in 0..3 {
                positions.push(Point2::new(i as f32 * spacing, j as f32 * spacing));
            }
        }
        let data = vec![(); positions.len()];

        let validator = SpatialSquareValidator {
            min_spacing: 5.0,
            max_spacing: 15.0,
        };
        let graph = GridGraph::build(&positions, &data, &validator, &GridGraphParams::default());

        let idx = |i: usize, j: usize| j * 3 + i;
        let center = neighbor_map(&graph.neighbors[idx(1, 1)]);
        assert_eq!(4, center.len());
        assert_eq!(idx(0, 1), center[&NeighborDirection::Left].index);
        assert_eq!(idx(2, 1), center[&NeighborDirection::Right].index);
        assert_eq!(idx(1, 0), center[&NeighborDirection::Up].index);
        assert_eq!(idx(1, 2), center[&NeighborDirection::Down].index);
    }

    #[test]
    fn spatial_square_rejects_out_of_range() {
        let positions = vec![
            Point2::new(0.0f32, 0.0),
            Point2::new(3.0, 0.0),  // too close
            Point2::new(50.0, 0.0), // too far
        ];
        let data = vec![(); 3];

        let validator = SpatialSquareValidator {
            min_spacing: 5.0,
            max_spacing: 15.0,
        };
        let graph = GridGraph::build(&positions, &data, &validator, &GridGraphParams::default());

        assert!(graph.neighbors[0].is_empty());
    }

    #[test]
    fn spatial_square_score_prefers_closest() {
        let positions = vec![
            Point2::new(0.0f32, 0.0),
            Point2::new(8.0, 0.0),  // closer
            Point2::new(12.0, 0.0), // farther, same quadrant
        ];
        let data = vec![(); 3];

        let validator = SpatialSquareValidator {
            min_spacing: 5.0,
            max_spacing: 15.0,
        };
        let graph = GridGraph::build(&positions, &data, &validator, &GridGraphParams::default());

        let right = graph.neighbors[0]
            .iter()
            .find(|n| n.direction == NeighborDirection::Right)
            .unwrap();
        assert_eq!(1, right.index); // closer one wins
    }

    #[test]
    fn spatial_square_diagonal_grid_works() {
        // Grid rotated 45° — quadrant classification still assigns consistent directions
        let spacing = 10.0;
        let angle = 45.0f32.to_radians();
        let ax = Vector2::new(angle.cos(), angle.sin());
        let ay = Vector2::new(-angle.sin(), angle.cos());

        let mut positions = Vec::new();
        for j in 0..3 {
            for i in 0..3 {
                let pos = ax * (i as f32 * spacing) + ay * (j as f32 * spacing);
                positions.push(Point2::new(pos.x + 50.0, pos.y + 50.0));
            }
        }
        let data = vec![(); positions.len()];

        let validator = SpatialSquareValidator {
            min_spacing: spacing * 0.5,
            max_spacing: spacing * 1.5,
        };
        let graph = GridGraph::build(&positions, &data, &validator, &GridGraphParams::default());

        let components = connected_components(&graph);
        assert_eq!(1, components.len());
        assert_eq!(9, components[0].len());
    }

    // -----------------------------------------------------------------------
    // SpatialHexValidator tests
    // -----------------------------------------------------------------------

    fn hex_lattice(radius: i32, spacing: f32) -> Vec<Point2<f32>> {
        let sqrt3 = 3.0f32.sqrt();
        let mut points = Vec::new();
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
    fn spatial_hex_center_has_six_neighbors() {
        let spacing = 50.0;
        let points = hex_lattice(2, spacing);
        let data = vec![(); points.len()];

        let validator = SpatialHexValidator {
            min_spacing: spacing * 0.5,
            max_spacing: spacing * 1.5,
        };
        let graph = HexGridGraph::build(
            &points,
            &data,
            &validator,
            &GridGraphParams {
                k_neighbors: 12,
                ..Default::default()
            },
        );

        let center = points
            .iter()
            .position(|p| p.x.abs() < 0.01 && p.y.abs() < 0.01)
            .unwrap();
        assert_eq!(6, graph.neighbors[center].len());
    }

    #[test]
    fn spatial_hex_edge_nodes_have_three() {
        let spacing = 50.0;
        let points = hex_lattice(1, spacing);
        let data = vec![(); points.len()];

        let validator = SpatialHexValidator {
            min_spacing: spacing * 0.5,
            max_spacing: spacing * 1.5,
        };
        let graph = HexGridGraph::build(
            &points,
            &data,
            &validator,
            &GridGraphParams {
                k_neighbors: 12,
                ..Default::default()
            },
        );

        for (i, p) in points.iter().enumerate() {
            if p.x.abs() < 0.01 && p.y.abs() < 0.01 {
                assert_eq!(6, graph.neighbors[i].len());
            } else {
                assert_eq!(3, graph.neighbors[i].len());
            }
        }
    }

    #[test]
    fn spatial_hex_rejects_out_of_range() {
        let points = vec![
            Point2::new(0.0f32, 0.0),
            Point2::new(3.0, 0.0), // too close
        ];
        let data = vec![(); 2];

        let validator = SpatialHexValidator {
            min_spacing: 10.0,
            max_spacing: 50.0,
        };
        let graph = HexGridGraph::build(&points, &data, &validator, &GridGraphParams::default());

        assert!(graph.neighbors[0].is_empty());
    }

    #[test]
    fn spatial_hex_single_component_and_correct_coordinates() {
        let spacing = 50.0;
        let points = hex_lattice(2, spacing);
        let data = vec![(); points.len()];

        let validator = SpatialHexValidator {
            min_spacing: spacing * 0.5,
            max_spacing: spacing * 1.5,
        };
        let graph = HexGridGraph::build(
            &points,
            &data,
            &validator,
            &GridGraphParams {
                k_neighbors: 12,
                ..Default::default()
            },
        );

        let components = hex_connected_components(&graph);
        assert_eq!(1, components.len());
        assert_eq!(points.len(), components[0].len());

        let coords = hex_assign_grid_coordinates(&graph, &components[0]);
        assert_eq!(points.len(), coords.len());

        // All coordinates should be unique
        let coord_set: std::collections::HashSet<(i32, i32)> =
            coords.iter().map(|&(_, g)| (g.i, g.j)).collect();
        assert_eq!(points.len(), coord_set.len());
    }
}
