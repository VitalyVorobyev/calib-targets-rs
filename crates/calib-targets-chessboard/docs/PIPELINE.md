# Chessboard-v2 detection pipeline — atomic stages

Concise stage-by-stage map of `calib-targets-chessboard`'s detector. Each
row lists the stage's input, decision predicate, output, dominant
failure modes, and the `DetectorParams` knobs that govern it. This is
the working reference for diagnosing a detector failure on a real
image — start here before reading source.

The detector is **precision-anchored**: every stage that can attach a
new label runs an axis / parity / edge invariant; the mandatory final
geometry check (Stage 9) drops anything that slipped through. Wrong
`(i, j)` labels are unrecoverable for downstream calibration; missing
corners are acceptable.

## Stage table

| # | Name | In | Out | Decision | Failure modes | Knobs |
|---|---|---|---|---|---|---|
| 0 | input | `&[Corner]` from ChESS | per-corner `CornerAug { stage: Raw }` | trivial copy | corners outside image; ChESS misdetections (markers) | — |
| 1 | strength + fit filter | `Raw` corners | `Strong` (passes) / `Raw` (rejected) | `strength ≥ min_corner_strength` and `fit_rms ≤ max_fit_rms_ratio · contrast` | very-low-contrast frames; saturated edges (sigma=π → no info) | `min_corner_strength`, `max_fit_rms_ratio` |
| 2 | orientation histogram | `Strong.axes` | smoothed circular histogram on `[0, π)` | per-axis vote `strength / (1 + sigma)` into one of `num_bins` bins; smoothed by `[1, 4, 6, 4, 1] / 16` | marker-internal corners contributing axes 30°-60° off chessboard | `num_bins`, `peak_min_separation_deg`, `min_peak_weight_fraction` |
| 3 | 2-means cluster centres + per-corner gate | histogram + `Strong.axes` | `(θ₀, θ₁) = ClusterCenters` + `Clustered { label }` / `NoCluster { max_d_deg }` | centres seeded from peak picking, refined by **double-angle 2-means**; per-corner cost-min over `{Canonical, Swapped}` slot assignment, admitted iff `max(d_a0, d_a1) ≤ cluster_tol_deg` | **histogram bias from marker corners pulls centres ~3° off true axes, breaking parity-B** (small3.png); cluster_sigma_k bonus capped to avoid sub-grid seeds | `cluster_tol_deg`, `cluster_sigma_k`, `max_iters_2means` |
| 4 | seed search | `Clustered` | `SeedOutput { seed: 4 corner indices, cell_size }` | self-consistent 4-corner quad: edges within `seed_edge_tol`, axis match within `seed_axis_tol_deg`, midpoint sanity, no marker-internal corners straddling | dense ChArUco regions producing spurious quads; cluster_sigma_k bonus admitting seeds at sqrt(2)·cell | `seed_edge_tol`, `seed_axis_tol_deg`, `seed_close_tol` |
| 5 | BFS grow | seed + `Clustered` corners | `GrowResult { labelled: HashMap<(i,j), idx>, ... }` | KD-tree of `Clustered`; for each empty cell adjacent to labelled, predict from neighbours, find candidate within `attach_search_rel · cell_size`, validate axes (`attach_axis_tol_deg`), edge length (`step_tol`), parity slot swap (`edge_axis_tol_deg`); ambiguity reject if 2nd within `attach_ambiguity_factor × nearest`; rebase to `(0, 0)` | column gaps (parity-B holes from Stage 3) prevent BFS from propagating one cell past; perspective foreshortening pushes cell length below `step_tol` | `attach_search_rel`, `attach_axis_tol_deg`, `step_tol`, `edge_axis_tol_deg`, `attach_ambiguity_factor` |
| 6 | local-H boundary extension | labelled bbox | extra labels at boundary + interior holes | per-candidate H from K nearest labelled (Manhattan), residual gate, single-claim attachment with same parity / axis / edge gates as BFS, tighter ambiguity (2.5×) | extrapolating from one-sided support corners drifts > search radius; left-strip orphans 1.5+ cells past bbox edge | `stage6_local_h`, `stage6_local_k_nearest` |
| 6.5 | NoCluster rescue | labelled set + `Strong` / `NoCluster` corners | extra labels admitted from non-Clustered corners | same per-cell local-H prediction; widened axis tolerance (`rescue_axis_tol_deg = 22°`); inferred parity from axes vs centres; same edge-slot-swap invariant; wider search radius (`rescue_search_rel = 0.8`) | dominant rejection on small3.png is `no_candidate` (557 cells); precision-protective (parity / edge gates still fire) | `enable_stage6_5_rescue`, `rescue_axis_tol_deg`, `rescue_search_rel`, `stage6_5_local_k_nearest` |
| 6.75 | post-grow centre refit | labelled axes only | refined `(θ₀′, θ₁′)` + optional second Stage-6 / 6.5 pass | undirected circular mean of labelled axes per slot — no marker contribution → unbiased; if `‖shift‖ > refit_min_shift_deg`, re-classify Strong/NoCluster under refined centres and re-run Stage 6 / 6.5 once. Does **not** re-run BFS (regresses other images under small centre shifts). | recall recovery limited because second-pass local-H still extrapolates from same anchors; refit primarily improves cluster admission for future iterations | `enable_post_grow_refit`, `refit_min_labelled`, `refit_min_shift_deg` |
| 8 | recall boosters | labelled set + `Clustered` (and `NoCluster` if `enable_weak_cluster_rescue`) | extra labels via interior gap fill + line extension | each addition runs the same parity / axis / edge invariants as BFS; capped by `max_booster_iters` | over-flag of borderline corners; line extension projecting past true board edge | `enable_line_extrapolation`, `enable_gap_fill`, `enable_component_merge`, `enable_weak_cluster_rescue`, `weak_cluster_tol_deg`, `max_booster_iters` |
| 7 | BFS validation loop | labelled set | blacklist for next BFS iteration | line collinearity (per row + per column) + per-corner local-H residual + step-aware tolerances; attribution rules pick the worst outlier | tight `local_h_tol_rel` over-flags perspective-distorted corners; runs only inside the seed/grow loop (NOT after Stage 6 / 6.5 / boosters) | `line_tol_rel`, `local_h_tol_rel`, `validate_step_aware`, `step_deviation_thresh_rel`, `max_validation_iters` |
| 9 | **MANDATORY geometry check** | final labelled set | drop list (`LabeledThenBlacklisted`) + `detection_refused` flag | (a) `validate()` with **looser** tolerances (`geometry_check_line_tol_rel = 0.45`, `geometry_check_local_h_tol_rel = 0.6`) catches gross mislabels (full-cell / diagonal shifts produce ~1.4 cell residual) without flagging perspective drift; (b) **largest-connected-component filter** keeps only the dominant cardinally-connected component, dropping isolated singletons and small leaks (typically marker corners that passed the cluster + parity gates but sit outside the main grid). | strict per-edge axis-slot-swap was tried as a third predicate but over-flags every distorted board (rigid `step_tol` length test); single-component constraint is the chessboard contract per CLAUDE.md and catches the small2.png-class orphan-marker case | `geometry_check_line_tol_rel`, `geometry_check_local_h_tol_rel` |
| 10 | emit detection | surviving labelled set | `Detection { grid_directions, cell_size, corners: LabeledCorner[] }` | rebase `(i, j)` to non-negative; canonicalise so `+i ≈ +x`, `+j ≈ +y`; sort by `(j, i)`; refuse if `final_count < min_labeled_corners` | — | `min_labeled_corners` |

## Multi-component dispatch

`Detector::detect_all` runs Stages 0-10 up to `max_components` times,
blacklisting corners consumed by each successful frame. Used by ChArUco
when one physical board produces several disconnected chessboard
sub-grids (markers split rows). The chessboard precision contract is
preserved per-component.

## What lives in `projective-grid` vs `calib-targets-chessboard`

- `projective-grid` (image-free): `square::seed`, `square::grow`,
  `square::grow_extension`, `square::validate`, `square::alignment`,
  `component_merge`, `topological`, `homography`, `circular_stats`.
- `calib-targets-chessboard` (chessboard-specific): orientation
  histogram + 2-means (`cluster.rs`), seed validator (parity gate),
  `ChessboardGrowValidator` + `ChessboardRescueValidator` (axis-slot-
  swap edge invariants), recall boosters with parity, post-grow refit,
  mandatory geometry check, multi-component dispatch.

## Diagnose dump fields

`bench diagnose --algorithm chessboard-v2 --dump-frame <path>` writes
`DebugFrame` JSON with one `IterationTrace` per validation loop pass
plus per-corner stage records. Each iteration trace carries:

- `extension`: Stage-6 stats (attached / rejected_no_candidate /
  rejected_label / rejected_validator / rejected_edge / h-residual
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

## Cross-references

- CLAUDE.md "Evidence-driven detector debugging" — methodology that
  every detector failure must be analysed against measurable numbers
  from this dump, not story-told.
- CLAUDE.md "Corner orientation contract (axes-only)" — the axis
  convention the cluster code and per-edge gates rely on.
- CLAUDE.md "Cell-size estimation gotcha" — why Stage 4 derives
  `cell_size` from a self-consistent seed rather than a global
  pre-computed scalar.
