# projective-grid

Pattern-agnostic algorithms for turning a 2D point cloud into a labelled
grid: seed-and-grow BFS, boundary extension via fitted homography,
post-grow validation, per-cell rectification.

`projective-grid` is the algorithmic backbone behind every grid-based
detector in the [calib-targets] workspace — chessboard, ChArUco, marker
board, PuzzleBoard — but has no calibration-specific dependencies.

Full API reference: see the [`projective-grid` book chapter][book-chapter].

## Install

```toml
[dependencies]
projective-grid = "0.9"
```

## Pipeline at a glance

`projective-grid` ships two grid-construction pipelines that produce
the same `(i, j) → corner_idx` map and share the same downstream
validation, rectification, and component-merge machinery.

### Quick start: point cloud in, labelled grid out

The zero-config entry point is `detect_regular_grid`. Hand it a
`&[Point2<f32>]`; it returns a `RegularGridDetection` where every
recovered corner carries its `(i, j)` label and the index back into
your input slice. No validator scaffolding required:

```rust
use nalgebra::Point2;
use projective_grid::detect_regular_grid;

// A clean 5×4 grid at 30 px pitch (clean, rotated, or perspective-
// warped input all work).
let mut points = Vec::new();
for j in 0..4 {
    for i in 0..5 {
        points.push(Point2::new(i as f32 * 30.0, j as f32 * 30.0));
    }
}

let grid = detect_regular_grid(&points).expect("clean grid detects");
assert_eq!(grid.points.len(), 20);
// Labels rebased so the bbox minimum is (0, 0); +i points right,
// +j points down (visual top-left origin).
```

For tuning, use `RegularGridDetector` + `RegularGridParams`
(boundary-extension strategy, top-left canonicalisation toggle,
connectivity pruning). `RegularGridDetector::detect_all` returns one
detection per disjoint component.

### Square seed-and-grow (advanced / pattern-specific)

`detect_regular_grid` is a thin wrapper over the validator-driven
`detect_square_grid` with a built-in permissive regular-grid policy.
When you need pattern-specific rules — parity, axis-cluster, marker
slots — implement the `square::grow::GrowValidator` +
`square::seed::finder::SeedQuadValidator` traits and call the
advanced API directly:

```rust
use projective_grid::square::{
    grow::{bfs_grow, GrowParams, GrowValidator, Seed},
    extension::{extend_via_global_homography, ExtensionParams},
    validate::{validate, LabelledEntry, ValidationParams},
};

// 1. Caller supplies: corner positions, a 2×2 seed quad, a validator
//    that knows the pattern's invariants, and a cell-size estimate.
let positions: Vec<nalgebra::Point2<f32>> = /* … */;
let seed: Seed = /* … */;
let cell_size: f32 = /* … */;
let validator: &impl GrowValidator = /* … */;

// 2. BFS-grow with adaptive local-step prediction.
let mut grow = bfs_grow(&positions, seed, cell_size, &GrowParams::default(), validator);

// 3. Optional: extend the labelled set via a globally-fit
//    homography. Refuses to extrapolate when the H residuals on the
//    BFS-validated set indicate the planar / pinhole assumption is
//    violated (heavy lens distortion, non-planar target).
let _stats = extend_via_global_homography(
    &positions,
    &mut grow,
    cell_size,
    &ExtensionParams::default(),
    validator,
);

// 4. Validation: line / local-H residual checks produce a blacklist of
//    outlier corner indices to drop and re-grow.
let entries: Vec<LabelledEntry> = /* (corner_idx, pixel, grid) per labelled */;
let _result = validate(&entries, cell_size, &ValidationParams::default());
```

The chessboard / ChArUco / PuzzleBoard detectors in the workspace
implement their pattern-specific `GrowValidator` and call the same
machinery. Their orchestrators iterate grow + validate with a
blacklist until the labelled set converges, then run boundary
extension, then re-validate.

Generic, target-agnostic output cleanup lives in `square::cleanup`
(`rebase_to_origin`, `prune_to_main_component`, `canonicalize_top_left`,
`sorted_grid_points`) — `detect_regular_grid` applies these
internally, and they are exposed for pattern detectors to reuse.

### Local-homography boundary extension

`square::extension::extend_via_local_homography` is the per-candidate
counterpart to `extend_via_global_homography`. Instead of fitting one
global `H`, it fits a separate `H` from the `K` nearest labelled
corners for each candidate cell. The per-candidate worst-residual
gate tolerates heavy radial distortion and multi-region perspective
where a single `H` breaks. Configured via `LocalExtensionParams`.

### Topological grid finder

`projective_grid::build_grid_topological` is an image-free,
axis-driven variant of the Shu/Brunton/Fiala 2009 topological grid
finder: Delaunay triangulation, edge classification by per-edge axis
match, triangle-pair → quad merge, and flood-fill `(i, j)` labelling.
See the [`topological`] module docs for the bibliographic entry.

[`topological`]: https://docs.rs/projective-grid/latest/projective_grid/topological/index.html

```rust
use projective_grid::{recover_topological_grid, LocalMergeParams, TopologicalParams};

let merged = recover_topological_grid(
    &positions,
    &axes_hints,
    &TopologicalParams::default(),
    &LocalMergeParams::default(),
)?;
```

See `docs/TOPOLOGICAL_PIPELINE.md` in the workspace for the per-stage
algorithm description and known limitations. The chessboard detector
selects between the two pipelines via
`DetectorParams::graph_build_algorithm`; ChArUco unconditionally
pins seed-and-grow because marker-internal corners poison the per-cell
axis test the topological path relies on.

## Inputs and outputs

| Stage | Input | Output |
|---|---|---|
| Cell-size estimate | `&[Point2<f32>]` | [`GlobalStepEstimate`] (`cell_size`, `confidence`, …) |
| Local-step refinement | per-corner positions + axes | `Vec<LocalStep<F>>` (for `square_find_inconsistent_corners_step_aware`) |
| Seed primitives | corner positions + quad indices | [`SeedOutput`] (`seed`, `cell_size`) |
| BFS-grow | positions + seed + validator + [`GrowParams`] | [`GrowResult`] (`labelled`, `holes`, `ambiguous`) |
| Boundary extension | positions + `GrowResult` + validator + [`ExtensionParams`] | [`ExtensionStats`] (residuals, attached, rejection counters) |
| Validation | labelled corners + [`ValidationParams`] | [`ValidationResult`] (blacklist + per-corner local-H residuals) |
| Rectification | labelled corners | [`SquareGridHomography`] (single global) or [`SquareGridHomographyMesh`] (per-cell) |

All public types re-exported at the crate root; the detailed module
layout sits under [`square`] and [`hex`].

## Configuration

Tuning knobs cluster into three groups. Defaults are chosen so that clean
synthetic grids "just work"; tune only when a specific input fails.

- **[`GrowParams`]** — `attach_search_rel` (search radius as a fraction
  of `cell_size`), `attach_ambiguity_factor`, and
  `boundary_search_factor` (open up the search when the target is being
  extrapolated outward instead of interpolated between two opposing
  labelled neighbours).
- **[`ExtensionParams`]** — `min_labels_for_h`,
  `max_median_residual_rel` / `max_residual_rel` (residual gate on the
  globally-fit H over the BFS-validated set), `search_rel`,
  `ambiguity_factor`, `max_iters`.
- **[`ValidationParams`]** — line-collinearity (`line_tol_rel`,
  `line_min_members`) and local-H (`local_h_tol_rel`) residual
  thresholds for the post-grow cleanup.

## Limitations

- **2D only.** Coordinates are `nalgebra::Point2<f32>`; no 3D support.
- **Roughly-square cells.** Strongly anisotropic aspect ratios (>3:1)
  degrade the local-step prediction; rescale the input cloud first.
- **Hex grids: geometry only.** `hex` ships D6 alignment + per-cell
  homography mesh + smoothness, but not seed-and-grow yet.
- **Heavy radial distortion.** A single global H can't fit fish-eye
  data; the H-residual gate refuses to extrapolate in that case
  (the boundary extension pass becomes a no-op). Use
  [`SquareGridHomographyMesh`] for per-cell rectification.

## Design notes

- **Local invariants, not global homography**, in BFS-grow: each step
  reasons about a target and its nearest neighbours, which is
  affine-locally valid even under moderate perspective. Per-neighbour
  finite-difference local-step prediction handles foreshortening as
  long as the labelled set has labels on both sides of the target.
- **Global H at the boundary.** When the target sits one step outside
  the labelled bbox, the local-step model is asymmetric and overshoots.
  The boundary-extension pass falls back to a globally-fitted homography
  for boundary cells, gated on a reprojection-residual check on the
  labelled set so it disables itself under non-planar / fish-eye
  conditions.
- **Undirected-angle circular means.** Any function averaging axis
  angles accumulates `(cos 2θ, sin 2θ)` and halves the resulting
  `atan2` — naive `(cos θ, sin θ)` averaging breaks at the 0°/180°
  seam. See [`circular_stats::refine_2means_double_angle`].
- **Plateau-aware peak picking.** When a physical direction's mass
  straddles a histogram-bin boundary, the smoothed peak is flat-topped
  across two adjacent bins. [`circular_stats::pick_two_peaks`] detects
  the plateau midpoint so axis estimates stay stable as input rotates.

## Related crates

- [calib-targets-chessboard][] — the reference consumer: invariant-first
  chessboard detector.
- [calib-targets-puzzleboard][] — self-identifying chessboard variant.
- [calib-targets][] — workspace facade with `detect_*` / `detect_*_best`.

[calib-targets]: https://docs.rs/calib-targets
[calib-targets-chessboard]: https://docs.rs/calib-targets-chessboard
[calib-targets-puzzleboard]: https://docs.rs/calib-targets-puzzleboard
[book-chapter]: https://vitalyvorobyev.github.io/calib-targets-rs/projective_grid.html
