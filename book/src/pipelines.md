# Pipelines

This section documents each target's **complete, end-to-end detection
pipeline** — one page per target type. Where the [Algorithms](algorithms.md)
section describes each building block in isolation, a pipeline page shows
how a particular target *composes* those blocks from a grayscale image (or
a pre-detected corner cloud) to a labelled, ID-carrying detection.

Each pipeline page **narrates and links** the canonical stage map that
lives next to the code. The crate-level `docs/PIPELINE.md` files are the
**source of truth**; these pages mirror them and must not diverge.

## The shared front-end

Every detector shares the same first three steps:

```text
┌───────────┐    ┌───────────┐    ┌───────────┐    ┌───────────┐
│  Image    │ -> │  ChESS    │ -> │ Target-   │ -> │ Labelled  │
│ (u8 gray) │    │ corners   │    │ specific  │    │ grid out  │
└───────────┘    │ (front-   │    │ detector  │    │           │
                 │  end)     │    │           │    │           │
                 └───────────┘    └───────────┘    └───────────┘
```

1. **Input image** — `image::GrayImage` or a `GrayImageView`. The facade
   helpers in `calib_targets::detect` accept either.
2. **Corner front-end** — the [ChESS X-junction](algo_chess_corners.md)
   detector via the `chess-corners` crate produces a raw corner cloud
   (sub-pixel position + two undirected axes, each with a 1σ angular
   uncertainty + a `strength` response magnitude). The workspace default is
   `calib_targets::detect::default_chess_config()`.
3. **Grid recovery** — every target then runs the *same* grid stack:
   [axis clustering](algo_axis_clustering.md) → the
   [topological grid finder](algo_topological_grid.md) →
   [recovery & validation](algo_recovery_validation.md). This is the
   chessboard pipeline, and it is the shared spine of all the others.
4. **Target-specific decode + IDs** — self-identifying targets add their
   own decoder (ArUco bits, PuzzleBoard edge codes) and ID assignment on
   top of the recovered grid.
5. **Output** — every detector produces a `TargetDetection` wrapping a
   `Vec<LabeledCorner>`; higher-level detectors wrap that in their own
   result struct with extra metadata (marker decodes, alignment, IDs). See
   [Understanding Results](output.md).

## The pages

| Pipeline | Composes | Source of truth |
|---|---|---|
| [Regular grid](pipeline_regular_grid.md) | clustering + topological grid + validation | `docs/algorithms/topological-grid-detection.md` |
| [Chessboard](pipeline_chessboard.md) | the full grid stack, precision-anchored | `crates/calib-targets-chessboard/docs/PIPELINE.md` |
| [PuzzleBoard](pipeline_puzzleboard.md) | chessboard grid + [edge-code decode](algo_puzzleboard_decode.md) | `crates/calib-targets-puzzleboard/docs/PIPELINE.md` |
| [ChArUco](pipeline_charuco.md) | chessboard grid + [ArUco decode](algo_aruco_decode.md) + [alignment](algo_charuco_alignment.md) | `crates/calib-targets-charuco/docs/PIPELINE.md` |
| [Marker board](pipeline_marker.md) | chessboard grid + 3-circle anchoring | `crates/calib-targets-marker/docs/PIPELINE.md` |

## One builder, everywhere

There is no grid-builder choice to make. `GraphBuildAlgorithm` is a
single-variant, `#[non_exhaustive]` enum (`Topological`) retained only as
a reserved config seam; the topological grid finder is the sole builder
for every target, including ChArUco. A config that carries a legacy value
is re-pinned to `Topological` on load.

## Output types

Output types are standardised in `calib-targets-core` as `TargetDetection`
with `LabeledCorner` values. The chessboard layer's labelling carries the
**precision contract** every target inherits: wrong `(i, j)` labels are
unrecoverable for downstream calibration, so the grid stage may *fail to
detect* a corner but must never deliver a wrong label. Higher-level crates
enrich that output with additional metadata (marker detections, rectified
views, per-corner IDs).
