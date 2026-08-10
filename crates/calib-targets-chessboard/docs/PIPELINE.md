# Chessboard detection — pipeline stage map

Concise stage-by-stage map of `calib-targets-chessboard`'s detector.
Each row in the stage table lists the stage's input, decision predicate,
output, dominant failure modes, and the `ChessboardAdvancedTuning` knobs that
govern it. This is the working reference for diagnosing a detector
failure on a real image — start here before reading source.

The detector is **precision-anchored**: every stage that can attach a
new label runs an axis / parity / edge invariant, and the mandatory
final geometry check drops anything that slipped through. Wrong `(i, j)`
labels are unrecoverable for downstream calibration; missing corners are
acceptable. The asymmetry is the whole contract — a miss is fine, a
false positive is not.

## One builder

The detector ships a single grid builder and exposes no algorithm selector.
ChArUco, PuzzleBoard, and marker board all run this same topological path
through their nested chessboard parameters.

The `(i, j)`-labelling itself comes from the **topological** grid finder
in `projective-grid` — the curated
`expert::square::assemble_oriented2_components` builder seam (Delaunay
triangulation + an axis-driven cell test, image-free below ChESS). The
chessboard crate owns everything around it: the prefilter, axis clustering,
parity alignment, recall boosters, mandatory geometry check, and output
canonicalisation. The generic grid-finder internals
are documented separately in
[`docs/algorithms/topological-grid-detection.md`](../../../docs/algorithms/topological-grid-detection.md).

**Fallible construction.** `Detector::new(params) -> Result<Self,
ChessboardParamsError>` validates params up front. No combination the
public surface can express is rejected today (`ChessboardParamsError` is
a reserved, uninhabited seam); the fallible signature is retained so a
future validation can be added without a breaking change.

**Two surviving knob layers.** Only three stable top-level
`ChessboardParams` fields (`min_labeled_corners`, `max_components`,
`min_corner_strength`) are part of the public config
contract. All per-stage knobs live behind the opt-in, **non-semver**
`DetectorParams::advanced` (`ChessboardAdvancedTuning`); when unset, every knob
holds its default. The Knobs column below names `ChessboardAdvancedTuning` fields
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
| 1 | `prefilter` | `inputs.rs` | `&[ChessCorner]` from ChESS | per-corner usable flag; weak corners kept as positions with no-information axes | `strength ≥ min_corner_strength` **and** `max(σ₀, σ₁) ≤ topological.axis_align_tol_rad` (`pipeline::axis_admission_sigma` — derived from the Stage 3 tolerance, not a knob of its own) | very-low-contrast frames; saturated edges (sigma = π → no info); marker misdetections | `params.min_corner_strength` (the σ half follows `axis_align_tol_rad`) |
| 2 | `cluster_axes` | `cluster/` | `Strong` corners' `axes` | `ClusterCenters {Θ₀ ≤ Θ₁}` in `[0, π)` + per-corner `Canonical`/`Swapped`/`NoCluster` label | generic `projective_grid::expert::orientation`: orientation histogram + plateau-aware peak picking + double-angle `(cos 2θ, sin 2θ)` 2-means; per-corner slot assignment admitted iff `max(d_a0, d_a1) ≤ cluster_tol_deg + cluster_sigma_k·max(σ)` | histogram bias from marker-internal corners pulling centres off true axes; uncertain axes becoming unclustered | `num_bins`, `max_iters_2means`, `cluster_tol_deg`, `cluster_sigma_k`, `peak_min_separation_deg`, `min_peak_weight_fraction` |
| 3 | `topological_grid` | `projective-grid::expert::square` via `pipeline/mod.rs` | oriented features (positions + dual axes) + cluster centres as an axis hint | merged labelled `(i, j) → source_index` components in the topology axis-slot frame | `assemble_oriented2_components`: Delaunay classify → quad assembly → axis-driven cell-test walk → local-geometry component merge; intentionally stops before public canonicalization, generic validation, and projective fit | axis-driven cell test admitting a spurious edge across a marker; foreshortening near the band edges | `topological` (`TopologicalParams`) |
| 4a | `axis_aware_recovery` | `recover.rs` + `boosters.rs` | facade-merged components + clustered corners | per-component grid extended by booster fills | Estimate directional cell scale, then interior gap fill + line extrapolation through `fill_grid_holes`; additions re-run axis, parity, and edge invariants. Optional weak-cluster rescue re-admits only near-threshold `NoCluster` corners. | borderline axes; extrapolation past the board | `attach_search_rel`, `attach_axis_tol_deg`, `attach_ambiguity_factor`, `step_tol`, `edge_axis_tol_deg`, `enable_weak_cluster_rescue`, `weak_cluster_tol_deg`, `max_booster_iters` |
| 4b | `geometry_only_recovery` | `recover.rs` + `boosters.rs` | output of 4a + admitted `NoCluster` corners | conservative additions, then post-recovery component merge | One fill pass over the existing KD/prediction engine. A unique candidate must be supported entirely by the pre-pass snapshot: two perpendicular cardinal neighbours plus their diagonal, parallelogram agreement, and every available cardinal edge-length check. `Raw`, weak, and sigma-invalid corners are ineligible. | severe local projection; ambiguous nearby candidate | `enable_geometry_only_recovery`, `geometry_recovery_tol_rel`, `attach_ambiguity_factor`, `step_tol`, `component_merge` |
| 5 | `final_geometry_check` | `geometry_check.rs` | final labelled set | drop list + `detection_refused` flag | **mandatory, can only DROP** (never add or relabel): (a) shared `validate` (line collinearity + local-H residual) with **looser** `geometry_check_*` tolerances — catches gross mislabels (full-cell / diagonal ≈ 1.4-cell residual) without flagging accepted perspective drift; (b) the direct topological wrong-label check (interior skipped-corner edges + duplicate-pixel labels); (c) largest-cardinally-connected-component filter, dropping isolated leaks outside the main grid. Refuses the detection if survivors `< min_labeled_corners` | strict per-edge length tests over-flag distorted boards (kept loose deliberately); single-component constraint is the chessboard contract | `geometry_check_line_tol_rel`, `geometry_check_local_h_tol_rel`, `line_min_members`, `validate_step_aware`, `enable_final_edge_shape_check` |
| 6 | `output` | `output.rs` | surviving labelled set | `ChessboardDetection { cell_size, corners: ChessboardCorner[] }` | call `projective_grid::expert::lattice::normalize_square_entries` (rebase min → `(0, 0)`; canonicalise so `+i ≈ +x`, `+j ≈ +y`; stable `(j, i)` sort), then copy the normalized `Coord{u,v}` onto each output corner | — | `params.min_labeled_corners` |

### Key invariants

These hold across every stage that can attach a label and bias the detector
toward precision. Zero wrong labels remains a measured regression gate, not a
mathematical guarantee for arbitrary imagery:

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
  has passed all implemented predicates; it still needs dataset evidence for
  the target domain.
- **Non-negative labels.** Output rebases the labelled bounding-box
  minimum to `(0, 0)` — a hard invariant for overlay / calibration
  consumers.

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
  - `topological` — the axis-driven grid finder used by both the ordinary
    facade and `expert::square::assemble_oriented2_components`: Delaunay
    classify → quads → walk → facade merge.
  - `shared::{merge, fit, validate, fill, grow}` — `merge_components_local`,
    the projective fit + residual helper, the lattice-general drop
    filters (line / local-H validation, topological wrong-label drops,
    largest-component filter), and the `fill_grid_holes` engine plus the
    `SquareAttachPolicy` seam where caller-specific invariants enter.
- **`calib-targets-chessboard`** (chessboard-specific): the strength /
  sigma prefilter (`inputs.rs`), axis clustering glue (`cluster/`), the recall
  boosters with parity + directional edge scale (`boosters.rs`), the
  per-component recovery + post-booster merge (`recover.rs`), the
  mandatory geometry-check **orchestration** (the drop filters
  themselves live in `shared::validate`; the chessboard sequences them),
  the output adapter (`output.rs`) that maps the normalized lattice
  canonical `Coord{u,v}` into the workspace result — the rebase +
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
