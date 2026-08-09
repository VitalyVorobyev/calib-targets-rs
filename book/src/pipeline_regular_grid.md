# Regular grid pipeline

> Composes: [axis clustering](algo_axis_clustering.md) →
> [topological grid finder](algo_topological_grid.md) →
> [recovery & validation](algo_recovery_validation.md).
> Source of truth: `docs/algorithms/topological-grid-detection.md`.
> Public surface: [The Grid Model](projective_grid.md).

The regular grid pipeline is the **target-free** end-to-end path: a cloud
of oriented point features in, a labelled `(i, j)` lattice out, with no
image and no calibration vocabulary. It is the standalone
[`projective-grid`](projective_grid.md) crate, and it is the spine every
target detector builds on. Reach for it directly when you have a grid that
is *not* one of the workspace's named targets — a laser-dot cloud, a
scanned form, a photographed board game.

## End-to-end stages

```text
OrientedFeature<2>[]  (positions + two undirected axes each)
 →  axis clustering        recover global directions {Θ₀, Θ₁}   (optional hint)
 →  topological grid       Delaunay → classify → quads → walk    (the builder)
 →  validation + fit       line / local-H / residual gate
 →  GridDetection          labelled (i, j) component(s) + projective fit
```

1. **Axis clustering (optional).** If the caller can supply the two global
   grid directions, they gate the topological usability prefilter. For the
   bare `projective-grid` entry points the caller may skip this and let the
   detector synthesize axes from neighbour geometry.
2. **Topological grid finder.** The sole builder: Delaunay triangulation →
   axis-driven edge classification → triangle-pair → quad merge →
   flood-fill `(i, j)` walk → orchestration into components. See the
   [algorithm page](algo_topological_grid.md) for each stage.
3. **Per-component validation + projective fit.** A pattern-agnostic
   geometry gate (line collinearity, local-H residual, edge-length band)
   plus a [projective fit](algo_homography.md) with a `max_residual_px`
   gate. For the bare grid crate this stage is *active* (it is the
   precision gate); the chessboard wrapper disables it and substitutes its
   own mandatory geometry check.
4. **Output.** A `GridDetection` per component: an accepted `LabelledGrid`
   plus a mandatory `LatticeFit`. Rejections and stage snapshots are an
   opt-in diagnostics contract.

## Public surface

The detection input is the `Evidence` enum — it names exactly how much
orientation the caller can supply (`Positions`, `Oriented1`, `Oriented2`,
`Oriented3`). The native square shape is `Oriented2`; less-oriented kinds
synthesize the missing axes up front. `detect_grid` returns the largest
component; `detect_grid_all` returns all of them. A separate
`check_consistency` entry point scores *pre-labelled* features against a
single projective fit. The full surface — `DetectionRequest`,
`GridDetection`, the diagnostics seam, and the worked example — is documented
in [The Grid Model](projective_grid.md) and the
[Regular Grid Detection example](example_regular_grid.md).

## Hex lattices

The same pipeline detects a hexagonal point lattice on the topological
path: the Delaunay triangles *are* the unit cells, so the
diagonal/quad-merge stage is bypassed and the axial `(q, r)` walk runs
directly, with the projective-fit back-half shared.

## Failure modes

| Symptom | Likely stage | What it means |
|---|---|---|
| `GridError::InsufficientEvidence` | input | Too few features to assemble a 2×2 seed cell. |
| `GridError::DegenerateGeometry` | input | Coincident or collinear points; no usable lattice spread. |
| `GridError::UnsupportedCombination` | dispatch | The `(lattice, evidence)` pair has no algorithm (e.g. `Hex` + `Oriented1`). Returned rather than guessed. |
| Few entries, many `Unlabelled` rejects | topological walk | Noisy or low-resolution axes — the classifier could not build enough confident grid edges. |
| `ValidationDropped` rejects | validation | Placed by the walk but failed line / local-H / edge-band; a gross mislabel was caught. |
| `ResidualTooHigh` rejects | fit | Reprojection residual over `max_residual_px`; loosen only if the geometry is genuinely distorted. |

## Tuning

For ordinary callers, `DetectionParams` intentionally has one stable knob:

- **`max_residual_px`** — the fit residual gate. Raise on genuinely
  distorted captures; it is the precision lever, so prefer the smallest
  value that still recovers the grid.

Stage-specific topological, validation, and recovery controls are available
only by attaching `projective_grid::expert::DetectionTuning`. This keeps the
default configuration compact while preserving an explicit seam for detector
builders and reproducible evidence campaigns.

When this pipeline runs *inside* a target detector, these knobs are mostly
set by the wrapper (the chessboard wrapper disables the validate stage and
owns its own checks); see [Tuning the Detector](tuning.md).

## Cross-references

- [The Grid Model](projective_grid.md) — the full public surface and a
  worked example.
- [Chessboard pipeline](pipeline_chessboard.md) — the same spine with the
  chessboard precision discipline layered on.
- `docs/algorithms/topological-grid-detection.md` — the generic core in full.
