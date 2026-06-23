# Chessboard detection — pipeline stage map

Concise stage-by-stage map of `calib-targets-chessboard`'s detector.
Each row in the stage table lists the stage's input, decision predicate,
output, dominant failure modes, and the `AdvancedTuning` knobs that
govern it. This is the working reference for diagnosing a detector
failure on a real image — start here before reading source.

The detector is **precision-anchored**: every stage that can attach a
new label runs an axis / parity / edge invariant, and the mandatory
final geometry check drops anything that slipped through. Wrong `(i, j)`
labels are unrecoverable for downstream calibration; missing corners are
acceptable. The asymmetry is the whole contract — a miss is fine, a
false positive is not.

## One builder

The detector ships a single grid builder. `DetectorParams::graph_build_algorithm`
is a single-variant, `#[non_exhaustive]` enum (`GraphBuildAlgorithm::Topological`)
retained only so the config schema stays stable if a future alternative
builder is added. There is no algorithm choice to make and no
target-family pinning: ChArUco, PuzzleBoard, and marker board all run
this same topological path through their nested `DetectorParams`.

The `(i, j)`-labelling itself comes from the **topological** grid finder
in `projective-grid` — `detect_grid_all`, the crate's sole grid builder
(Delaunay triangulation + an axis-driven cell test, image-free below
ChESS). The chessboard crate owns everything
around it: the prefilter, axis clustering (including the DiskFit
slot-coherence repair), the recall boosters, the mandatory geometry
check, and output canonicalisation. The generic grid-finder internals
are documented separately in
[`docs/algorithms/topological-grid-detection.md`](../../../docs/algorithms/topological-grid-detection.md).

**Fallible construction.** `Detector::new(params) -> Result<Self,
ChessboardParamsError>` validates params up front. No combination the
public surface can express is rejected today (`ChessboardParamsError` is
a reserved, uninhabited seam); the fallible signature is retained so a
future validation can be added without a breaking change.

**Two surviving knob layers.** Only four stable top-level
`DetectorParams` fields (`graph_build_algorithm`, `min_labeled_corners`,
`max_components`, `min_corner_strength`) are part of the public config
contract. All per-stage knobs live behind the opt-in, **non-semver**
`DetectorParams::advanced` (`AdvancedTuning`); when unset, every knob
holds its default. The Knobs column below names `AdvancedTuning` fields
unless prefixed `params.` (a stable top-level field).

---

## Topological pipeline

The orchestrator is `pipeline::detect_all_topological` (production) /
`pipeline::trace_topological` (compact serializable trace over the same
production path — no separate timed implementation). The six logical
stages map onto the `pipeline/` module tree as follows.

### Stage table

| # | Name | Module | In | Out | Decision | Failure modes | Knobs |
|---|---|---|---|---|---|---|---|
| 1 | `prefilter` | `inputs.rs` | `&[ChessCorner]` from ChESS | per-corner usable flag; weak corners kept as positions with no-information axes | `strength ≥ min_corner_strength` **and** `fit_rms ≤ max_fit_rms_ratio · contrast` (skipped when `contrast ≤ 0` or ratio is `∞`) | very-low-contrast frames; saturated edges (sigma = π → no info); marker misdetections | `params.min_corner_strength`, `max_fit_rms_ratio` |
| 2 | `cluster_axes` | `cluster/` | `Strong` corners' `axes` | `ClusterCenters {Θ₀ ≤ Θ₁}` in `[0, π)` + per-corner `Canonical`/`Swapped`/`NoCluster` label | generic `projective_grid::cluster`: orientation histogram + plateau-aware peak picking + double-angle `(cos 2θ, sin 2θ)` 2-means; per-corner slot assignment admitted iff `max(d_a0, d_a1) ≤ cluster_tol_deg + cluster_sigma_k·max(σ)`; then the **DiskFit slot-coherence repair** (`slot_coherence.rs`) — see below | histogram bias from marker-internal corners pulling centres a few degrees off true axes; uniform DiskFit antipodal-sector flips breaking the parity invariant | `num_bins`, `max_iters_2means`, `cluster_tol_deg`, `cluster_sigma_k`, `peak_min_separation_deg`, `min_peak_weight_fraction` |
| 3 | `topological_grid` | `projective-grid` via `mod.rs` | oriented features (positions + dual axes) + cluster centres as an axis hint | connected labelled `(i, j) → source_index` components | `detect_grid_all` (the sole grid builder): Delaunay classify → quad assembly → axis-driven cell-test walk → facade `merge_components_local`. The facade's own post-build validation / residual drop / recovery are disabled (`+∞`, `Off`) — the chessboard owns those downstream | axis-driven cell test admitting a spurious edge across a marker; foreshortening near the band edges | `topological` (`TopologicalParams`) |
| 4 | `recover_components` | `recover.rs` + `boosters.rs` | facade-merged components + clustered corners | per-component grid extended by booster fills, then re-merged in label space | per component: estimate cell size from labelled cardinal edges, then `boosters.rs` (interior gap fill + line extrapolation via `fill_grid_holes`, with a per-axis **directional edge scale** since the visible component can be anisotropic before boundaries fill); each addition re-runs the same axis / parity / edge-slot-swap invariants as the walk; capped by `max_booster_iters`. Optional weak-cluster rescue re-admits `NoCluster` corners within `weak_cluster_tol_deg`. Then `merge_components_local` reunites components | over-flag of borderline corners; line extrapolation projecting past the true board edge | `attach_search_rel`, `attach_axis_tol_deg`, `attach_ambiguity_factor`, `step_tol`, `edge_axis_tol_deg`, `enable_weak_cluster_rescue`, `weak_cluster_tol_deg`, `max_booster_iters`, `component_merge` |
| 5 | `final_geometry_check` | `geometry_check.rs` | final labelled set | drop list + `detection_refused` flag | **mandatory, can only DROP** (never add or relabel): (a) shared `validate` (line collinearity + local-H residual) with **looser** `geometry_check_*` tolerances — catches gross mislabels (full-cell / diagonal ≈ 1.4-cell residual) without flagging accepted perspective drift; (b) the direct topological wrong-label check (interior skipped-corner edges + duplicate-pixel labels); (c) largest-cardinally-connected-component filter, dropping isolated leaks outside the main grid. Refuses the detection if survivors `< min_labeled_corners` | strict per-edge length tests over-flag distorted boards (kept loose deliberately); single-component constraint is the chessboard contract | `geometry_check_line_tol_rel`, `geometry_check_local_h_tol_rel`, `line_min_members`, `validate_step_aware`, `enable_final_edge_shape_check` |
| 6 | `output` | `output.rs` | surviving labelled set | `ChessboardDetection { grid_directions, cell_size, corners: ChessboardCorner[] }` | build a `projective_grid::LabelledGrid` from the labelled set and call `LabelledGrid::normalize()` (rebase min → `(0, 0)`; canonicalise so `+i ≈ +x`, `+j ≈ +y`; stable `(j, i)` sort — all owned by projective-grid), then adapt the normalized lattice `Coord{u,v}` to the workspace `GridCoords{i,j}` | — | `params.min_labeled_corners` |

### Key invariants

These hold across every stage that can attach a label, and are what make
a miss recoverable but a false positive impossible:

- **Two grid directions.** Clustering recovers `{Θ₀, Θ₁}` (≈ 90° apart)
  as the only global axis prior. All axis means use the undirected
  `(cos 2θ, sin 2θ)` accumulation and halve the `atan2` result — there is
  no `Corner::orientation`, only `Corner.axes: [AxisEstimate; 2]`.
- **Parity / edge-slot-swap.** A corner's k=4 cardinal neighbours sit at
  the *opposite* axis-slot parity by construction. Every attachment (walk
  cell-test, booster fill) checks that the candidate edge crosses a
  slot-swap boundary, which is why a diagonal or skipped-corner
  attachment is rejected structurally rather than by a magnitude
  threshold.
- **Geometry check can only subtract.** Stage 5 never adds or relabels a
  corner; it only drops or refuses. A corner that survives every stage
  has been *proven* to sit at a real intersection.
- **Non-negative labels.** Output rebases the labelled bounding-box
  minimum to `(0, 0)` — a hard invariant for overlay / calibration
  consumers.

### DiskFit slot-coherence repair (Stage 2)

`slot_coherence::fix_axis_slot_coherence` is a live recall safety-net,
not dead code. The upstream `chess-corners` detector exposes two
orientation modes (still selectable via the facade / Studio / bench):

- Under **`RingFit`** axis-slot ordering is consistent by construction;
  the cluster split is ~50/50, the imbalance gate never fires, and this
  pass is a **no-op**.
- Under **`DiskFit`** the axes-fitter can uniformly pick the wrong
  antipodal dark sector, reversing a corner's `(axes[0], axes[1])`
  ordering relative to the board and globally breaking the parity
  invariant the walk and edge-ok rules depend on. The pass detects this
  by a gross-imbalance gate (minority class < 22 %), then BFS-2-colours
  the clustered corners at cell spacing and swaps the two `AxisEstimate`
  slots of whichever corners disagree.

It is precision-safe by construction: a bipartite-quality gate aborts
the pass unless the 2-colouring is essentially perfect, so it can only
add recall, never a wrong label. The slot swap is the load-bearing
mutation — every downstream consumer reads `axes[0]` vs `axes[1]`, so
swapping is equivalent to re-clustering with corrected ordering.

### Multi-component dispatch

`Detector::detect_all` is the multi-board entry point: it can return
several `ChessboardDetection`s (up to `params.max_components`) when one
image contains physically distinct grids. Within a single image, the
topological facade already produces and merges connected components, so a
single physical board that the grid split into disjoint sub-grids
(e.g. ChArUco rows separated by markers) is reunited in label space by
the Stage-4 `merge_components_local`. The chessboard precision contract
is preserved per emitted component.

---

## What lives where

The lattice-general logic lives in `projective-grid`; the chessboard
crate keeps the ChESS glue and slot-parity semantics.

- **`projective-grid`** (image-free, no internal workspace deps):
  - `cluster` — the generic axis-clustering math (histogram + peak
    picking + double-angle 2-means), preserving the `(cos 2θ, sin 2θ)`
    circular-mean contract.
  - `topological` — the axis-driven grid finder
    (`detect_grid_all`, the sole grid builder): Delaunay
    classify → quads → walk → facade merge.
  - `shared::{merge, fit, validate, fill, grow}` — `merge_components_local`,
    the projective fit + residual helper, the lattice-general drop
    filters (line / local-H validation, topological wrong-label drops,
    largest-component filter), and the `fill_grid_holes` engine plus the
    `SquareAttachPolicy` seam where caller-specific invariants enter.
- **`calib-targets-chessboard`** (chessboard-specific): the strength /
  fit prefilter (`inputs.rs`), axis clustering glue + the
  `slot_coherence` DiskFit parity repair (`cluster/`), the recall
  boosters with parity + directional edge scale (`boosters.rs`), the
  per-component recovery + post-booster merge (`recover.rs`), the
  mandatory geometry-check **orchestration** (the drop filters
  themselves live in `shared::validate`; the chessboard sequences them),
  the output adapter (`output.rs`) that maps the normalized lattice
  `Coord{u,v}` back to the workspace `GridCoords{i,j}` — the rebase +
  canonicalise + sort *algorithm* itself now lives in
  `projective_grid::LabelledGrid::normalize` — and the multi-component
  dispatch.

## Cross-references

- [`docs/algorithms/topological-grid-detection.md`](../../../docs/algorithms/topological-grid-detection.md)
  — the generic `projective-grid` topological builder in full (core +
  chessboard input adapter + recovery layer).
- `crates/projective-grid/src/topological/` — the projective-grid
  topological core, independent of chessboard semantics.
- `crate`-level rustdoc (`src/lib.rs`) — the canonical six-stage summary
  table and a runnable quickstart.
- CLAUDE.md "Evidence-driven debugging" — every detector-failure
  conclusion must be tied to measured numbers / per-corner facts, never a
  plausible narrative; `bench check`'s `pos=` does **not** validate new
  `(i, j)` labels, so overlays + an independent geometry check are
  mandatory.
- CLAUDE.md "Corner orientation contract (axes-only)" — the axis
  convention the cluster code and per-edge gates rely on.
