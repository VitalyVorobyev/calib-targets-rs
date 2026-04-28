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
projective-grid = "0.7"
nalgebra = "0.34"
```

## Pipeline at a glance

The crate centres on a five-stage pipeline. Pattern-specific gates
(parity, axis-cluster, marker rules, …) plug into the
[`square::grow::GrowValidator`] trait; the geometric machinery underneath
is generic.

```rust
use projective_grid::square::{
    grow::{bfs_grow, GrowParams, GrowValidator, Seed},
    grow_extension::{extend_via_global_homography, ExtensionParams},
    validate::{validate, LabelledEntry, ValidationParams},
};

// 1. Caller supplies: corner positions, a 2×2 seed quad, a validator
//    that knows the pattern's invariants, and a cell-size estimate.
let positions: Vec<nalgebra::Point2<f32>> = /* … */;
let seed: Seed = /* … */;
let cell_size: f32 = /* … */;
let validator: &impl GrowValidator = /* … */;

// 2. Stage 5: BFS-grow with adaptive local-step prediction.
let mut grow = bfs_grow(&positions, seed, cell_size, &GrowParams::default(), validator);

// 3. Stage 6 (optional): extend the labelled set via a globally-fit
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

// 4. Stage 7: line / local-H residual validation produces a blacklist of
//    outlier corner indices to drop and re-grow.
let entries: Vec<LabelledEntry> = /* (corner_idx, pixel, grid) per labelled */;
let _result = validate(&entries, cell_size, &ValidationParams::default());
```

The chessboard / ChArUco / PuzzleBoard detectors in the workspace
implement their pattern-specific `GrowValidator` and call the same
machinery; their orchestrators iterate Stage 5–7 with a blacklist until
the labelled set converges, then run Stage 6, then re-validate.

### Alternative Stage 6: local homography

`extend_via_local_homography` (in `square::grow_extension`) is an
opt-in replacement for `extend_via_global_homography`. Instead of
fitting one global H, it fits a separate H from the K nearest labelled
corners for each candidate cell. The per-candidate trust gate tolerates
heavy radial distortion and multi-region perspective where a single H
breaks. Configure it via `LocalExtensionParams`.

### Topological pipeline

For images where corners arrive in a dense, nearly-regular cloud, the
topological pipeline is an image-free alternative to seed-and-grow:

```rust
use projective_grid::{build_grid_topological, merge_components_local,
    ComponentInput, LocalMergeParams, TopologicalParams};

let params = TopologicalParams::default();
let topo = build_grid_topological(&positions, &axes_hints, &params)?;

// merge_components_local reunites partial components (shared by both pipelines).
let views: Vec<ComponentInput<'_>> = topo.components.iter()
    .map(|c| ComponentInput { labelled: &c.labelled, positions: &positions })
    .collect();
let merged = merge_components_local(&views, &LocalMergeParams::default());
```

See `docs/TOPOLOGICAL_PIPELINE.md` in the workspace for a detailed
description of the algorithm and known limitations.

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
  (Stage 6 becomes a no-op). Use [`SquareGridHomographyMesh`] for per-cell
  rectification.

## Design notes

- **Local invariants, not global homography**, in BFS-grow: each step
  reasons about a target and its nearest neighbours, which is
  affine-locally valid even under moderate perspective. Per-neighbour
  finite-difference local-step prediction handles foreshortening as
  long as the labelled set has labels on both sides of the target.
- **Global H at the boundary.** When the target sits one step outside
  the labelled bbox, the local-step model is asymmetric and overshoots.
  Stage 6 falls back to a globally-fitted homography for boundary
  cells, gated on a reprojection-residual check on the labelled set so
  it disables itself under non-planar / fish-eye conditions.
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
