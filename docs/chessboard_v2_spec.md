# Chessboard Detector v2 — Specification

**Status:** Draft for review.
**Goal:** Detect a chessboard grid graph from a cloud of ChESS corners
with **strong geometric invariants**, such that a wrong `(i, j)` label
is impossible by construction. At every step where the pipeline is
tempted to add a corner it must pass multiple independent geometric
checks. Missing corners are acceptable; wrong corners are not.

**Form:** standalone Rust crate `crates/chessboard-v2/` alongside the
existing `calib-targets-chessboard`. Reuses `projective-grid`
primitives (KD-tree, homography fit, local step). No changes to the
production detector until v2 is proven better.

**Prerequisite workspace migration** (see §12): remove the legacy
`Corner::orientation` field workspace-wide. All orientation logic in
v2 is axis-based.

---

## 1. Corner orientation contract

The **only** per-corner orientation signal is `Corner.axes: [AxisEstimate; 2]`.
Per `crates/calib-targets-core/src/corner.rs`, the convention is:

- `axes[0].angle ∈ [0, π)`; `axes[1].angle ∈ (axes[0].angle, axes[0].angle + π)`.
- `axes[1].angle - axes[0].angle = π/2` for an ideal grid corner (the
  two axes are orthogonal grid directions).
- The CCW sweep from `axes[0]` to `axes[1]` crosses a dark sector.

**Consequences for the chessboard detector.**

Let `Θ₀, Θ₁` be the two global grid-direction peaks in `[0, π)` — they
are `~π/2` apart. Every corner's two axes approximate these two peaks,
modulo the assignment to slots `[0]` and `[1]`:

- At **parity-0** corners, `axes[0].angle ≈ Θ₀`, `axes[1].angle ≈ Θ₀ + π/2`
  (which wraps to `Θ₁` in `[0, π)`).
- At **parity-1** corners the slots swap: `axes[0].angle ≈ Θ₁`,
  `axes[1].angle ≈ Θ₁ + π/2 = Θ₀ (mod π)`.

The slot assignment is the parity bit — this is the "two orientation
clusters" invariant restated for the axes-only contract.

For any edge `A → B` along a grid axis with direction `θ_e = atan2(B−A) mod π`:

- `θ_e` matches exactly one of `A.axes[0], A.axes[1]` — call its index
  `ax_A ∈ {0, 1}`.
- Symmetrically `ax_B ∈ {0, 1}` at `B`.
- **Adjacent grid corners flip parity**, so `ax_A ≠ ax_B` for every
  legitimate 4-connected edge.

Diagonals (`i ± j = const` neighbors) are NOT 4-connected in the graph;
v2 does not build edges for them.

---

## 2. Pipeline invariants

A labelled corner `C` at `(i, j)` is **kept** iff all of the following
hold after convergence:

1. **Axis membership.** Both `C.axes[0]` and `C.axes[1]` are within
   `cluster_tol_deg` of the two global grid-direction peaks
   `{Θ₀, Θ₁}`, each axis matching a different peak.
2. **Cluster label = axis-slot.** `cluster(C) = 0` iff `C.axes[0]` is
   closer to `Θ₀`, else `1`. Binary, per-corner.
3. **Parity.** `cluster(C) == (i + j) mod 2`, modulo a sign flip fixed
   by the seed quad.
4. **Edge orientation along the corner's axes (no ±π/4 offset).** For
   every in-graph edge `C ↔ N` with vector `v = N.pos − C.pos`, the
   angle `atan2(v) mod π` is within `edge_axis_tol_deg` of exactly one
   of `C.axes[0].angle, C.axes[1].angle` AND of exactly one of
   `N.axes[0].angle, N.axes[1].angle`.
5. **Edge axis-slot swap.** Let `ax_C ∈ {0, 1}` be the slot of `C`
   matching the edge, and `ax_N` the slot of `N`. Require `ax_C ≠ ax_N`.
6. **Cell-size consistency.** `|v| ∈ [1 − step_tol, 1 + step_tol] × s`
   (local cell size).
7. **Line collinearity.** For every labelled line through `C` (row,
   column) with `≥ 3` members, `C`'s perpendicular residual to the
   fitted line is `≤ line_tol × s`. With `≥ 4` members, fit a
   projective-line (a chessboard row/column is the image of a straight
   line under a homography — it remains straight under a single
   homography and curves only under lens distortion); use a tighter
   tolerance for straight fits, a looser one for polynomial fits.
8. **Local-H consistency.** A local 4-point homography from 4 non-
   collinear labelled neighbors predicts `C`'s pixel position with
   residual `≤ local_h_tol × s`.
9. **No ambiguity at attachment.** When admitted via prediction, no
   other candidate strong corner lies within `2 ×` the attachment
   distance.

A corner failing **any** invariant is blacklisted. If the blacklist
grew during a validation pass, the pipeline restarts from the seed
stage with blacklisted indices excluded (capped iterations).

---

## 3. Data flow

```text
Corner[]
 → 1. pre-filter: strength + fit-quality
 → 2. global grid-direction centers {Θ₀, Θ₁}  (axes-histogram + 2-means)
 → 3. hard cluster assignment per corner  (drop if axes don't match both centers)
 → 4. global cell-size s
 → 5. seed: pick the best 2×2 quad satisfying invariants 1–6 for all 4 edges
 → 6. grow: BFS over (i, j) boundary; attach one corner at a time,
        enforcing invariants 1–6, 9 at the admission step
 → 7. validate: check invariants 7, 8 across the labelled set;
        blacklist outliers via the attribution rules; if any new blacklist
        entries, restart from step 5 with the blacklist excluded
 → 8. RECALL BOOSTERS (post-convergence):
        8a. Line extrapolation — extend labelled rows/columns one corner
            at a time along projective-line fits.
        8b. Interior gap fill — attach corners at unlabelled (i,j)
            strictly inside the bbox with ≥3 labelled neighbors.
        8c. Component merge — run the full pipeline a second time with
            blacklisted corners excluded, try to grow a second seed
            disjoint from the first component, then merge by aligning
            the two (i,j) frames via local homography across the gap.
        8d. Weak-cluster rescue — corners dropped in Stage 3 with
            `max_d ∈ (cluster_tol, weak_cluster_tol]` become eligible
            as attachment candidates in 8a, 8b, 8c, with HALVED search
            radius and the full invariant stack still enforced.
   Every 8x step ADDS corners but never relaxes invariants 4–6, 9 at
   the attachment site. The post-validation (Stage 7) runs once more
   at the end on the enlarged labelled set — still with the "0 wrong
   corners" acceptance bar.
 → 9. output: Detection (one component) or None
```

Steps 2, 4 are global. Steps 5–7 are the **precision core**; any
labelled corner is guaranteed to pass all invariants. Steps 8a–8d are
the **recall boosters**; they extend the labelled set without
compromising the precision contract. Step 7 loops (capped).

---

## 4. Public API

```rust
// crates/chessboard-v2/src/lib.rs

pub struct Detector { pub params: DetectorParams }

impl Detector {
    pub fn new(params: DetectorParams) -> Self;
    pub fn detect(&self, corners: &[Corner]) -> Option<Detection>;
    pub fn detect_debug(&self, corners: &[Corner]) -> DebugFrame;
}

pub struct Detection {
    pub corners: Vec<LabeledCorner>,   // position, (i, j), cluster, local_h_residual
    pub grid_directions: [f32; 2],     // (Θ₀, Θ₁)
    pub cell_size: f32,
}
```

`DebugFrame` surfaces every input corner's stage transition plus
per-iteration traces; see §7.

---

## 5. Stage algorithms

### 5.1 Pre-filter (Stage 1)

Drop corner `c` if any of:
- `c.strength < min_corner_strength` (default `0.0`, off).
- `c.contrast > 0` and `c.fit_rms > max_fit_rms_ratio × c.contrast`
  (default `max_fit_rms_ratio = 0.5`, on).
- Both `c.axes[0].sigma` and `c.axes[1].sigma` equal the no-info
  sentinel `π` (the corner has no axis descriptor).

### 5.2 Global grid-direction centers (Stage 2)

**Input:** the strong corners from Stage 1.
**Output:** `(Θ₀, Θ₁)` with `0 ≤ Θ₀ < Θ₁ < π`, `Θ₁ − Θ₀ ≈ π/2`.

Algorithm:
1. Build a circular histogram on `[0, π)` with `num_bins` bins.
2. For every corner `c` and every axis `axes[k]`, add a vote at
   `c.axes[k].angle mod π` with weight `c.strength / (1 + c.axes[k].sigma)`.
3. Smooth with a length-5 Gaussian-like kernel `[1, 4, 6, 4, 1] / 16`.
4. Find local maxima; keep peaks with summed weight `≥ min_peak_weight_fraction × total_weight`.
5. Require at least two peaks separated by at least
   `peak_min_separation_deg` (default `60°`). If the two top peaks are
   not separated by `≥ 75°` (expected `~90°`), warn but continue.
6. Refine via **2-means over per-axis votes** (not per corner). Each
   axis vote is assigned to the nearest center; centers update as
   weighted circular means. Iterate up to `max_iters`, stop at fixed
   point.

This fixes the v1 Phase-4 regression where the histogram used both
axes but the refinement iterated per-corner on a single-axis field.

### 5.3 Per-corner cluster label (Stage 3)

For each survivor `c`:
1. Compute `d(axes[k], Θ_m) = angular_dist_pi(c.axes[k].angle, Θ_m)`
   for `k ∈ {0,1}`, `m ∈ {0,1}` (four distances).
2. Find the two assignments (Hungarian-style on 4 distances — trivial
   for a 2×2 matrix):
   - **Canonical**: `axes[0]→Θ₀, axes[1]→Θ₁`, total cost `d(0,0)+d(1,1)`.
   - **Swapped**: `axes[0]→Θ₁, axes[1]→Θ₀`, total cost `d(0,1)+d(1,0)`.
3. Pick the cheaper assignment; let `max_d` be the larger of the two
   distances in that assignment.
4. If `max_d > cluster_tol_deg`, **drop** `c` (at least one axis
   matches neither center within tolerance).
5. Otherwise set `cluster(c) = 0` for the canonical assignment (axes[0]
   closer to Θ₀), `1` for the swapped assignment.

Corners with `cluster ∈ {0, 1}` pass to downstream stages.

### 5.4 Global cell size (Stage 4)

Reuse `projective_grid::estimate_global_cell_size` on the clustered
corner positions. Fallback: for each corner, its 4 nearest neighbors;
take the median of all per-corner medians. Output: `s` (scalar, px).

An optional caller hint `cell_size_hint: Option<f32>` narrows the
search: if `|s - hint| / hint > 0.3` we log a warning and trust the
estimate.

### 5.5 Seed (Stage 5)

Find the best 2×2 cell `A, B, C, D` labelled:

- `A = (0, 0)` with cluster 0,
- `B = (1, 0)` with cluster 1,
- `C = (0, 1)` with cluster 1,
- `D = (1, 1)` with cluster 0.

All 4 seed edges `AB, CD, AC, BD` must satisfy invariants 4–6:

- `AB` direction matches `A.axes[0]` and `B.axes[1]` (axis-slot swap).
- `AC` direction matches `A.axes[1]` and `C.axes[0]`.
- `BD` direction matches `B.axes[1]` and `D.axes[0]` — wait, for a
  parity-0 `D` the edge `BD` is a column edge; it matches `B.axes[0]`
  and `D.axes[1]`. Same for `CD`.
- Every edge length in `[1 − seed_edge_tol, 1 + seed_edge_tol] × s`.
- The quad closes: `|D − (A + (B − A) + (C − A))| < seed_close_tol × s`
  (parallelogram consistency).

**Search strategy.** Iterate cluster-0 corners by descending strength.
For each candidate `A`:
1. kNN-search cluster-1 corners within `[1 − tol, 1 + tol] × s`.
2. For each neighbor `B` in the right-ish half-plane along `A.axes[0]`:
   verify `AB` passes invariants.
3. For each neighbor `C` in the down-ish half-plane along `A.axes[1]`:
   verify `AC` passes invariants.
4. Predict `D ≈ B + (C − A)`. Search cluster-0 corners within
   `seed_close_tol × s` of the prediction; verify `BD` and `CD` pass.
5. First quad passing every check wins.

If no quad passes, fall back to a second pass with each tolerance
multiplied by 1.5; if still none, return `None`.

### 5.6 Growth (Stage 6)

State:
- `labelled: HashMap<(i,j), CornerIdx>`.
- `rejected: HashSet<(i,j)>` (ambiguous, no candidate, or failed invariants).
- `boundary: VecDeque<(i,j)>` of labelled-adjacent unlabelled positions.

Seed inserts `(0,0), (1,0), (0,1), (1,1)` into `labelled`, pushes their
4 cardinal unlabelled neighbors into `boundary`.

Loop while `boundary` non-empty:
1. Pop `(i, j)`. Required cluster `k = (i + j) mod 2` (XOR with the
   seed parity-offset, see §10.5).
2. Collect labelled neighbors in a 3×3 window around `(i, j)`. If fewer
   than 3, defer — re-push with a decremented attempt counter; drop
   after 3 attempts.
3. Fit a local affine `(i, j) → (x, y)` from neighbors (least squares).
   If ≥ 4 non-collinear neighbors available, use the 4 closest (in
   grid-space) to fit a local homography instead. Predict `p̂`.
4. **Candidate search.** Find strong corners `c` not already labelled
   nor blacklisted, satisfying:
   - `cluster(c) == k`,
   - both axes match `{Θ₀, Θ₁}` within `attach_axis_tol_deg`,
   - `|c.pos − p̂| ≤ attach_search_rel × s`.
5. If 0 candidates: mark `(i, j)` as `Hole`; continue.
6. If ≥ 2 candidates and the second-nearest is within
   `attach_ambiguity_factor × ` the nearest distance: mark `(i, j)` as
   `Ambiguous`; continue (no blacklist).
7. Let `c*` be the unique nearest. Verify, for each labelled neighbor
   `N` of `(i, j)`, that the edge `c* ↔ N` satisfies invariants 4–6
   (direction along axes, slot swap, length within cell-size window).
   If any edge fails: continue as `Hole` — do **not** blacklist `c*`
   (it may be a real corner that just mis-predicts at this position).
8. Label `c*` as `(i, j)`, cluster `k`. Push its unlabelled cardinal
   neighbors into `boundary`.

Growth ends when `boundary` is empty.

### 5.7 Post-growth validation (Stage 7)

Runs on the labelled set; may blacklist corners and trigger a restart
from Stage 5 with the blacklist excluded.

**7a. Line collinearity.**
For each row (`j = const`) and column (`i = const`) with ≥ `line_min_members`
labelled members:

1. Sort members by the other index (`i` for rows, `j` for columns).
2. If members ≥ 4: fit a line by least squares with Huber weights,
   plus a projective-parametrisation variant — fit one homography to
   `(k, 0) → (x_k, y_k)` where `k` is the member's index-along-line.
   Pick the better fit by χ² and record per-member residual.
3. Compute each member's residual (perpendicular distance for a
   straight fit; projective residual for a projective fit).
4. Flag members with residual `> line_tol × s` (straight) or
   `> projective_line_tol × s` (projective; looser because the fit
   accommodates distortion).

Per-corner bookkeeping: `line_flag_count[c] += 1` each time `c` is
flagged in any line.

**7b. Local-H residual.**
For each labelled `C` with ≥ 4 non-collinear labelled neighbors in
grid space:
1. Pick the 4 closest neighbors in grid space (tie-breaking by
   Euclidean distance).
2. Fit `homography_from_4pt` mapping their `(i, j)` to their pixel
   positions.
3. Predict `C`'s pixel position. Record residual `r_C`.
4. If `r_C > local_h_tol × s`, flag `C`.

**7c. Outlier attribution.**
Mapping from flags to blacklist decisions:

- `line_flag_count[c] ≥ 2` → `c` is the outlier (flagged in
  independent lines): **blacklist**.
- `r_C > 2 × local_h_tol × s` AND `line_flag_count[c] ≥ 1` →
  `c` is the outlier: **blacklist**.
- `r_C > local_h_tol × s` AND no line flag on `c` AND at least one of
  the 4 base neighbors has `line_flag_count ≥ 1` → the **base** is the
  outlier; do not blacklist `C`. Scan the 4 base neighbors; blacklist
  the one with the highest `line_flag_count`.
- `r_C > local_h_tol × s` AND no line flags anywhere → insufficient
  evidence; defer (no blacklist this iteration).

**7d. Restart.**
If the blacklist grew in this pass, re-run Stage 5 (seed) onwards with
`corners \ blacklist`. Cap at `max_validation_iters = 3`.

### 5.8 Recall boosters (Stage 8)

Run **only after** Stage 7 has converged with no new blacklist
entries. Each booster is independently toggleable; all four are on by
default. The invariants 1–6, 9 remain in force at every attachment
attempt.

The booster pool is `eligible = strong_corners \ labelled \ blacklist`.
Under `weak_cluster_rescue`, it is extended with corners that failed
Stage 3 by a margin `≤ weak_cluster_tol_deg` (beyond `cluster_tol_deg`
but not by much).

Each booster attempt may add at most one corner at a time; the outer
loop repeats until no booster attaches anything in a full pass (cap
`max_booster_iters = 5`).

**8a. Line extrapolation.**

For every labelled line (each row `j = const`, each column
`i = const`) with `≥ 3` labelled members:

1. Fit a projective line through the labelled members (straight line
   under ideal homography; falls back to straight LSQ fit when the
   4-point projective fit is ill-conditioned).
2. Try to extend to `(i_min - 1, j)` (left end) and `(i_max + 1, j)`
   (right end), and similarly for columns.
3. For each candidate end-position, predict the pixel location by
   extrapolating the line AND using a local homography from the last
   2–3 labelled members + the same-side neighbor in the perpendicular
   direction (if available). Blend by averaging when both predictions
   agree within `0.15 × s`; otherwise prefer the homography
   prediction when it has ≥ 4 base corners.
4. Search the `eligible` pool for candidates within
   `attach_search_rel × s` of the predicted position; apply the full
   attachment invariant stack (cluster parity, axes alignment,
   uniqueness, edge invariants 4–6 against all labelled neighbors).
5. If a unique candidate passes, label it; re-queue the line to try
   another extension. Stop when extension fails twice in a row on
   that line.

**8b. Interior gap fill.**

For every unlabelled `(i, j)` strictly inside the labelled bounding
box with `≥ 3` labelled neighbors in a 3×3 window:

1. Predict pixel position via local affine from the labelled
   neighbors (prefer 4+ neighbors → 4-point homography).
2. Find the unique candidate under the attachment invariant stack
   (§5.6 step 4 + step 7).
3. Attach if present; otherwise record as `Hole` and move on.

This mirrors v1's Phase-5 gap-fill but tightens the attachment
criteria (parity + axes + no ambiguity).

**8c. Connected-component merge.**

After Stage 7 converges, freeze the current labelled set as
`component_A`. If `|component_A| < target_corners` (e.g. < 60% of the
expected board corners) AND there remain `≥ min_labeled_corners`
eligible corners in the pool:

1. Run Stages 5–7 on `pool \ component_A`, producing a second labelled
   set `component_B`.
2. If `component_B` is empty, stop (no second component).
3. Otherwise attempt a merge:
   - Identify the nearest labelled pair `(a ∈ A, b ∈ B)` in image
     space.
   - Using `component_B`'s own `(i, j)` frame, map it into `A`'s
     frame via a local homography fitted from the 4 closest pairs
     across the A–B boundary. Produce a candidate
     `(i, j)_{A-frame}` for every corner in `B`.
   - Verify every candidate mapping preserves the parity rule:
     `cluster(b) == (i_A + j_A) mod 2 ⊕ seed_parity_offset`.
   - Verify that the edge geometry across the A-B gap (any new edges
     the merge would create) passes invariants 4–6.
   - Verify the combined labelled set still passes line-collinearity
     (Stage 7a) across the join — line fits that span both components
     must be consistent.
4. If all checks pass, merge into a single `(i, j)` frame and proceed
   to Step 8d / final validation. Otherwise discard `component_B`.

Component merge supports at most 2 components per frame (calibration
never benefits from more; each additional component doubles the
failure surface).

**8d. Weak-cluster rescue.**

During each of 8a, 8b, 8c: expand the `eligible` pool to include
corners that failed Stage 3 by a margin up to
`weak_cluster_tol_deg` (e.g. 18° when `cluster_tol_deg = 12°`).
When such a corner is proposed as an attachment candidate, apply:

- HALVED search radius (`0.5 × attach_search_rel × s`),
- full invariant stack (edges 4–6 + parity + uniqueness) at the
  attachment site,
- and an extra per-corner line-collinearity check: the corner must
  lie on at least one fitted line within `0.5 × line_tol × s`.

Weak-rescue is the last resort — it lets us reclaim corners on the
edge of clustering tolerance at positions we're already confident
about, without polluting the precision core.

---

## 6. Parameters

```rust
pub struct DetectorParams {
    // Stage 1
    pub min_corner_strength: f32,            // 0.0 (off)
    pub max_fit_rms_ratio: f32,              // 0.5

    // Stage 2 + 3
    pub num_bins: usize,                     // 90 (2° per bin)
    pub cluster_tol_deg: f32,                // 12.0
    pub peak_min_separation_deg: f32,        // 60.0
    pub min_peak_weight_fraction: f32,       // 0.05
    pub max_iters_2means: usize,             // 10

    // Stage 4
    pub cell_size_hint: Option<f32>,         // None

    // Stage 5 (seed)
    pub seed_edge_tol: f32,                  // 0.15 → [0.85, 1.15] × s
    pub seed_axis_tol_deg: f32,              // 10.0
    pub seed_close_tol: f32,                 // 0.15 × s (parallelogram closure)

    // Stage 6 (grow)
    pub attach_search_rel: f32,              // 0.3 × s
    pub attach_axis_tol_deg: f32,            // 10.0
    pub attach_ambiguity_factor: f32,        // 2.0
    pub step_tol: f32,                       // 0.15
    pub edge_axis_tol_deg: f32,              // 10.0

    // Stage 7 (validate)
    pub line_tol_rel: f32,                   // 0.15 × s
    pub projective_line_tol_rel: f32,        // 0.25 × s
    pub line_min_members: usize,             // 3
    pub local_h_tol_rel: f32,                // 0.20 × s
    pub max_validation_iters: u32,           // 3

    // Stage 8 (recall boosters)
    pub enable_line_extrapolation: bool,     // true
    pub enable_gap_fill: bool,               // true
    pub enable_component_merge: bool,        // true
    pub enable_weak_cluster_rescue: bool,    // true
    pub weak_cluster_tol_deg: f32,           // 18.0
    pub component_merge_min_boundary_pairs: usize, // 2
    pub max_booster_iters: u32,              // 5

    // Stage 9
    pub min_labeled_corners: usize,          // 8
}
```

All spatial tolerances are **multiplicative with respect to `s`**; the
pipeline is scale-invariant once `s` is known. All angular tolerances
are absolute (degrees).

---

## 7. Debug surface

```rust
pub enum CornerStage {
    Raw,
    Strong,                                  // passed Stage 1
    NoCluster { max_d_deg: f32 },            // Stage 3 rejected
    Clustered { cluster: usize },            // Stage 3 accepted
    AttachmentAmbiguous { at: (i32, i32) },  // Stage 6 step 6
    AttachmentFailedInvariants { at: (i32, i32), reason: String }, // step 7
    LabeledThenBlacklisted { at: (i32, i32), reason: String }, // Stage 7
    Labeled { at: (i32, i32), local_h_residual_px: Option<f32> },
}

pub struct IterationTrace {
    pub iter: u32,
    pub seed: Option<[usize; 4]>,
    pub labelled_count: usize,
    pub new_blacklist: Vec<(usize, String)>,
    pub converged: bool,
}

pub struct DebugFrame {
    pub input_corners: Vec<CornerDebug>,    // position, axes, strength, stage
    pub grid_directions: Option<[f32; 2]>,
    pub cell_size: Option<f32>,
    pub iterations: Vec<IterationTrace>,
    pub result: Option<Detection>,
}
```

The Python overlay script consumes `DebugFrame` and renders:
- `Labeled` corners as gold dots with `(i, j)` annotation.
- Grid-neighbor edges only (undirected, `|Δi|+|Δj| = 1`).
- `LabeledThenBlacklisted` corners as red X with reason.
- Stray `Strong / Clustered` corners NOT shown by default.

---

## 8. Iteration loop

```python
def detect(corners):
    strong = pre_filter(corners)
    if len(strong) < MIN: return None

    centers = grid_direction_centers(strong)
    if centers is None: return None

    clustered = [c for c in strong if assign_cluster(c, centers) is not None]
    if len(clustered) < MIN: return None

    s = global_cell_size(clustered)

    blacklist = set()
    for it in range(MAX_VALIDATION_ITERS):
        pool = [c for c in clustered if c.idx not in blacklist]
        seed = find_seed(pool, centers, s)
        if seed is None: return None
        labelled = grow_from_seed(pool, seed, centers, s)
        if len(labelled) < MIN_LABELED: return None
        new_bad = validate(labelled, s)
        if not new_bad:
            return assemble_detection(labelled, centers, s)
        blacklist |= new_bad
    return None
```

---

## 9. Testing plan

Unit tests per stage in `crates/chessboard-v2/tests/synthetic.rs`:

- **Cluster centers.** Synthetic axes pairs at `{30°, 120°}` and
  `{40°, 130°}` with ±5° noise, parity-flipping slot assignment →
  centers recover within 1°, all corners are clustered.
- **Cluster assignment.** Corners with one axis at cluster-tol + 5°
  off both centers → dropped. Corners with orthogonal axes matching
  both centers → labelled.
- **Seed finder.** Clean 4×4 synthetic axes-correct grid → returns one
  of the 3×3 cells as seed quad. 4×4 grid with one false corner
  injected at cluster-swapped parity → seed avoids it.
- **Growth.** 5×5 clean grid → all 25 labelled. 5×5 with one parity-
  wrong false corner → labelled count = 24 (the false corner is
  skipped).
- **Validation.** 7×7 grid with one mislabeled corner injected →
  blacklist pinpoints it in ≤ 1 iteration. 7×7 with one bad base
  neighbor (the base is mis-positioned but has correct axes and
  cluster) → blacklist correctly attributes to the base, not to the
  otherwise-well-predicted target.

Integration in `tests/dataset.rs` (feature-gated) on
`testdata/3536119669/target_*.png`:

- 120 snaps, run through v2, emit per-snap `DebugFrame` JSONL.
- Precision gate (HARD — a fail means v2 regressed):
  - **0** frames with a wrongly-labelled corner. Visual spot-check of
    every labelled corner in the 22 frames flagged in
    `docs/120issues.txt`; each must lie at a real cell intersection.
  - No detection with a per-corner local-H residual > 1.5 px post-
    convergence.
- Recall targets (compared against v1 Phase-5's 84.2% at ≥9 corners):
  - **≥ 90 %** of the 116 non-excluded frames detect with ≥ 20
    labelled corners after Stage 8 boosters.
  - **≥ 6 / 7** of the missing-recoverable frames from
    `docs/120issues.txt` recovered (boosters 8a, 8b should hit them).
  - **≥ 1** component-merge success on frames that v1 Phase-5
    returned empty because the seed never had ≥ min_labeled corners
    (booster 8c).
  - Per-frame median labelled corners ≥ v1's (currently ~50) — no
    single-frame regression beyond a 5-corner tolerance.
- Ablation run with every booster disabled → the precision core
  alone must still hit ≥ 60 % recall and 0 wrong corners.

---

## 10. Open questions

1. **Degenerate axes.** When `axes[0].sigma` OR `axes[1].sigma` is
   near `π` (no info on that axis): drop the corner, or vote with the
   one good axis only? Current proposal: drop — it's simpler and
   safer. Alternative: vote with the one axis and flag the corner as
   "single-axis", deferring it to late-stage gap-fill.
2. **Seed retry policy.** If the first seed grows to < min-labeled,
   (a) try the next-best seed; (b) blacklist the current seed and
   re-search. (a) is cheaper. Preferring (a).
3. **Distortion-curved lines.** Under strong lens distortion a row
   bends visibly. Options:
   - (a) straight-line fit with looser tolerance,
   - (b) projective-line parameterisation (straight under ideal
     homography, curves only under distortion), looser tolerance,
   - (c) piecewise local collinearity over consecutive triplets only.

   Preference: **(b) when ≥ 4 members, (c) for triplets**. (a) is the
   least informative.
4. **Multi-seed growth.** Preference: **single seed only**.
   Calibration wants a single consistent frame; multi-seed stitching
   is out of scope for v2.
5. **Seed parity offset.** The seed fixes which cluster is "even
   parity". Convention: the seed's top-left corner `A` is parity 0.
   This choice is arbitrary and internal — consumers only see
   `(i, j)`.
6. **Cell-size hint from caller.** Useful when known (dataset-specific
   runs). Make `cell_size_hint` optional; when provided and
   `|s - hint| / hint < 0.3`, lock `s = hint` (tighter gates in Stages
   5–6). Otherwise warn and use the estimate.

---

## 11. Not in scope for v2

- Multi-board detection (multi-component merge).
- Global homography fitting / rectification.
- Subpixel refinement of ChESS corners.
- ChArUco marker decoding.
- API back-compat with the existing
  `calib_targets_chessboard::ChessboardDetector`.

These remain in `calib-targets-chessboard`. v2 is grid-only.

---

## 12. Workspace migration prerequisite: remove `Corner::orientation`

v2 assumes the axes-only contract (§1). The existing workspace still
carries `Corner::orientation: f32` as a legacy single-axis field that
predates `axes`. Every v1 validator and the legacy clustering read it.

**Migration plan** (separate commits, lands **before** v2 is wired in
any production code path):

1. Rewrite `cluster_orientations` in
   `crates/calib-targets-core/src/orientation_clustering.rs` so that
   the histogram and 2-means refinement both consume axes (both
   `axes[0]` and `axes[1]`, per-axis weights `1 / (1 + σ)`). The
   `use_dual_axis` feature flag disappears — dual-axis IS the only
   mode.
2. Migrate `ChessboardSimpleValidator` and any `orientation`-reading
   code in `crates/calib-targets-chessboard/src/gridgraph.rs` to use
   `axes` directly. The Simple validator's orthogonality check becomes
   "axes[0] and axes[1] are orthogonal within tolerance" (trivial
   per-corner precondition).
3. Remove `pub orientation: f32` from `Corner` in
   `crates/calib-targets-core/src/corner.rs`.
4. Remove `pub orientation_cluster` if it was derived from
   `orientation` only. In the axes-only world the cluster is a
   function of `(axes[0], axes[1], centers)` computed by the detector;
   it doesn't need to be cached on `Corner`. Keep as a transient
   field in the v1 detector's internal state if removal is too
   invasive.
5. Update Python / WASM / FFI bindings and tests that construct
   `Corner { orientation: ..., .. }`. Tests usually initialise via
   `..Corner::default()`; this change is mostly mechanical.
6. Update `adapt_chess_corner` in every consumer crate — it previously
   populated `orientation` from `axes[0]`; now it simply copies `axes`.
7. Regenerate FFI header and Python typing stubs per the CLAUDE.md
   parity rule.

v2 can be **drafted** (Phases A–C) against the current `Corner` by
ignoring `orientation`, but v2 **ships** only after the migration is
merged, since v2's clustering is axes-based and conflicts philosophy
with leaving `orientation` around.
