# calib-targets-chessboard

`calib-targets-chessboard` detects a plain chessboard from a cloud of ChESS corners. It is graph-based and perspective-aware, and it returns integer grid coordinates for each detected corner.

## Detection pipeline

The detector follows these steps (see `ChessboardDetector`):

1. Filter corners by minimum strength.
2. Estimate two dominant grid axes from corner orientations.
3. Estimate a base spacing from nearest-neighbor distances.
4. For each corner, find up to 4 neighbors (right/left/up/down) based on distance and orientation consistency.
5. Build a 4-connected undirected grid graph.
6. BFS each connected component and assign integer `(i, j)` coordinates.
7. Compute width, height, and completeness per component.
8. Keep the best component that matches expected size and completeness thresholds.

Currently the detector returns at most one board instance (the best-scoring component).

## Key types

- `ChessboardDetector`: main entry point.
- `ChessboardParams`: detection thresholds and expected board size.
- `GridGraphParams`: neighbor search and geometric constraints.
- `ChessboardDetectionResult`:
  - `detection`: `TargetDetection` with labeled corners.
  - `inliers`: indices into the corner list used for rectification.
  - `orientations`: estimated grid axes (optional).
  - `debug`: optional histogram and graph data for diagnostics.

## Parameters

`ChessboardParams` controls high-level validity checks:

- `min_corner_strength`: filter weak corners early.
- `min_corners`: minimum number of corners to accept a component.
- `expected_rows`, `expected_cols`: **inner corner counts** in each direction.
- `completeness_threshold`: detected / expected corner ratio.
- `use_orientation_clustering`: toggle orientation clustering (enabled by default).

`GridGraphParams` controls how neighbors are chosen:

- `min_spacing_pix`, `max_spacing_pix`: expected corner spacing range in pixels.
- `k_neighbors`: how many nearest neighbors to consider.
- `orientation_tolerance_deg`: angular tolerance for neighbor relations.

## Grid graph details

Neighbor selection uses orientation information in two modes:

- **With clustering**: corners are labeled by one of two axis clusters. Candidate edges must align with one of the two grid directions derived from those clusters.
- **Without clustering**: orientations are checked for near-orthogonality, and the edge direction must be close to 45 degrees from each corner orientation.

Edges are classified into `Right`, `Left`, `Up`, `Down` based on image-space directions, and only the best candidate per direction is kept. This yields a clean 4-connected grid graph for BFS.

## Rectification helpers

The crate provides two rectification options:

- `rectify_from_chessboard_result`: fits a single global homography and produces a `RectifiedBoardView`.
- `rectify_mesh_from_grid`: fits one homography per cell and produces a `RectifiedMeshView` (more robust to lens distortion).

Both require labeled corners and a chosen `px_per_square` scale.

## Example

```rust
use calib_targets_chessboard::{ChessboardDetector, ChessboardParams, GridGraphParams};
use calib_targets_core::Corner;

fn detect(corners: &[Corner]) {
    let params = ChessboardParams::default();
    let detector = ChessboardDetector::new(params)
        .with_grid_search(GridGraphParams::default());

    if let Some(result) = detector.detect_from_corners(corners) {
        println!("detected {} corners", result.detection.corners.len());
    }
}
```

For a full runnable example, see `crates/calib-targets-chessboard/examples/chessboard.rs`.
