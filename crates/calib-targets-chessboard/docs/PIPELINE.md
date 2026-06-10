# Chessboard detection — pipeline stage maps

Concise stage-by-stage map of `calib-targets-chessboard`'s detector(s).
Each row in the stage tables below lists the stage's input, decision
predicate, output, dominant failure modes, and the `DetectorParams`
knobs that govern it. This is the working reference for diagnosing a
detector failure on a real image — start here before reading source.

The detector is **precision-anchored**: every stage that can attach a
new label runs an axis / parity / edge invariant; the mandatory final
geometry check drops anything that slipped through. Wrong `(i, j)`
labels are unrecoverable for downstream calibration; missing corners
are acceptable.

## Two graph-build algorithms

`DetectorParams::graph_build_algorithm` selects between two grid
builders that produce the same `(i, j) → corner_idx` output shape, so
downstream consumers (ChArUco, marker board, PuzzleBoard) are agnostic
to which ran:

- **`GraphBuildAlgorithm::Topological` (default)** — Shu/Brunton/Fiala
  grid finder (`projective_grid::detect_grid_all`) wrapped by a
  chessboard-specific input adapter and recovery layer. Image-free
  below ChESS; an axis-driven cell test replaces the paper's
  image-color test. Higher recall than seed-and-grow on clean
  chessboards / PuzzleBoards; not precision-safe on ChArUco-style
  marker frames (hence the ChArUco pin below). Documented separately in
  [`docs/topological-grid-detection.md`](../../../docs/topological-grid-detection.md).
- **`GraphBuildAlgorithm::SeedAndGrow`** — invariant-rich seed-and-grow
  pipeline (`crate::grow::bfs_grow` + boundary extension via
  homography). Battle-tested across every target family and pinned for
  ChArUco. This is the path documented below.

**ChArUco pinning.** `CharucoDetector::new` unconditionally overrides
`chessboard.graph_build_algorithm = SeedAndGrow` regardless of caller
choice — the topological pipeline's per-edge axis test cannot survive
marker-internal X-corners, so the override is a precision guarantee,
not a configuration choice. PuzzleBoard and marker board inherit the
caller's choice via their nested `DetectorParams`.

---

## SeedAndGrow pipeline (pinned for ChArUco)

### Stage table

| # | Name | In | Out | Decision | Failure modes | Knobs |
|---|---|---|---|---|---|---|
| 0 | input | `&[Corner]` from ChESS | per-corner `CornerAug { stage: Raw }` | trivial copy | corners outside image; ChESS misdetections (markers) | — |
| 1 | strength + fit filter | `Raw` corners | `Strong` (passes) / `Raw` (rejected) | `strength ≥ min_corner_strength` and `fit_rms ≤ max_fit_rms_ratio · contrast` | very-low-contrast frames; saturated edges (sigma=π → no info) | `min_corner_strength`, `max_fit_rms_ratio` |
| 2 | orientation histogram | `Strong.axes` | smoothed circular histogram on `[0, π)` | per-axis vote `strength / (1 + sigma)` into one of `num_bins` bins; smoothed by `[1, 4, 6, 4, 1] / 16` | marker-internal corners contributing axes 30°-60° off chessboard | `num_bins`, `peak_min_separation_deg`, `min_peak_weight_fraction` |
| 3 | 2-means cluster centres + per-corner gate | histogram + `Strong.axes` | `(θ₀, θ₁) = ClusterCenters` + `Clustered { label }` / `NoCluster { max_d_deg }` | centres seeded from peak picking, refined by **double-angle 2-means**; per-corner cost-min over `{Canonical, Swapped}` slot assignment, admitted iff `max(d_a0, d_a1) ≤ cluster_tol_deg` | **histogram bias from marker corners pulls centres ~3° off true axes, breaking parity-B** (small3.png); cluster_sigma_k bonus capped to avoid sub-grid seeds | `cluster_tol_deg`, `cluster_sigma_k`, `max_iters_2means` |
| 4 | seed search | `Clustered` | `SeedOutput { seed: 4 corner indices, cell_size }` | self-consistent 4-corner quad: edges within `seed_edge_tol`, axis match within `seed_axis_tol_deg`, midpoint sanity, no marker-internal corners straddling | dense ChArUco regions producing spurious quads; cluster_sigma_k bonus admitting seeds at sqrt(2)·cell | `seed_edge_tol`, `seed_axis_tol_deg`, `seed_close_tol` |
| 5 | BFS grow | seed + `Clustered` corners | `GrowResult { labelled: HashMap<(i,j), idx>, ... }` | KD-tree of `Clustered`; for each empty cell adjacent to labelled, predict from neighbours, find candidate within `attach_search_rel · cell_size`, validate axes (`attach_axis_tol_deg`), edge length (`step_tol`), parity slot swap (`edge_axis_tol_deg`); ambiguity reject if 2nd within `attach_ambiguity_factor × nearest`; rebase to `(0, 0)` | column gaps (parity-B holes from Stage 3) prevent BFS from propagating one cell past; perspective foreshortening pushes cell length below `step_tol` | `attach_search_rel`, `attach_axis_tol_deg`, `step_tol`, `edge_axis_tol_deg`, `attach_ambiguity_factor` |
| 6 | boundary extension via homography | labelled bbox | extra labels at boundary + interior holes | switch on `stage6_local_h`: **default `false`** → `extend_via_global_homography` (single global H from the whole labelled set, residual gate, single-claim attachment); `true` → `extend_via_local_homography` (per-candidate H from K nearest labelled by Manhattan distance, residual gate). Both branches share the same parity / axis / edge gates as BFS and tighter ambiguity (2.5×). | extrapolating from one-sided support corners drifts > search radius; left-strip orphans 1.5+ cells past bbox edge | `stage6_local_h`, `stage6_local_k_nearest` |
| 6.5 | NoCluster rescue | labelled set + `Strong` / `NoCluster` corners | extra labels admitted from non-Clustered corners | same per-cell local-H prediction; widened axis tolerance (`rescue_axis_tol_deg = 22°`); inferred parity from axes vs centres; same edge-slot-swap invariant; wider search radius (`rescue_search_rel = 0.8`) | dominant rejection on small3.png is `no_candidate` (557 cells); precision-protective (parity / edge gates still fire) | `enable_stage6_5_rescue`, `rescue_axis_tol_deg`, `rescue_search_rel`, `stage6_5_local_k_nearest` |
| 6.75 | post-grow centre refit | labelled axes only | refined `(θ₀′, θ₁′)` + optional second Stage-6 / 6.5 pass | undirected circular mean of labelled axes per slot — no marker contribution → unbiased; if `‖shift‖ > refit_min_shift_deg`, re-classify Strong/NoCluster under refined centres and re-run Stage 6 / 6.5 once. Does **not** re-run BFS (regresses other images under small centre shifts). | recall recovery limited because second-pass local-H still extrapolates from same anchors; refit primarily improves cluster admission for future iterations | `enable_post_grow_refit`, `refit_min_labelled`, `refit_min_shift_deg` |
| 8 | recall boosters | labelled set + `Clustered` (and `NoCluster` if `enable_weak_cluster_rescue`) | extra labels via interior gap fill + line extrapolation | each addition runs the same parity / axis / edge invariants as BFS; capped by `max_booster_iters`. Does **not** call `merge_components_local` — SeedAndGrow keeps the labelled set as a single connected component by construction (multi-board recall comes from `Detector::detect_all` re-running the pipeline up to `max_components` times with a blacklist, not from component merging). | over-flag of borderline corners; line extrapolation projecting past true board edge | `enable_weak_cluster_rescue`, `weak_cluster_tol_deg`, `max_booster_iters` |
| 7 | BFS validation loop | labelled set | blacklist for next BFS iteration | line collinearity (per row + per column) + per-corner local-H residual + step-aware tolerances; attribution rules pick the worst outlier | tight `local_h_tol_rel` over-flags perspective-distorted corners; runs only inside the seed/grow loop (NOT after Stage 6 / 6.5 / boosters) | `line_tol_rel`, `local_h_tol_rel`, `validate_step_aware`, `step_deviation_thresh_rel`, `max_validation_iters` |
| 9 | **MANDATORY geometry check** | final labelled set | drop list (`LabeledThenBlacklisted`) + `detection_refused` flag | (a) `validate()` with **looser** tolerances (`geometry_check_line_tol_rel = 0.45`, `geometry_check_local_h_tol_rel = 0.6`) catches gross mislabels (full-cell / diagonal shifts produce ~1.4 cell residual) without flagging perspective drift; (b) **largest-connected-component filter** keeps only the dominant cardinally-connected component, dropping isolated singletons and small leaks (typically marker corners that passed the cluster + parity gates but sit outside the main grid). | strict per-edge axis-slot-swap was tried as a third predicate but over-flags every distorted board (rigid `step_tol` length test); single-component constraint is the chessboard contract per CLAUDE.md and catches the small2.png-class orphan-marker case | `geometry_check_line_tol_rel`, `geometry_check_local_h_tol_rel` |
| 10 | emit detection | surviving labelled set | `Detection { grid_directions, cell_size, corners: LabeledCorner[] }` | rebase `(i, j)` to non-negative; canonicalise so `+i ≈ +x`, `+j ≈ +y`; sort by `(j, i)`; refuse if `final_count < min_labeled_corners` | — | `min_labeled_corners` |

### Multi-component dispatch

`Detector::detect_all` runs Stages 0-10 up to `max_components` times,
blacklisting corners consumed by each successful frame. Used by ChArUco
when one physical board produces several disconnected chessboard
sub-grids (markers split rows). The chessboard precision contract is
preserved per-component. **This is also how SeedAndGrow handles the
"multiple boards in one image" case** — it dispatches the whole
pipeline N times rather than producing one labelled set with multiple
connected components and merging them.

### Diagnose dump fields

`bench diagnose --algorithm seed-and-grow --dump-frame <path>` writes
`DebugFrame` JSON with one `IterationTrace` per validation loop pass
plus per-corner stage records. Each iteration trace carries:

- `extension`: Stage-6 stats (attached / rejected_no_candidate /
  rejected_label / rejected_policy / rejected_edge / h-residual
  median + max).
- `rescue`: Stage-6.5 stats, same shape.
- `refit`: Stage-6.75 (`shift_deg`, `new_centers_deg`, `promoted`,
  `second_pass_ran`).
- `extension2` / `rescue2`: second-pass Stage-6 / 6.5 stats after refit.
- `geometry_check`: Stage-9 (`dropped`, `dropped_line_collinearity`,
  `dropped_local_h_residual`, `dropped_edge_invariant`,
  `detection_refused`).

When investigating a missing-corner case, the canonical workflow is:

1. `bench diagnose <image> --dump-frame /tmp/dump.json`.
2. Find the corner in `corners[]` and read its `stage`. If
   `NoCluster { max_d_deg }`, check whether the value is just-above
   `cluster_tol_deg` (Stage 3 issue → check Stage 6.75 refit).
3. If the corner is `Clustered` but unattached, BFS rejected it —
   check `extension` / `rescue` rejection counters for `no_candidate`
   (search radius too tight) vs `validator` / `edge` (axis or parity
   gate firing).
4. If the corner is `Labeled` then `LabeledThenBlacklisted`, look at
   the reason field — `geometry-check` means Stage 9 caught it
   (gross-mislabel safety net).

---

## Topological pipeline (default)

This is the default builder. To set it explicitly (or after overriding):

```rust
DetectorParams {
    graph_build_algorithm: GraphBuildAlgorithm::Topological,
    ..DetectorParams::default()
}
```

The topological builder is documented in full — generic
`projective-grid` core, chessboard input adapter, and chessboard
recovery layer — in
[`docs/topological-grid-detection.md`](../../../docs/topological-grid-detection.md).
The bench harness selects it with
`cargo run -p calib-targets-bench -- {run,preview,diagnose}
--algorithm topological`; output JSON / overlay filenames carry the
algorithm slug so a topological run and a seed-and-grow run coexist in
the same directory.

---

## What lives in `projective-grid` vs `calib-targets-chessboard`

- `projective-grid` (image-free, no internal workspace deps):
  `detect::advanced::square::{seed,grow,fill,grow_extend,extension,validate,component_merge,homography}`
  plus `detect::square::topological`. Provides the
  `SquareAttachPolicy` trait as the seam where caller-specific invariants
  enter.
- `calib-targets-chessboard` (chessboard-specific): orientation
  histogram + 2-means (`cluster.rs`), seed validator (parity gate),
  `ChessboardSquareAttachPolicy` + `ChessboardRescueValidator` (axis-slot-
  swap edge invariants), recall boosters with parity, post-grow refit,
  mandatory geometry check, multi-component dispatch, and the
  topological input adapter + recovery layer (see
  [`docs/topological-grid-detection.md`](../../../docs/topological-grid-detection.md)).

## Cross-references

- [`docs/topological-grid-detection.md`](../../../docs/topological-grid-detection.md)
  — the default `Topological` builder: generic `projective-grid` core,
  chessboard input adapter, and recovery layer.
- `crates/projective-grid/src/detect/square/topological/` — the
  projective-grid topological core, independent of chessboard semantics.
- CLAUDE.md "Evidence-driven detector debugging" — methodology that
  every detector failure must be analysed against measurable numbers
  from this dump, not story-told.
- CLAUDE.md "Corner orientation contract (axes-only)" — the axis
  convention the cluster code and per-edge gates rely on.
- CLAUDE.md "Cell-size estimation gotcha" — why the seed stage derives
  `cell_size` from a self-consistent seed rather than a global
  pre-computed scalar.
