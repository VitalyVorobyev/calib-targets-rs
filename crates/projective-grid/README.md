# projective-grid

A standalone Rust crate for recovering **locally-planar 2D grids** — square
chessboards, hexagonal lattices, or any other structured point cloud — from
arbitrary 2D point sets. The crate is pattern-agnostic: you plug in a
validator that knows "does this nearby point qualify as a grid neighbor of
this one?" and the crate does the rest (graph construction, coordinate
assignment, homography fitting, post-growth validation).

`projective-grid` is the algorithmic core behind
[`calib-targets-chessboard`](../calib-targets-chessboard), but has no
calibration-specific dependencies. Callers outside computer vision — mesh
extraction from point clouds, game-board rectification, laser-dot grid
fitting — can consume it directly.

## Install

```toml
[dependencies]
projective-grid = "0.6"
nalgebra = "0.34"    # Point2<f32> is the shared coordinate type
```

## Quickstart — square grid

```rust
use projective_grid::{
    GridGraph, GridGraphParams, NeighborCandidate, NeighborDirection,
    NeighborValidator, assign_grid_coordinates, connected_components,
};
use nalgebra::Point2;

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
        } else if candidate.offset.y > 0.0 {
            NeighborDirection::Down
        } else {
            NeighborDirection::Up
        };
        Some((dir, candidate.distance))
    }
}

let positions = vec![
    Point2::new(0.0_f32, 0.0), Point2::new(10.0, 0.0),
    Point2::new(0.0, 10.0),    Point2::new(10.0, 10.0),
];
let data = vec![(); 4];

let graph = GridGraph::build(
    &positions, &data, &QuadrantValidator, &GridGraphParams::default(),
);

let components = connected_components(&graph);
let coords = assign_grid_coordinates(&graph, &components[0]);
```

## Quickstart — hex grid

See `hex::HexGridGraph` + `hex::HexNeighborValidator`. Same shape, axial
`(q, r)` coordinates, six neighbor directions.

## Core concepts

The crate is organised around four stages that most grid-detection pipelines
share. Each can be used standalone or as part of a full pipeline.

| Stage | Entry points | What it does |
|---|---|---|
| **Cell-size estimate** | [`estimate_global_cell_size`], [`estimate_local_steps`] | Infer an approximate lattice spacing from a raw point cloud. |
| **Graph construction** | [`GridGraph::build`], [`hex::HexGridGraph::build`], [`NeighborValidator`] | KD-tree based nearest-neighbour search + validator-driven edge admission. |
| **Coordinate assignment** | [`assign_grid_coordinates`], [`connected_components`] | BFS from one seed corner, producing integer `(i, j)` / axial `(q, r)` labels. |
| **Post-growth validation** | [`square::validate`] | Line-collinearity + local-homography residuals → blacklist of outlier corners. |

Alongside those stages, the crate ships reusable utilities:

- **Circular statistics** ([`circular_stats`]) — plateau-aware peak detection
  and double-angle 2-means for axis-angle histograms.
- **Homography** ([`homography`]) — 4-point DLT solver with Hartley
  normalization.
- **Mesh rectification** ([`square::mesh`], [`hex::mesh`]) — per-cell
  homographies for distortion-robust unwarp on curved lenses.

## API tour

### Square (4-connected)

Top-level square-grid re-exports at the crate root (back-compat), with full
module paths under [`square`]:

| Item | Path | Purpose |
|---|---|---|
| `GridIndex` | `square::index::GridIndex` | `(i, j)` cell identifier, shared with hex as axial `(q, r)` |
| `NeighborDirection`, `NodeNeighbor` | `square::direction` | Four cardinal directions + the stored-edge struct |
| `GridAlignment`, `GridTransform`, `GRID_TRANSFORMS_D4` | `square::alignment` | Dihedral group D4 (8 transforms) over `(i, j)` |
| `GridHomography` | `square::rectify` | Global homography from assigned grid corners |
| `GridHomographyMesh` | `square::mesh` | Per-cell homographies |
| `find_inconsistent_corners`, `predict_grid_position` | `square::smoothness` | Midpoint prediction / outlier detection |
| `SpatialSquareValidator`, `XJunctionValidator` | `square::validators` | Ready-to-use validators |
| `validate`, `ValidationParams`, `ValidationResult`, `LabelledEntry` | `square::validate` | Post-growth line / local-H validation |

### Hex (6-connected)

Lives under [`hex`]. Same module shape — `HexGridGraph`, `HexDirection`,
`HexGridHomography`, `HexGridHomographyMesh`, etc. Uses pointy-top axial
coordinates `(q, r)`:

- `q` increases east, `r` increases south-east.
- Six directions: E / W / NE / SW / NW / SE.
- Rectified mapping: `x = s · (q + r/2)`, `y = s · r · √3/2`.

### Shared

| Item | Purpose |
|---|---|
| [`graph::NeighborValidator`] | The one trait every grid-detection pipeline implements. |
| [`graph::GridGraphParams`] | KD-tree radius + max-candidates knobs. |
| [`graph_cleanup`] | Symmetry enforcement, straightness pruning, crossing-edge removal. |
| [`homography::homography_from_4pt`] | 4-point DLT with Hartley normalization. |
| [`circular_stats`] | Axis-angle histogram + plateau peaks + double-angle 2-means. |

## Design notes

### Validator pattern

`NeighborValidator` is the one trait you must implement for a new pattern.
Implementors take a *candidate* (spatially close point + its data payload)
and return `Some((direction, quality_score))` iff that candidate qualifies
as a neighbor. The graph builder takes it from there.

Two ready-to-use validators live in `square::validators`:

- `SpatialSquareValidator` — pure distance / angle quadrant validation, no
  corner orientation required.
- `XJunctionValidator` — additionally consumes per-corner X-junction axis
  estimates (e.g. from ChESS), rejecting neighbors whose axes don't match.

### Undirected-angle (mod-π) circular means

When averaging axis directions (orientation, not headings), accumulate
`(cos 2θ, sin 2θ)` and halve the resulting atan2. `circular_stats::
refine_2means_double_angle` does this correctly; naive `(cos θ, sin θ)`
averaging silently breaks at the 0°/180° seam. This was a real bug in
chessboard v1 and is documented in the workspace's `CLAUDE.md`.

### Plateau-aware peak picking

When a physical direction's mass straddles a histogram bin boundary, the
smoothed peak is flat-topped across two adjacent bins. Naive strict
local-maximum detection misses it. `circular_stats::pick_two_peaks`
detects plateau peaks — a maximal run of equal-valued bins bordered on both
sides by strictly lower values — and returns the plateau midpoint.

## Features

- `tracing` — enables tracing instrumentation (reserved for future use).

## Known caveats

- **`NeighborValidator` is synchronous.** There's no async validator trait;
  the graph builder calls the validator serially per candidate edge.
- **2D only.** Coordinates are `nalgebra::Point2<f32>`. There's no 3D grid
  support.
- **Square grid assumes 4-connectivity.** Diagonal neighbors are not part
  of the default `NeighborDirection` enum; implement a custom validator
  that returns only cardinal directions.

## Related crates

- [`calib-targets-chessboard`](../calib-targets-chessboard) — the
  invariant-first chessboard detector; the reference consumer of this crate.
- [`calib-targets-puzzleboard`](../calib-targets-puzzleboard) — self-
  identifying chessboard variant, also consumes `projective-grid` for
  graph construction.
- [`calib-targets`](../calib-targets) — facade crate with `detect_*` and
  `detect_*_best` helpers.

## Book

The `book/` at the repository root has a chapter dedicated to
`projective-grid`'s pipeline with end-to-end worked examples.
