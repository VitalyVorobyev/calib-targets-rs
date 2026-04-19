# Chessboard Detection — Corner-to-Graph Pipeline Specification

Precise specification of the chessboard detector in the `calib-targets-rs`
workspace: transforming a flat `&[Corner]` into a `TargetDetection` of
`LabeledCorner`s with integer `(i, j)` grid indices.

This document describes the **current implementation** on branch
`better_grid` (2026-04-18). Parameter values are the `Default::default()`
path unless a sweep is explicitly named.

> **Update, Phases 1, 2, 5 landed.**
>
> **Phase 1 / 2** — three post-graph geometric-sanity passes now run
> between graph build and connected-component extraction when
> `graph.mode == TwoAxis`: straightness (drop the worse of each bent
> Right/Left or Up/Down pair), planarity (drop the worse of each
> crossing edge pair), symmetry (drop asymmetric directed edges).
> See `crates/projective-grid/src/graph_cleanup.rs` and the
> `GraphCleanupParams` knobs in `params.rs`. A separate
> `min_component_size: Option<usize>` (default `None` → fallback to
> `min_corners`) gates the minimum component size at the
> `collect_components` stage.
>
> **Phase 5** — after BFS + local-H prune, `fill_gaps_via_local_affine`
> iterates the bounding box of the labelled set. For each unlabelled
> `(i, j)` with `≥ min_neighbors` labelled neighbors inside a
> `window_half`-cell window, it fits a local affine map `(i,j) → (x,y)`
> by least-squares, predicts the missing pixel position, and attaches
> the nearest unlabelled strong corner within `search_rel × local_step`.
> The pass iterates up to `max_iters` times because newly-attached
> corners unlock further predictions. See `GapFillParams` in
> `params.rs` and `fill_gaps_via_local_affine` in `detector.rs`.
> Recovers 6/7 of the "missing recoverable" frames from
> `docs/120issues.txt`.
>
> **Phase 4** — `OrientationClusteringParams.use_dual_axis` gates a
> histogram that votes with both `axes[0]` and `axes[1]` per corner
> (weighted by `1/(1+σ)`). Default `false`: enabling regressed
> detection rate from 84% to 1% on the 120 sweep and needs follow-up
> investigation. Infrastructure is in place for a future fix.
>
> **Phase 3** — a smooth cell-size field + marker-interior corner
> rejection. Not implemented: measurements show the `TwoAxis` validator
> step-window + Phase 2 planarity/straightness already sequester
> marker-interior false positives into sub-`min_component_size`
> components that get dropped by the component filter. Re-evaluate if
> a dataset surfaces marker-interior contamination that survives
> Phases 1+2+5.

Entry point: `ChessboardDetector::detect_all_instrumented`
(`crates/calib-targets-chessboard/src/detector.rs:262`).

Pipeline (linear):

```text
Corner[]
 → 1. per-corner filters (strength, fit-RMS)
 → 2. orientation clustering  (optional)
 → 3. local-step estimate + graph build
 → 4. connected components
 → 5. BFS (i, j) assignment
 → 6. dedup by (i, j) + board-size fit + completeness gate
 → 7. local-homography residual prune      (optional)
 → 8. global-homography residual prune     (optional)
 → 9. post-prune local-homography p95 gate (optional)
 → TargetDetection
```

The multi-component path iterates stages 5–9 once per connected component,
returning all qualifying components sorted by corner count descending.

---

## 0. Input: `Corner`

`crates/calib-targets-core/src/corner.rs:48`. Fields consumed:

| Field                 | Type                   | Role                                                                     |
|-----------------------|------------------------|--------------------------------------------------------------------------|
| `position`            | `Point2<f32>`          | Pixel (origin TL, +x right, +y down).                                    |
| `orientation`         | `f32` (rad mod π)      | Legacy single-axis angle. Stage 2, Simple/Cluster validators.            |
| `orientation_cluster` | `Option<usize>`        | `{0, 1}` or `None`. Written by Stage 2; read by Cluster validator.       |
| `axes`                | `[AxisEstimate; 2]`    | Two local grid axes with 1σ. TwoAxis validator.                          |
| `contrast`            | `f32`                  | `|A|` from upstream tanh fit. Fit-RMS ratio filter.                      |
| `fit_rms`             | `f32`                  | RMS residual of two-axis fit. Fit-RMS ratio filter.                      |
| `strength`            | `f32`                  | Raw ChESS response. Strength filter + `LabeledCorner.score`.             |

`AxisEstimate { angle, sigma }` (`corner.rs:21`): `angle ∈ [0, π)` for
`axes[0]`, `(axes[0].angle, axes[0].angle + π)` for `axes[1]`. Default axis
has `sigma = π` ("no information"). There is **no per-corner local-step
field** — local step is computed per-graph inside Stage 3 from neighbor
geometry.

---

## 1. Per-corner filters

Two independent predicates, both on a single `Corner`, applied back-to-back
at `detector.rs:269`.

**1.1 Strength.** `keep iff c.strength >= params.min_corner_strength`.
Default `0.0` (disabled).

**1.2 Fit-RMS ratio** (P1.3 insurance, `passes_fit_quality`, `detector.rs:32`):

```text
if !max_ratio.is_finite():     pass        # disabled
if c.contrast <= 0.0:          pass        # legacy descriptor
keep iff c.fit_rms <= max_ratio * c.contrast
```

Default `max_fit_rms_ratio = f32::INFINITY` (disabled).

**Early exit.** If `after_strength_filter < min_corners` (default 16) the
detector returns an empty results vector with populated
`ChessboardStageCounts` (`detector.rs:282`).

---

## 2. Orientation clustering (optional)

Runs when `use_orientation_clustering = true` (default). Purpose: recover
the two dominant orientation lines of the corner cloud (the light-square
diagonals, at 45° and 135° for an axis-aligned board), label each corner
`{0, 1, None}`, drop `None`.

File: `crates/calib-targets-core/src/orientation_clustering.rs`.

### 2.1 Histogram (`build_smoothed_histogram`, line 239)

1. Wrap `c.orientation` to `[0, π)`.
2. Bin into `num_bins` (default 90 → 2° each).
3. Bucket weight = `max(strength, 0)` when `use_weights = true` (default), else 1.
4. Circular-smooth with kernel `[1,4,6,4,1]/16` (`smooth_circular_histogram`, line 407).

### 2.2 Peak finding (`find_peaks`, line 436)

Local maxima bins: `h[i] ≥ h[i±1]` and `h[i] > 0`. For each peak, expand
left and right into a contiguous run of monotonically non-increasing bins
(`build_peak_support`, line 319). A `PeakSupport` is `{bins, weight, weighted_angle}`.
Discard supports with `weight < total_weight × min_peak_weight_fraction`
(default 0.05). Return `None` if fewer than 2 remain.

### 2.3 Seed selection

- Seed 1: weighted circular mean of `Corner.orientation`s falling in the
  strongest support (`angle_from_corners`, line 289).
- Seed 2: next-strongest support whose weighted-mean angle differs from
  seed 1 by ≥ `peak_min_separation_deg` (default 10°) on the period-π
  circle (`angular_dist_pi` returns `[0, π/2]`).
- Return `None` if no second seed qualifies.

### 2.4 Circular 2-means (line 151, default `max_iters = 10`)

```text
for _ in 0..max_iters:
    for i, c in corners:
        t = wrap_π(c.orientation); d0 = dist_π(t,centers[0]); d1 = dist_π(t,centers[1])
        (best, d) = argmin; labels[i] = best if d ≤ outlier_thresh_rad else None
    centers[k] = circular_mean_weighted(corners labelled k, weight = strength)
    break if no label changed
```

`outlier_threshold_deg` default 30°.

### 2.5 Bypass / fallback

- `use_orientation_clustering = false` → skipped entirely; `grid_diagonals = None`
  on entry to fallback.
- `cluster_orientations` returns `None` when `num_bins < 4`, total weight
  is zero, fewer than 2 surviving peaks, or seeds too close.
- Fallback (`estimate_grid_axes_from_orientations`, line 457): double-angle
  weighted mean over all corners, then `grid_diagonals = [wrap_π(θ), wrap_π(θ + π/2)]`.
  Important: fallback leaves `graph_diagonals = None` (see ambiguity #2) —
  so Stage 3 uses the Simple validator, not the Cluster validator.

Corners with `labels[i] == None` are dropped. A second `min_corners` gate
runs (`detector.rs:356`). `counts.after_orientation_cluster_filter` is
populated only when clustering succeeded (not on fallback).

---

## 3. Graph build

File: `crates/calib-targets-chessboard/src/gridgraph.rs`, entry
`build_chessboard_grid_graph_instrumented` (line 521).

Output: `GridGraph { neighbors: Vec<Vec<NodeNeighbor>> }` —
`NodeNeighbor { direction, index, distance, score }`
(`projective-grid/src/direction.rs:40`) — with at most one neighbor per
cardinal direction `{Right, Left, Up, Down}`.

### 3.1 Mode dispatch (`ChessboardGraphMode`, `params.rs:8`)

| Mode               | Validator                                                                                   |
|--------------------|---------------------------------------------------------------------------------------------|
| `Legacy` (default) | `ChessboardClusterValidator` if `grid_diagonals.is_some()`, else `ChessboardSimpleValidator`. |
| `TwoAxis`          | `ChessboardTwoAxisValidator` (always).                                                      |

The rest of Section 3 specifies **TwoAxis**, which is the current target
of this work. Legacy validator rejection reasons are listed in §3.7.

### 3.2 Local step estimation (`estimate_corner_local_steps`, `gridgraph.rs:212`)

Wraps `projective_grid::local_step::estimate_local_steps`
(`local_step.rs:132`). Per corner `i`:

1. KD-tree for `k_neighbors + 1` nearest (default `k = 8`). Drop self /
   coincidents.
2. Drop `|offset| > max_step_factor × median(|offset|)` (default `×3`).
3. Fold each neighbor's angle to `[0, π)`; classify into u-sector (nearer
   `fold(axis_u)`) or v-sector (nearer `fold(axis_v)`). Drop if both
   line-differences exceed `sector_half_width_rad` (default `π/6`).
4. Per sector: 1-D mean-shift with Epanechnikov kernel, bandwidth
   `bandwidth_rel × median` (default 0.15). Fall back to sector median on
   non-convergence (20 iter max, `1e-3` rel convergence).
5. `LocalStep { step_u, step_v, confidence, supporters_u, supporters_v }`
   with `confidence = clamp(min(s_u + s_v, denom) / denom, 0, 1)`,
   default `denom = 4`.

Output parallel to `strong`, indexed by raw-corner idx.

### 3.3 Global step (TwoAxis only) (`estimate_global_cell_size`, `global_step.rs:83`)

1. Per corner, record its nearest non-self distance.
2. Mean-shift mode from seeds at the 25th/50th/75th percentile, bandwidth
   `0.15 × seed`, 20 iter max.
3. Score modes by `support × cell_size` (scaled-by-size breaks dual-scale
   ChArUco ties in favour of the board step).
4. Returns `GlobalStepEstimate { cell_size, support, sample_count, confidence }`.

The TwoAxis validator uses `global_step.cell_size` as `step_fallback_pix`.
If estimation fails, fall back to `params.step_fallback_pix` (default 50
px), or 50 px literal if that is also non-finite (`gridgraph.rs:563`).

### 3.4 KD-tree pre-filter (`graph.rs:72`)

Per source, query `max(params.k_neighbors, 20)` nearest corners within
`max_distance = global_step × max(params.max_step_rel × 2, 2.5)` (default
`2.6 × global_step`). The wide window tolerates `global_step` underestimates
from marker-internal corners; per-edge bounds in §3.5 re-enforce the true
`±max_step_rel × local_step` range.

### 3.5 TwoAxis edge validator (`ChessboardTwoAxisValidator::validate`, line 410)

Candidate `(A, B)` with `offset = B.position − A.position`,
`edge_angle = atan2(offset.y, offset.x)`, `base_tol = angular_tol_rad`
(default 10° → `π/18`).

1. **Axis match at A** (`pick_best_axis`, line 489). For each `axis ∈ A.axes`:
   `diff = axis_vec_diff(axis.angle, edge_angle)` ∈ `[0, π/2]`;
   `tol = min(base_tol + axis.sigma, 2·base_tol)`. Pick the smallest `diff`
   with `diff ≤ tol`. No qualifier → **`NoAxisMatchSource`**, reject.
2. **Axis match at B.** Same on `B.axes`. No qualifier →
   **`NoAxisMatchCandidate`**, reject. (Slot index may differ from A — slot
   assignment can swap on a polarity flip; matched *lines* are what matter.)
3. **Axis-line agreement.** If
   `axis_vec_diff(A.axes[src_idx].angle, B.axes[cand_idx].angle) > 2 × max(tol_src, tol_cand)`,
   record **`AxisLineDisagree`**, reject.
4. **Step window at each endpoint.** Effective step:

   ```text
   eff(step, ax, fb) = step.{u|v} if step.confidence>0 AND step.{u|v}>0
                       else fb
   ```

   With `s_A = eff(A.step, src_idx, fallback)`, `s_B` analogous, accept iff

   ```text
   min_step_rel * s_A ≤ distance ≤ max_step_rel * s_A  AND  same at B
   ```

   Otherwise record **`OutOfStepWindow`**, reject. Defaults:
   `min_step_rel = 0.7`, `max_step_rel = 1.3`; `fallback = global_step`
   (§3.3).

5. **Direction classification** is pure image-space geometry —
   `direction_quadrant(offset)`:

   ```text
   if |offset.x| > |offset.y|: Right if offset.x ≥ 0 else Left
   else                      : Down  if offset.y ≥ 0 else Up
   ```

   Independent of `src_idx`/`cand_idx` — LRUD stays consistent under slot
   swaps.

6. **Score** (lower = better):
   `diff_src + diff_cand + step_error + 0.1 × (σ_src + σ_cand)`
   where `step_error = min(|distance − s_A| / max(s_A, 1e−3), 1)`.

### 3.6 Per-direction selection (`select_neighbors`, `graph.rs:137`)

Keep at most one accepted candidate per direction: lowest `score` wins,
ties broken by smaller `distance`. Each node ends with 0–4 outgoing edges.

### 3.7 Edge rejection taxonomy (`EdgeRejectReason`, `gridgraph.rs:37`)

When a `RejectionCounter` is attached (instrumented path, `detector.rs:368`)
each rejected candidate increments exactly one bucket. Snake-case tags
(`EdgeRejectReason::as_str`, line 77) become `HashMap<String, u64>` keys
in `counts.edges_by_reject_reason`.

| Reason                      | Validator       | Meaning                                                                           |
|-----------------------------|-----------------|-----------------------------------------------------------------------------------|
| `NotOrthogonal`             | Simple          | `|θ_A − θ_B|` not within `orientation_tolerance_deg` of π/2.                      |
| `OutOfDistanceWindow`       | Simple, Cluster | `distance ∉ [min_spacing_pix, max_spacing_pix]`.                                  |
| `EdgeAxisAngleMismatch`     | Simple          | Edge direction not at 45° ± tol to either endpoint orientation.                   |
| `MissingCluster`            | Cluster         | Either endpoint's `orientation_cluster` is `None`.                                |
| `SameClusterLegacy`         | Cluster         | Both endpoints in same diagonal cluster.                                          |
| `LowAlignment`              | Cluster         | Edge-direction best-alignment with canonical axes < `cos(tol)`.                   |
| `NoAxisMatchSource`         | TwoAxis         | Step 1 failed.                                                                    |
| `NoAxisMatchCandidate`      | TwoAxis         | Step 2 failed.                                                                    |
| `AxisLineDisagree`          | TwoAxis         | Step 3 failed.                                                                    |
| `OutOfStepWindow`           | TwoAxis         | Step 4 failed.                                                                    |
| `ClusterPolarityFlip`       | (stub)          | Reserved; not emitted.                                                            |
| `LocalHomographyResidual`   | (stub)          | Reserved; not emitted.                                                            |

### 3.8 Directedness and symmetry

The graph is a **directed** adjacency list — `graph.neighbors[i]` holds
`i`'s out-edges. No explicit symmetrisation runs. The TwoAxis predicates
are symmetric in `(A, B)` (all four checks treat endpoints symmetrically,
and `direction_quadrant(−offset)` returns the opposite of
`direction_quadrant(offset)`), so symmetry holds implicitly. Unit test
`direction_symmetry_on_rotated_grid` (`gridgraph.rs:800`) asserts the
symmetry for every accepted edge.

`counts.graph_edges` is the sum of adjacency-list lengths, i.e. each
undirected edge counts twice.

---

## 4. Connected components

`projective_grid::traverse::connected_components` (`traverse.rs:10`).

Iterative DFS treating the directed graph as undirected (visit all
`neighbors[i]` regardless of edge direction). Returns `Vec<Vec<usize>>`.

No filtering here. Isolated nodes become singleton components. Downstream,
`collect_components` (`detector.rs:566`) processes components in descending
size order; any with `len() < min_corners` (default 16) is skipped.

---

## 5. BFS `(i, j)` labelling

`assign_grid_coordinates` (`traverse.rs:48`).

**Seed.** Unconditionally `component[0]` at `(0, 0)`. This is the first
node in DFS traversal order from Stage 4 — typically the lowest-index
node in the component. No geometric seed heuristic.

**Propagation.** BFS over the adjacency list:

| direction | `(di, dj)` |
|-----------|------------|
| `Right`   | `(+1,  0)` |
| `Left`    | `(−1,  0)` |
| `Up`      | `( 0, −1)` |
| `Down`    | `( 0, +1)` |

Pop → record `(idx, {i, j})` → enqueue each neighbor at `(i+di, j+dj)`.

**Conflict handling.** **None at the BFS layer.** Duplicate queue entries
are tolerated; the first `pop_front` to visit a node wins, later copies
are discarded by the `visited` check (`traverse.rs:60`). Two neighbors
propagating conflicting `(i, j)` to the same physical corner silently
produce the first-enqueued label (FIFO), later attempts dropped.

Downstream `(i, j)` collision resolution happens at the `LabeledCorner`
layer (Stage 6) — **two physical corners mapping to one cell**, not "one
corner with two candidate labels."

---

## 6. Component → board coordinates

`component_to_board_coords` (`detector.rs:637`).

**6.1 Bounding box + board-size fit** (`select_board_size`, line 1147):
compute `(min_i, max_i, min_j, max_j)`; `width = max_i−min_i+1`,
`height = max_j−min_j+1`. If both `expected_cols` and `expected_rows` are
set, try both orientations and accept the one that fits (or transpose),
preferring the tighter summed gap; otherwise `(width, height, swap=false)`.
No fit → reject component.

**6.2 Dedup.** Bucket `(node_idx, GridIndex)` into
`HashMap<GridCoords, LabeledCorner>` keyed by normalised `(gi, gj) =
(g.j − min_j, g.i − min_i)` if `swap_axes` else `(g.i − min_i, g.j − min_j)`.
On collision, keep the higher `score` (= `Corner.strength`). Resolves the
BFS-conflict case in §5.

**6.3 Completeness.** `completeness = unique_corners / (cols × rows)`.
When both expected dimensions are set AND this is the primary (largest)
component, reject if `completeness < completeness_threshold` (default
`0.7`). Secondary components skip this gate
(`skip_completeness = true`, `detector.rs:599`).

---

## 7. Local-homography residual prune (optional)

`prune_by_local_homography_residual` (`detector.rs:968`). Runs when
`params.local_homography.enable = true`. Default `false`. Short-circuits
on `labeled.len() < min_keep` or `min_neighbors < 4`.

Iterative **one-at-a-time**: each pass removes at most the single
worst-scoring outlier. Removing one refits every other window for the
next pass, preventing label-error contamination from cascading.

```text
for _ in 0..max_iters:                           # default 16
    if len(labeled) <= min_keep: break
    build idx_by_grid = {(g.i, g.j) : idx}
    worst = None
    for (idx, lc) in labeled:
        collect neighbors in [-W..W] × [-W..W] window (excluding self)
           grid_pts, img_pts = labelled neighbors
           nbr_distances    = immediate (|di|,|dj| ≤ 1) pixel distances
        if len(grid_pts) < min_neighbors: continue
        H = estimate_homography_rect_to_img(grid_pts, img_pts)  # DLT
        if H is None: continue
        pred     = H.apply((g.i, g.j))
        residual = |pred − lc.position|
        step_est = median(nbr_distances) or None
        thresh   = max(threshold_rel * step_est, threshold_px_floor) if step_est
                   else threshold_px_floor
        if residual > thresh:
            margin = residual / thresh
            if worst is None or margin > worst.margin:
                worst = (idx, margin)
    if worst is None: break
    labeled.remove(worst.idx)
```

Defaults: `window_half = 2` (5×5 window), `min_neighbors = 5`,
`threshold_rel = 0.15`, `threshold_px_floor = 2.0`, `max_iters = 16`.

The prune computes a **fresh** per-corner step from the median of the
immediate-neighbor pixel distances in the current window — it does **not**
reuse Stage 3.2's `LocalStep` output (which is indexed by raw-corner idx,
not post-dedup `LabeledCorner` idx).

---

## 8. Global-homography residual prune (optional)

`prune_by_homography_residual` (`detector.rs:858`). Runs when
`enable_global_homography_prune = true` (default). Runs **after** §7 so
its input is locally cleaned. Short-circuits on `labeled.len() < 4` or
`len() ≤ min_keep`.

Hard-coded constants: `HARD_CUTOFF_PX = 0.5`, `HARD_TIER_DROP_FRAC = 0.05`,
`P95_TARGET_PX = 1.0`, `OUTLIER_MAD_FACTOR = 3.0`, `HARD_FLOOR_PX = 0.5`,
`MAX_HARD_ITERS = 10`, `MAX_MAD_ITERS = 5`.

**Hard tier** (line 880):

```text
for _ in 0..MAX_HARD_ITERS:
    H  = DLT(labeled grid → img)
    r  = |H.apply(g) − p| per corner
    if median(r) <= 0.5 AND p95(r) <= 1.0: break
    drop = max(1, ceil(n * 0.05))
    if n − drop < min_keep: break
    remove the `drop` highest-residual corners
```

**MAD tier** (line 918):

```text
for _ in 0..MAX_MAD_ITERS:
    r, med = residuals, median(r)
    mad    = median(|r − med|)
    thresh = max(med + 3.0 * mad * 1.4826, 0.5)
    drop r > thresh
    if nothing dropped OR n − pruned < min_keep: break
```

Rationale (`detector.rs:845-856`): MAD alone cannot attack the
"globally-skewed BFS" mode — every corner has a large residual and MAD
stays small. The hard tier forces median + p95 down first, then MAD
handles final cleanup.

The global-fit assumption breaks under non-trivial lens distortion;
flip the flag off for high-distortion / wide-FoV captures.

---

## 9. Post-prune local-homography p95 gate (optional)

Runs when `max_local_homography_p95_px.is_some()` (default `None`,
disabled).

`local_homography_residual_p95` (`detector.rs:1069`) — analogous to §7
but with **fixed** window / min-neighbor constants, independent of
`LocalHomographyPruneParams`:

1. For each labelled corner: collect labelled neighbors in a 2-cell window
   (`|di|, |dj| ≤ 2`), excluding self.
2. If ≥ 5 neighbors, fit a DLT homography, compute residual
   `|H.apply((g.i, g.j)) − lc.position|`.
3. Return p95 across all computed residuals, or `None` if < 2.

If `p95 > max_local_homography_p95_px`, reject detection
(`detector.rs:777`); stage counts are still populated for diagnostics.

Rationale: a correctly-labelled grid has sub-pixel local residuals
regardless of lens distortion; high post-prune p95 = pruning bottomed
out at `min_corners` before clearing label errors.

---

## 10. Output assembly

`TargetDetection` at `detector.rs:826`:

```rust
TargetDetection {
    kind: TargetKind::Chessboard,
    corners: Vec<LabeledCorner>,         // sorted by (j, i) row-major asc
}
```

`LabeledCorner` (`corner.rs:111`) per chessboard pipeline:

| Field             | Value                                                    |
|-------------------|----------------------------------------------------------|
| `position`        | Surviving `Corner.position` (no subpixel refit here).   |
| `grid`            | `Some({ i, j })`, normalised so `min = 0`.              |
| `id`              | `None`.                                                 |
| `target_position` | `None`.                                                 |
| `score`           | `Corner.strength` of the winning corner for the cell.  |

`ChessboardDetectionResult` wraps with
`inliers = 0..corners.len()` (every surviving corner is an inlier by
definition at this point), `orientations = grid_diagonals`, and a
`ChessboardDebug` payload (orientation histogram + full `GridGraph`).

The multi-component entry point returns all qualifying components sorted
by `detection.corners.len()` descending.

---

## Appendix A — Parameter defaults

| Parameter                                         | Default         | Disabled behaviour                       |
|---------------------------------------------------|-----------------|-------------------------------------------|
| `min_corner_strength`                             | `0.0`           | Strength filter is a no-op.               |
| `max_fit_rms_ratio`                               | `f32::INFINITY` | Fit-RMS filter is a no-op.                |
| `min_corners`                                     | `16`            | Gates entry + each component.             |
| `use_orientation_clustering`                      | `true`          | `false` → fallback double-angle mean.     |
| `completeness_threshold`                          | `0.7`           | Only used with both expected dims.        |
| `orientation_clustering.num_bins`                 | `90`            |                                           |
| `orientation_clustering.peak_min_separation_deg`  | `10°`           |                                           |
| `orientation_clustering.outlier_threshold_deg`    | `30°`           |                                           |
| `orientation_clustering.min_peak_weight_fraction` | `0.05`          |                                           |
| `graph.mode`                                      | `Legacy`        | TwoAxis must be selected explicitly.      |
| `graph.k_neighbors`                               | `8`             | TwoAxis raises floor to `20`.             |
| `graph.min/max_spacing_pix`                       | `5` / `50`      | Legacy only.                              |
| `graph.orientation_tolerance_deg`                 | `22.5°`         | Legacy only.                              |
| `graph.min_step_rel` / `max_step_rel`             | `0.7` / `1.3`   | TwoAxis only.                             |
| `graph.angular_tol_deg`                           | `10°`           | TwoAxis only.                             |
| `graph.step_fallback_pix`                         | `50 px`         | Overridden by `global_step` when it succeeds. |
| `local_homography.enable`                         | `false`         | Stage 7 skipped.                          |
| `local_homography.window_half`                    | `2`             | 5×5 window.                               |
| `local_homography.min_neighbors`                  | `5`             | Corners with fewer are kept unchecked.    |
| `local_homography.threshold_rel`                  | `0.15`          |                                           |
| `local_homography.threshold_px_floor`             | `2.0 px`        |                                           |
| `local_homography.max_iters`                      | `16`            |                                           |
| `enable_global_homography_prune`                  | `true`          | Stage 8 active by default.                |
| `max_local_homography_p95_px`                     | `None`          | Stage 9 skipped.                          |

## Appendix B — Ambiguities flagged

1. **BFS conflict resolution is FIFO.** Conflicting `(i, j)` propagations
   to the same node produce the first-enqueued label; later attempts are
   dropped. Not a geometric resolution step. Downstream dedup handles
   *two physical corners → one cell*, not *one corner → two labels*.
2. **`graph_diagonals` vs `grid_diagonals`.** The fallback double-angle
   estimator sets `grid_diagonals` but leaves `graph_diagonals = None`,
   so a clustering failure forces the Simple validator even with
   `use_orientation_clustering = true`. Subtle.
3. **`step_fallback_pix` effectively unreachable.** In TwoAxis the
   configured default (50 px) is overridden by
   `estimate_global_cell_size` every frame; only consulted on estimator
   failure.
4. **TwoAxis `k_neighbors` floor.** TwoAxis silently raises `k_neighbors`
   to ≥ 20 (`gridgraph.rs:585`). Callers configuring `8` observe a
   higher effective k.
5. **Stage-9 constants are hard-coded.** Window half (2) and min
   neighbours (5) in `local_homography_residual_p95` do **not** reuse
   `LocalHomographyPruneParams` — the two stages evaluate "local
   residual" on slightly different neighbour sets.
6. **Stage-count semantics for fallback.**
   `counts.after_orientation_cluster_filter = None` on fallback even
   though the `strong` set is unchanged; treat `None` as "filter not
   applied," not "kept everything."
