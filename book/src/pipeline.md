# Pipeline Overview

Every detector in the workspace shares the same high-level workflow:
take a grayscale image (or a pre-detected corner cloud), produce a
`TargetDetection` with labelled `(i, j)` grid coordinates, logical
marker IDs (where applicable), and rectification-ready pixel
positions.

---

## Shared stages

```text
┌───────────┐    ┌───────────┐    ┌───────────┐    ┌───────────┐
│  Image    │ -> │  ChESS    │ -> │ Target-   │ -> │ Labelled  │
│ (u8 gray) │    │ corners   │    │ specific  │    │ grid out  │
└───────────┘    │ (front-   │    │ detector  │    │           │
                 │  end)     │    │           │    │           │
                 └───────────┘    └───────────┘    └───────────┘
```

1. **Input image** — `image::GrayImage` or a `GrayImageView`. The
   facade helpers in `calib_targets::detect` accept either.
2. **Corner front-end** — [ChESS X-junction](https://www.cl.cam.ac.uk/research/rainbow/projects/chess/)
   detector via the `chess-corners` crate. Produces a `Vec<Corner>` —
   per-corner position, two axis-angle estimates, strength, and fit
   residuals. The workspace's default config is
   `calib_targets::detect::default_chess_config()`.
3. **Target-specific detector** — see the dedicated chapters:
   - [Chessboard](chessboard.md) — invariant-first detector
     (119 / 120 detections, 0 wrong labels on the canonical 120-snap
     regression dataset).
   - [ChArUco](charuco.md) — chessboard detector + ArUco marker
     decoding + alignment.
   - [PuzzleBoard](puzzleboard.md) — chessboard detector + edge-dot
     decoder.
   - [Marker board](marker.md) — ChESS checker corners + 3-circle
     marker anchoring.
4. **Output** — every detector produces a `TargetDetection` wrapping
   a `Vec<LabeledCorner>`. Higher-level detectors (ChArUco,
   PuzzleBoard) wrap that in their own result struct with extra
   metadata (marker decodes, alignment, per-corner IDs).

---

## Chessboard detector internals

The chessboard detector itself runs eight internal stages. The
invariant-first framing means every stage emits a more-constrained
subset of the previous stage's output, with no backtracking that
would compromise precision:

| Stage | Input | Output | Reference |
|---|---|---|---|
| 1. Pre-filter | raw `Corner` array | `CornerStage::Strong` corners (strength + fit-quality pass) | `cluster::build_histogram` |
| 2. Global grid directions | axes histograms | two centers `(Θ₀, Θ₁)` via plateau peaks + double-angle 2-means | [`projective_grid::circular_stats`](projective_grid.md) |
| 3. Per-corner label | each `Strong` corner's axes vs `(Θ₀, Θ₁)` | `CornerStage::Clustered { label }` with `Canonical`/`Swapped` parity | `cluster::assign_corner` |
| 4. Cell size | `Clustered` cross-cluster NN distances | `cell_size: f32` estimate | **derived inside Stage 5**; global scalar kept only as a sanity prior |
| 5. Seed | clustered corners + cluster centers | 2×2 quad + `cell_size` = mean of seed edges | `seed::find_seed` |
| 6. Grow | seed + candidate pool | labelled `(i, j) → idx` map via BFS + prediction averaging | [`projective_grid::square::grow`](projective_grid.md) |
| 7. Validate | labelled map | blacklist via line collinearity + local-H residuals | [`projective_grid::square::validate`](projective_grid.md) |
| 8. Recall boosters | labelled map + remaining clustered corners | additional admits via gap-fill, line extrapolation, component merge | `boosters::apply_boosters` |

Stages 5-7 run inside a blacklist loop — each iteration the validator
may reject outliers; the pipeline re-seeds on the remaining set.
Capped by `DetectorParams::max_validation_iters` (default 3).

See the [Chessboard Detector chapter](chessboard.md) for the full
invariant stack and failure-mode analysis.

---

## Which crate does what

The chessboard detector **algorithm** is split across two crates:

- [`projective-grid`](projective_grid.md) owns the pattern-agnostic
  machinery — BFS growth, KD-tree candidate search, circular-
  histogram peak picking (plateau-aware), double-angle 2-means,
  line / local-H validation. No calibration-specific dependencies;
  useful standalone.
- [`calib-targets-chessboard`](https://docs.rs/calib-targets-chessboard)
  supplies the chessboard-specific pieces that plug into the generic
  trait surface: ChESS-axis-based clustering, `ClusterLabel` parity,
  per-axis-slot edge validation, boosters. Orchestrates the
  end-to-end pipeline.

Output types are standardised in `calib-targets-core` as
`TargetDetection` with `LabeledCorner` values. Higher-level crates
enrich that output with additional metadata (marker detections,
rectified views, per-corner IDs).
