# projective-grid (Standalone)

> Code: [`projective-grid`](https://github.com/VitalyVorobyev/calib-targets-rs/tree/main/crates/projective-grid).

`projective-grid` is the pattern-agnostic core of the workspace's
grid detectors. It provides the algorithmic pieces every grid-
detection pipeline needs — KD-tree graph construction, BFS coordinate
assignment, homography-based rectification, circular-statistics peak
picking, line / local-homography validation — with no dependency on
calibration-specific types.

You can consume it directly for non-calibration use cases: rectifying
a photograph of a board game, fitting a locally-planar lattice to a
laser-dot cloud, extracting a grid from a scanned document, or
building a new detector for a pattern the workspace doesn't yet ship.

---

## Design

The crate is organised around four stages that most grid-detection
pipelines share. Each can be used standalone or combined via a trait-
driven pipeline:

| Stage | Entry points | What it does |
|---|---|---|
| **Cell-size estimate** | `estimate_global_cell_size`, `estimate_local_steps` | Infer an approximate lattice spacing from a raw point cloud. |
| **Graph construction** | `GridGraph::build`, `hex::HexGridGraph::build`, `NeighborValidator` trait | KD-tree nearest-neighbour search + validator-driven edge admission. |
| **Coordinate assignment** | `assign_grid_coordinates`, `connected_components`, `square::bfs_grow`, `GrowValidator` trait | BFS from one seed corner (or 2×2 quad), producing integer `(i, j)` / axial `(q, r)` labels. |
| **Post-growth validation** | `square::validate` | Line collinearity + local-homography residuals → blacklist of outlier corners. |

Alongside those stages, the crate ships reusable utilities:

- **Circular statistics** (`circular_stats`) — plateau-aware peak
  detection and double-angle 2-means for axis-angle histograms.
- **Homography** (`homography`) — 4-point DLT solver with Hartley
  normalisation.
- **Mesh rectification** (`square::mesh`, `hex::mesh`) — per-cell
  homographies for distortion-robust unwarp on curved lenses.

---

## Extension points

Two trait pairs let you plug pattern-specific logic into the generic
machinery without forking the pipeline:

### `NeighborValidator` (graph construction)

```rust,ignore
use projective_grid::{NeighborCandidate, NeighborDirection, NeighborValidator};

impl NeighborValidator for MyValidator {
    type PointData = MyCornerMeta;

    fn validate(
        &self,
        source_index: usize,
        source_data: &MyCornerMeta,
        candidate: &NeighborCandidate,
        candidate_data: &MyCornerMeta,
    ) -> Option<(NeighborDirection, f32)> {
        // return Some((direction, quality_score)) iff candidate is
        // a valid neighbor of source
    }
}
```

Used during Stage-2 graph construction to admit edges between nearby
points. Ships with two ready-to-use implementations:
- `SpatialSquareValidator` — distance + angle-quadrant only, no
  orientation required.
- `XJunctionValidator` — consumes per-corner ChESS X-junction axis
  estimates and requires the neighbour's axes to match.

### `GrowValidator` (BFS growth)

```rust,ignore
use projective_grid::square::grow::{Admit, GrowValidator, LabelledNeighbour};
use nalgebra::Point2;

impl GrowValidator for MyValidator {
    fn is_eligible(&self, idx: usize) -> bool { /* … */ }
    fn required_label_at(&self, i: i32, j: i32) -> Option<u8> { /* … */ }
    fn label_of(&self, idx: usize) -> Option<u8> { /* … */ }

    fn accept_candidate(
        &self,
        idx: usize,
        at: (i32, i32),
        prediction: Point2<f32>,
        neighbours: &[LabelledNeighbour],
    ) -> Admit {
        // Accept / Reject per candidate in order of increasing
        // distance to `prediction`.
    }

    fn edge_ok(
        &self,
        candidate_idx: usize,
        neighbour_idx: usize,
        at_candidate: (i32, i32),
        at_neighbour: (i32, i32),
    ) -> bool { /* soft per-edge check */ true }
}
```

Used by `square::bfs_grow` (and the hex equivalent under
`hex::`) to walk a labelled grid from a 2×2 seed. Every pattern-
specific rule — parity, axis matching, label enumeration — lives
inside the validator.

The chessboard detector's plug-in
(`crates/calib-targets-chessboard/src/grow.rs`) is a reference
implementation: ~90 lines of chess-specific axis-slot logic on top of
the generic BFS.

---

## Module layout

```
projective-grid/src/
├── lib.rs
├── float_helpers.rs          (private)
├── graph.rs                  NeighborValidator, GridGraph, GridGraphParams
├── graph_cleanup.rs          symmetry, straightness, crossing pruning
├── global_step.rs            cell-size estimation from a raw cloud
├── local_step.rs             per-region local-step estimation
├── traverse.rs               connected_components, assign_grid_coordinates
├── homography.rs             Homography, homography_from_4pt
├── circular_stats.rs         wrap_pi, smooth_circular_5, pick_two_peaks,
│                             refine_2means_double_angle
├── square/                   4-connected square-grid support
│   ├── alignment.rs          D4 transforms
│   ├── direction.rs          NeighborDirection (Right/Left/Up/Down)
│   ├── grow.rs               GrowValidator, bfs_grow, Seed, GrowResult
│   ├── index.rs              GridIndex (i, j)
│   ├── mesh.rs               GridHomographyMesh (per-cell)
│   ├── rectify.rs            GridHomography (global)
│   ├── smoothness.rs         predict_grid_position, find_inconsistent_corners
│   ├── validate.rs           line + local-H post-growth validator
│   └── validators.rs         SpatialSquareValidator, XJunctionValidator
└── hex/                      6-connected hex-grid mirror (pointy-top, axial (q, r))
    ├── direction.rs
    ├── graph.rs
    ├── index.rs
    ├── mesh.rs
    ├── rectify.rs
    ├── smoothness.rs
    ├── traverse.rs
    └── validators.rs
```

---

## Invariants worth keeping in mind

### Undirected-angle circular means

When averaging axis directions (orientations, not headings), accumulate
`(cos 2θ, sin 2θ)` and halve the resulting atan2. `circular_stats::
refine_2means_double_angle` does this correctly; naive `(cos θ, sin θ)`
averaging silently breaks at the 0°/180° seam. The chessboard v1
detector had this exact bug and a hard-to-reproduce Phase-4 regression
until the fix landed.

### Plateau-aware peak detection

When a physical direction's mass straddles a histogram bin boundary,
the smoothed peak is flat-topped across two adjacent bins. Naive
strict local-maximum detection misses it entirely. `circular_stats::
pick_two_peaks` handles this by looking for maximal runs of equal-
valued bins bordered on both sides by strictly lower values, and
returning the plateau's midpoint. This fixed the synthetic-
puzzleboard regression on `testdata/puzzleboard_reference/example8.png`
and `example9.png`.

### Non-negative grid labels with visual top-left origin

All `(i, j)` output from `bfs_grow` is rebased so the bounding-box
minimum is `(0, 0)`. Downstream consumers that canonicalise axis
direction (chessboard v2 does this in
`calib_targets_chessboard::Detector::detect`) additionally swap /
flip axes so `(0, 0)` sits at the **visual top-left** of the detected
grid — `+i` points right (+x), `+j` points down (+y). This is not
enforced by `bfs_grow` itself — it's a chessboard-side contract.

---

## When not to use this

- **3D grids.** Coordinates are `nalgebra::Point2<f32>`. There's no
  3D support.
- **Non-planar surfaces.** The homography machinery assumes each
  labelled region fits a single projective transform. Severely curved
  surfaces need the mesh variant at best.
- **Dense point clouds without structure.** The graph builder assumes
  the lattice spacing is recoverable from nearest-neighbour
  distances.
