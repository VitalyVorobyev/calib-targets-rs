# projective-grid

Generic 2D projective grid graph construction, traversal, and homography tools.

This crate provides reusable algorithms for building 4-connected grid graphs
from detected 2D corners, assigning grid coordinates via BFS traversal,
and computing projective mappings (homographies) for grid rectification.
It is pattern-agnostic and has no dependency on image types or calibration-specific
logic.

## Quickstart

```rust
use projective_grid::{
    GridGraph, GridGraphParams, GridIndex, NeighborCandidate, NeighborDirection,
    NeighborValidator,
};
use nalgebra::Point2;

/// Trivial validator: accepts any neighbor within range, classifies by quadrant.
struct QuadrantValidator;

impl NeighborValidator for QuadrantValidator {
    type PointData = ();

    fn validate(
        &self,
        _source_index: usize,
        _source_data: &(),
        candidate: &NeighborCandidate,
        _candidate_data: &(),
    ) -> Option<(NeighborDirection, f32)> {
        let dir = if candidate.offset.x.abs() > candidate.offset.y.abs() {
            if candidate.offset.x > 0.0 { NeighborDirection::Right }
            else { NeighborDirection::Left }
        } else {
            if candidate.offset.y > 0.0 { NeighborDirection::Down }
            else { NeighborDirection::Up }
        };
        Some((dir, candidate.distance))
    }
}

fn main() {
    let positions = vec![
        Point2::new(0.0f32, 0.0), Point2::new(10.0, 0.0),
        Point2::new(0.0, 10.0),   Point2::new(10.0, 10.0),
    ];
    let data = vec![(); 4];

    let graph = GridGraph::build(
        &positions, &data, &QuadrantValidator, &GridGraphParams::default(),
    );

    let components = projective_grid::connected_components(&graph);
    assert_eq!(components.len(), 1);

    let coords = projective_grid::assign_grid_coordinates(&graph, &components[0]);
    println!("assigned {} grid coordinates", coords.len());
}
```

## Modules

| Module | Description |
|---|---|
| `graph` | `GridGraph::build()` with pluggable `NeighborValidator` trait |
| `traverse` | `connected_components()`, `assign_grid_coordinates()` |
| `homography` | `Homography` struct, DLT estimation, 4-point solver with Hartley normalization |
| `grid_mesh` | `GridHomographyMesh` -- per-cell homographies for distortion-robust rectification |
| `grid_rectify` | `GridHomography` -- single global homography from grid corners |
| `grid_smoothness` | Neighbor-based position prediction and outlier detection |
| `grid_alignment` | `GridTransform`, `GridAlignment`, dihedral group D4 transforms |
| `direction` | `NeighborDirection`, `NodeNeighbor` |
| `grid_index` | `GridIndex { i, j }` |

## Design

The `NeighborValidator` trait is the main extension point. Implementors decide
which spatially close points qualify as grid neighbors and assign them a cardinal
direction and quality score. This lets the same graph construction algorithm work
for chessboards (orientation-based validation), ChArUco grids (marker-anchored
validation), or any other 2D grid pattern.

The crate is standalone: it depends only on `nalgebra`, `kiddo`, `serde`, and
`thiserror`. No image types, no calibration-specific logic.

## Features

- `tracing`: enables tracing instrumentation (currently reserved for future use).

## Links

- Repository: https://github.com/VitalyVorobyev/calib-targets-rs
