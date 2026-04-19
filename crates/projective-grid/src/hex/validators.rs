//! Ready-to-use [`HexNeighborValidator`] implementations for hex grids.
//!
//! | Validator | PointData | Use case |
//! |-----------|-----------|----------|
//! | [`SpatialHexValidator`] | `()` | Unoriented features on a hex lattice (ringgrid, etc.) |

use crate::float_helpers::{lit, to_degrees};
use crate::graph::NeighborCandidate;
use crate::hex::direction::HexDirection;
use crate::hex::graph::HexNeighborValidator;
use crate::Float;
use nalgebra::Vector2;

// ---------------------------------------------------------------------------
// Geometry helpers (private to the hex validators)
// ---------------------------------------------------------------------------

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
    use nalgebra::Point2;

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
