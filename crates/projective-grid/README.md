# projective-grid

Generic 2D projective grid graph construction, traversal, and homography tools
for **square** and **hexagonal** grids.

This crate provides reusable algorithms for building grid graphs from detected
2D corners, assigning grid coordinates via BFS traversal, and computing
projective mappings (homographies) for grid rectification. It supports both
4-connected square grids and 6-connected hexagonal grids (pointy-top, axial
coordinates). It is pattern-agnostic and has no dependency on image types or
calibration-specific logic.

## Quickstart

### Square grid

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

### Hex grid

```rust
use projective_grid::hex::{
    HexDirection, HexGridGraph, HexNeighborValidator, HexNodeNeighbor,
    hex_connected_components, hex_assign_grid_coordinates,
};
use projective_grid::{GridGraphParams, NeighborCandidate};
use nalgebra::Point2;

/// Classify neighbors into hex sextants by angle.
struct SextantValidator;

impl HexNeighborValidator for SextantValidator {
    type PointData = ();

    fn validate(
        &self,
        _source_index: usize,
        _source_data: &(),
        candidate: &NeighborCandidate,
        _candidate_data: &(),
    ) -> Option<(HexDirection, f32)> {
        let deg = candidate.offset.y.atan2(candidate.offset.x).to_degrees();
        let dir = if (-30.0..30.0).contains(&deg) { HexDirection::East }
            else if (30.0..90.0).contains(&deg) { HexDirection::SouthEast }
            else if (90.0..150.0).contains(&deg) { HexDirection::SouthWest }
            else if !(-150.0..150.0).contains(&deg) { HexDirection::West }
            else if (-150.0..-90.0).contains(&deg) { HexDirection::NorthWest }
            else { HexDirection::NorthEast };
        Some((dir, candidate.distance))
    }
}

fn main() {
    // Build a small hex lattice (pointy-top, axial coordinates)
    let spacing = 10.0f32;
    let sqrt3 = 3.0f32.sqrt();
    let mut positions = Vec::new();
    for q in -1..=1i32 {
        for r in -1..=1i32 {
            if (q + r).abs() > 1 { continue; }
            let x = spacing * (q as f32 + r as f32 * 0.5);
            let y = spacing * (r as f32 * sqrt3 / 2.0);
            positions.push(Point2::new(x, y));
        }
    }
    let data = vec![(); positions.len()];

    let graph = HexGridGraph::build(
        &positions, &data, &SextantValidator, &GridGraphParams::default(),
    );

    let components = hex_connected_components(&graph);
    let coords = hex_assign_grid_coordinates(&graph, &components[0]);
    println!("assigned {} hex grid coordinates", coords.len());
}
```

## Modules

### Square grid (4-connected)

| Module | Description |
|---|---|
| `graph` | `GridGraph::build()` with pluggable `NeighborValidator` trait |
| `traverse` | `connected_components()`, `assign_grid_coordinates()` |
| `grid_smoothness` | Neighbor-based position prediction (2 axis pairs) and outlier detection |
| `grid_alignment` | `GridTransform`, `GridAlignment`, dihedral group D4 (8 transforms) |
| `grid_rectify` | `GridHomography` -- single global homography from grid corners |
| `grid_mesh` | `GridHomographyMesh` -- per-cell homographies for distortion-robust rectification |
| `direction` | `NeighborDirection` (Right/Left/Up/Down), `NodeNeighbor` |

### Hexagonal grid (6-connected)

| Module | Description |
|---|---|
| `hex::graph` | `HexGridGraph::build()` with pluggable `HexNeighborValidator` trait |
| `hex::traverse` | `hex_connected_components()`, `hex_assign_grid_coordinates()` |
| `hex::smoothness` | Neighbor-based position prediction (3 axis pairs) and outlier detection |
| `hex::alignment` | Dihedral group D6 (12 transforms) via `GridTransform` |
| `hex::rectify` | `HexGridHomography` -- global homography with axial-to-rectified mapping |
| `hex::mesh` | `HexGridHomographyMesh` -- per-triangle affine/homography mesh |
| `hex::direction` | `HexDirection` (E/W/NE/SW/NW/SE), `HexNodeNeighbor` |

### Shared

| Module | Description |
|---|---|
| `homography` | `Homography` struct, DLT estimation, 4-point solver with Hartley normalization |
| `grid_index` | `GridIndex { i, j }` -- used as `(col, row)` for square and `(q, r)` for hex |
| `grid_alignment` | `GridTransform`, `GridAlignment` -- generic 2x2 integer matrix transforms |

## Design

The validator traits (`NeighborValidator` for square, `HexNeighborValidator` for
hex) are the main extension points. Implementors decide which spatially close
points qualify as grid neighbors and assign them a direction and quality score.
This lets the same graph construction algorithm work for chessboards
(orientation-based validation), ChArUco grids (marker-anchored validation),
ring calibration targets on hex lattices, or any other 2D grid pattern.

### Hex coordinate convention

Hex grids use **pointy-top** orientation with **axial coordinates** `(q, r)`,
stored in `GridIndex` as `i = q`, `j = r`:

- `q` increases eastward
- `r` increases south-eastward
- Six directions: E `(+1, 0)`, W `(-1, 0)`, NE `(+1, -1)`, SW `(-1, +1)`, NW `(0, -1)`, SE `(0, +1)`
- Rectified mapping: `x = s * (q + r/2)`, `y = s * (r * sqrt(3)/2)` where `s` = pixels per cell

### Hex mesh triangulation

The hex mesh (`HexGridHomographyMesh`) decomposes the axial grid into
parallelogram cells, each split into two triangles. Each triangle stores both
an `AffineTransform2D` (exact 3-point mapping) and a `Homography` (4-point,
using the centroid), giving callers the choice between speed and projective
accuracy.

The crate is standalone: it depends only on `nalgebra`, `kiddo`, `serde`, and
`thiserror`. No image types, no calibration-specific logic.

## Features

- `tracing`: enables tracing instrumentation (currently reserved for future use).

## Links

- Repository: https://github.com/VitalyVorobyev/calib-targets-rs
