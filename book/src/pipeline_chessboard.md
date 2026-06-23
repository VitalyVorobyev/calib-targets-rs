# Chessboard pipeline

> Composes the full grid stack: [ChESS corners](algo_chess_corners.md) →
> [axis clustering](algo_axis_clustering.md) →
> [topological grid](algo_topological_grid.md) →
> [recovery & validation](algo_recovery_validation.md).
> **Source of truth:** `crates/calib-targets-chessboard/docs/PIPELINE.md`.
> Crate reference: [The Chessboard Detector](chessboard.md).

The chessboard detector takes a cloud of [ChESS X-junction
corners](algo_chess_corners.md) and produces an integer-labelled grid
`(i, j) → image position`. It is the **shared spine** of every other
target pipeline, and it is **precision-anchored**: every stage that can
attach a label runs an axis / parity / edge invariant, and the mandatory
final geometry check drops anything that slipped through. Wrong `(i, j)`
labels are unrecoverable for downstream calibration; missing corners are
acceptable — that asymmetry is the whole contract.

## The six stages

The orchestrator is `pipeline::detect_all_topological`. The canonical
stage map (mirror of the crate `docs/PIPELINE.md`):

| # | Stage | In → Out | What it does |
|---|---|---|---|
| 1 | `prefilter` | ChESS corners → usable-flagged corners | Keep a corner iff `strength ≥ min_corner_strength` **and** `fit_rms ≤ max_fit_rms_ratio · contrast`. Weak corners are kept as positions with no-information axes (so indices stay stable) but cannot vote. |
| 2 | `cluster_axes` | strong corners' axes → `{Θ₀ ≤ Θ₁}` + per-corner slot label | The generic [axis clustering](algo_axis_clustering.md) (histogram + plateau peak picking + double-angle 2-means), then the **DiskFit slot-coherence repair** (below). |
| 3 | `topological_grid` | oriented features + cluster centres → labelled components | The [topological grid finder](algo_topological_grid.md) (`detect_grid_all`); its own post-build validation / residual / recovery are disabled — the chessboard owns those downstream. |
| 4 | `recover_components` | merged components → boosted, re-merged grid | Per-component cell-size estimate, then the [recovery boosters](algo_recovery_validation.md) (interior gap fill + line extrapolation with a per-axis directional edge scale), optional weak-cluster rescue, then `merge_components_local`. Every addition re-runs the axis / parity / edge-slot-swap invariants. |
| 5 | `final_geometry_check` | labelled set → drop list + refuse flag | **Mandatory, can only DROP.** The shared [`drop_set`](algo_recovery_validation.md) precision pass: line collinearity + local-H residual + the topological wrong-label checks (skipped-corner edges, duplicate-pixel labels, frontier line-spacing smoothness) + the largest-component filter. Refuses if survivors `< min_labeled_corners`. |
| 6 | `output` | surviving set → `ChessboardDetection` | Build a `LabelledGrid` and call [`normalize()`](algo_recovery_validation.md) (rebase min → `(0, 0)`; canonicalise `+u ≈ +x`, `+v ≈ +y`; stable `(v, u)` sort). The lattice `Coord{u,v}` is the canonical grid-coordinate type, so it is copied straight onto each output corner. |

> **Note on the output shape.** `ChessboardDetection` is `Coord{u,v}`-based;
> what moved is *where* normalization
> lives — the rebase + canonicalise + sort algorithm is now owned by
> `projective_grid::LabelledGrid::normalize`, with the output stage merely
> calling it.

## Key invariants

These hold across every stage that can attach a label, and are what make a
miss recoverable but a false positive impossible:

- **Two grid directions.** Clustering recovers `{Θ₀, Θ₁}` (≈ 90° apart) as
  the only global axis prior. All axis means use the undirected
  `(cos 2θ, sin 2θ)` accumulation — there is no `Corner::orientation`,
  only `Corner.axes: [AxisEstimate; 2]`.
- **Parity / edge-slot-swap.** A corner's four cardinal neighbours sit at
  the *opposite* axis-slot parity by construction. Every attachment checks
  that the candidate edge crosses a slot-swap boundary, so a diagonal or
  skipped-corner attachment is rejected *structurally*, not by a magnitude
  threshold.
- **Geometry check can only subtract.** Stage 5 never adds or relabels; a
  corner that survives every stage has been *proven* to sit at a real
  intersection.
- **Non-negative labels.** Output rebases the labelled bbox minimum to
  `(0, 0)`.

## DiskFit slot-coherence repair (Stage 2)

The [ChESS](algo_chess_corners.md) detector's `DiskFit` mode can uniformly
pick the wrong antipodal dark sector, reversing a corner's
`(axes[0], axes[1])` ordering and breaking the parity invariant globally.
A live recall safety-net (`slot_coherence`) detects this with a
gross-imbalance gate, BFS-2-colours the clustered corners at cell spacing,
and swaps the two `AxisEstimate` slots of whichever corners disagree. A
bipartite-quality gate aborts the pass unless the 2-colouring is
essentially perfect, so it can only add recall, never a wrong label. Under
`RingFit` the split is already ~50/50 and the pass is a no-op.

## Multi-component dispatch

`Detector::detect_all` is the multi-board entry point: it returns several
`ChessboardDetection`s (up to `max_components`) when one image contains
physically distinct grids. Within a single image, the topological facade
already merges connected components, so a single physical board split into
disjoint sub-grids (e.g. ChArUco rows separated by markers) is reunited in
label space by the Stage-4 merge. The precision contract holds per emitted
component. The workspace explicitly does **not** support multiple separate
physical boards in one frame.

## Failure modes

Identify the stage from the serializable topological trace
(`pipeline::trace_topological`, layered over the production path) and the
final-check `GeometryCheckTrace` drop counters, then consult:

| Symptom | Likely stage | Knob to try | Notes |
|---|---|---|---|
| No detection, no grid directions | Stage 2 (clustering) | `min_peak_weight_fraction`, `peak_min_separation_deg` | The two grid axes never separated — common on very-bad-light frames. |
| No cell size / no seed | Stage 3 (topological) | `detect_chessboard_best` with `sweep_default()` | No quad assembled. Builder tolerances are internal. |
| Very few corners | Stage 4 (recover) | `attach_search_rel`, `attach_axis_tol_deg`, `step_tol`, `edge_axis_tol_deg` | Grid grew but couldn't extend — common on heavily distorted views. |
| Many dropped corners | Stage 5 (geometry check) | `geometry_check_local_h_tol_rel` | Invariants found outliers; check the drop reasons. |
| **Wrong `(i, j)` labels** | **never** | — | File a bug. The precision contract has been violated; do not tune around it. |

## Tuning

`DetectorParams` splits into a **stable core** (`graph_build_algorithm`
[single-variant], `min_labeled_corners`, `max_components`,
`min_corner_strength`) plus an opt-in, **non-semver** `advanced`
(`AdvancedTuning`) block of per-stage knobs. Leave `advanced` unset unless
a specific input fails and you have evidence for the change. For
challenging images use `detect_chessboard_best` with
`DetectorParams::sweep_default()` (three configs varying only
recall-affecting tolerances; all preserve the precision invariants). The
full knob table is in [Tuning the Detector](tuning.md) and the
[chessboard crate chapter](chessboard.md).

## Cross-references

- [The Chessboard Detector](chessboard.md) — the full invariant stack,
  the topological-trace diagnostics surface, and a quickstart.
- `crates/calib-targets-chessboard/docs/PIPELINE.md` — the canonical stage
  map this page mirrors.
- The downstream pipelines that build on this spine:
  [PuzzleBoard](pipeline_puzzleboard.md), [ChArUco](pipeline_charuco.md),
  [Marker board](pipeline_marker.md).
