# projective-grid

Pattern-agnostic algorithms for turning a cloud of 2D points into a grid:
graph construction, connected components, integer `(i, j)` coordinate
assignment, line/local-homography validation, and per-cell rectification.

`projective-grid` is the algorithmic backbone behind every grid-based
detector in the [calib-targets] workspace — chessboard, ChArUco, marker
board, PuzzleBoard — but it has no calibration-specific dependencies.
Callers outside computer vision (mesh extraction from point clouds, game
board rectification, laser-dot grid fitting, printed-PCB alignment) can use
it directly.

Full API reference: see the [`projective-grid` book chapter][book-chapter].

## Install

```toml
[dependencies]
projective-grid = "0.7"
nalgebra = "0.34"    # Point2<f32> is the shared coordinate type
```

## Quickstart

```rust
use nalgebra::Point2;
use projective_grid::{
    GridGraph, GridGraphParams, SpatialSquareValidator,
    assign_grid_coordinates, connected_components,
};

let positions: Vec<Point2<f32>> = /* detected points */;
let data = vec![(); positions.len()];        // no per-point metadata

let validator = SpatialSquareValidator::<f32> {
    min_spacing: 5.0,
    max_spacing: 200.0,
};
let graph = GridGraph::build(
    &positions,
    &data,
    &validator,
    &GridGraphParams { k_neighbors: 8, max_distance: 200.0 },
);

for component in connected_components(&graph) {
    // assign_grid_coordinates returns (point_index, GridIndex { i, j }) pairs.
    for (point_idx, grid_idx) in assign_grid_coordinates(&graph, &component) {
        let p = positions[point_idx];
        println!("({}, {}) at ({}, {})", grid_idx.i, grid_idx.j, p.x, p.y);
    }
}
```

The input is an array of 2D points; the output is one `(i, j)` label per
point in each connected component. The calibration-specific detectors plug
in richer validators (e.g. `XJunctionValidator`, which also requires
corner-axis alignment between neighbours), but the pipeline is the same.

## Inputs and outputs

| Stage | Input | Output |
|---|---|---|
| Cell-size estimate | `&[Point2<f32>]` | [`GlobalStepEstimate`] (`cell_size`, `confidence`, …) |
| Graph build | points + per-point data + [`NeighborValidator`] + [`GridGraphParams`] | [`GridGraph`] |
| Component split | [`GridGraph`] | `Vec<Vec<usize>>` — each component is a list of point indices |
| Coordinate assignment | graph + one component | `Vec<(usize, GridIndex)>` — point index + its `(i, j)` label |
| Post-growth validation | labelled corners + [`ValidationParams`] | [`ValidationResult`] — inlier mask + rejection reasons |
| Rectification | labelled corners | [`GridHomography`] (single) or [`GridHomographyMesh`] (per-cell) |

All public types re-exported at the crate root; the detailed module layout
sits under [`square`] (4-connected) and [`hex`] (6-connected pointy-top).

## Configuration

Tuning knobs cluster into three groups. Defaults are chosen so that clean
synthetic grids "just work"; tune only when a specific input fails.

- **[`GridGraphParams`]** — `k_neighbors` (KD-tree fan-out, default 8) and
  `max_distance` (radius cap on candidate edges). Raise `max_distance` so
  the largest real cell in your image comfortably fits; lower it to stop
  false edges jumping across gaps in sparse point clouds.
- **[`SpatialSquareValidator`] / [`XJunctionValidator`]** — pattern-specific
  neighbour admission. `SpatialSquareValidator` filters by absolute
  `min_spacing` / `max_spacing`; `XJunctionValidator` additionally reads
  per-corner axis estimates and rejects neighbours whose edges don't lie
  on the corner's own axes.
- **[`ValidationParams`]** — line-collinearity (`line_tol_rel`,
  `projective_line_tol_rel`, `line_min_members`) and local-homography
  (`local_h_tol_rel`) residual thresholds for post-growth cleanup. Loosen
  under heavy radial distortion; tighten for near-pinhole cameras.

See the book chapter for parameter-by-parameter guidance.

## Tuning difficult inputs

- **Fragmented grid** — `connected_components` returns several components.
  Reconnect by rerunning with a larger `GridGraphParams::max_distance`, or
  keep the fragments and merge downstream.
- **Moderate oblique perspective** — defaults hold. If real edges are
  rejected, widen `GridGraphParams::k_neighbors` so perspective-
  foreshortened candidates survive the KD-tree fan-out.
- **Moderate radial distortion** — use [`GridHomographyMesh`] rather than
  [`GridHomography`] for rectification: per-cell homographies absorb
  curvature that a single global homography cannot.
- **Dense clutter (dot-pattern detectors, laser spots)** — feed
  [`estimate_global_cell_size`] on a central crop first, then seed
  `SpatialSquareValidator::{min_spacing, max_spacing}` with a band around
  the returned `cell_size`.

## Limitations

- **2D only.** Coordinates are `nalgebra::Point2<f32>`; no 3D support.
- **4-connected square grid by default.** Diagonal neighbours require a
  custom validator; they aren't part of the stock `NeighborDirection` enum.
- **Synchronous validator.** The graph builder calls validators serially
  per candidate; no async or streaming API.
- **Roughly-square cells.** Strongly anisotropic aspect ratios (>3:1)
  degrade the graph; rescale the input cloud first.
- **No parameter auto-tuning.** When defaults do not close the grid,
  callers must tune — usually by estimating cell size first and seeding
  `GridGraphParams` with it.

## Design notes (why it works under distortion)

- **Local invariants, not global homography.** The graph builder only ever
  reasons about a point and its nearest neighbours, which is affine-locally
  valid even under moderate perspective or radial distortion.
- **Undirected-angle circular means.** Any function averaging axis angles
  accumulates `(cos 2θ, sin 2θ)` and halves the resulting `atan2` — naive
  `(cos θ, sin θ)` averaging breaks at the 0°/180° seam. See
  [`circular_stats::refine_2means_double_angle`].
- **Plateau-aware peak picking.** When a physical direction's mass
  straddles a histogram-bin boundary, the smoothed peak is flat-topped
  across two adjacent bins. [`circular_stats::pick_two_peaks`] detects the
  plateau midpoint so axis estimates stay stable as input rotates.

## Features

- `tracing` — enables tracing instrumentation (reserved for future use).

## Related crates

- [calib-targets-chessboard][] — the reference consumer: invariant-first
  chessboard detector.
- [calib-targets-puzzleboard][] — self-identifying chessboard variant.
- [calib-targets][] — workspace facade with `detect_*` / `detect_*_best`.

[calib-targets]: https://docs.rs/calib-targets
[calib-targets-chessboard]: https://docs.rs/calib-targets-chessboard
[calib-targets-puzzleboard]: https://docs.rs/calib-targets-puzzleboard
[book-chapter]: https://vitalyvorobyev.github.io/calib-targets-rs/projective_grid.html
