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
   detector via the `chess-corners` crate. Produces a raw corner cloud —
   per-corner position, two axis-angle estimates, strength, and fit
   residuals — which the facade adapter hands to each target detector as
   its own input type (e.g. `calib_targets_chessboard::ChessCorner`).
   The workspace's default config is
   `calib_targets::detect::default_chess_config()`.
3. **Target-specific detector** — see the dedicated chapters:
   - [Chessboard](chessboard.md) — invariant-first detector
     precision-by-construction on our private regression dataset
     (high detection rate, zero wrong labels).
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

The chessboard detector runs as a sequence of named stages
(orchestrated by `pipeline::run_pipeline`, with one module per stage
group under `crates/calib-targets-chessboard/src/pipeline/`). The
invariant-first framing means every stage emits a more-constrained
subset of the previous stage's output, with no backtracking that
would compromise precision:

| Stage | Name | Responsibility | Reference |
|---|---|---|---|
| 1 | `prefilter` | Drop corners failing strength / fit-quality / axes-validity gates. | `pipeline::prefilter` |
| 2 | `cluster_axes` | Recover the two global grid-direction centres `{Θ₀, Θ₁}` via histogram + 2-means; label each corner canonical or swapped. | [`projective_grid::circular_stats`](projective_grid.md) |
| 3 | `estimate_cell_size` | Cross-cluster nearest-neighbour mode → global cell size `s` (sanity prior only). | `cell_size::estimate_cell_size` |
| 4 | `find_seed` | Pick a 2×2 quad passing every geometric invariant; refine `s` from the seed edges. | `seed::find_seed` |
| 5 | `grow` | BFS over the `(i, j)` boundary with the full invariant stack at every attachment. | [`projective_grid::square::grow`](projective_grid.md) |
| 6 | `extend_boundary` | Homography-based extension (global or per-candidate local-H) outward and into interior holes. | `pipeline::extension` |
| 7 | `fix_partial_slot_flip` | Re-check axis-slot-swap parity after extension; flip disagreeing entries. | `pipeline::extension` |
| 8 | `rescue_no_cluster` | Re-admit `Strong` / `NoCluster` corners within the rescue tolerance via local-H prediction. | `pipeline::extension` |
| 9 | `refit_cluster_centers` | Re-estimate `{Θ₀, Θ₁}` from labelled corners; on a large shift, re-run extension + rescue. | `pipeline::refit` |
| 10 | `validate` | Line collinearity + local-H residual checks; blacklist outliers and restart from `find_seed`. | [`projective_grid::square::validate`](projective_grid.md) |
| 11 | `apply_boosters` | Recall boosters: interior gap fill + line extrapolation + component merge. | `boosters::apply_boosters` |
| 12 | `final_geometry_check` | Mandatory precision gate: per-edge length + axis-slot parity + largest cardinal component. Can only drop corners. | `pipeline::geometry_check` |

Stages 4–10 run inside a blacklist loop — each iteration the validator
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
