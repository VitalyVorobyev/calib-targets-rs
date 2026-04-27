# From a corner cloud to a labelled grid: inside `projective-grid`

> *Building a `(i, j)`-labelled lattice over a cloud of detected X-corners is
> the core combinatorial step in every chessboard / ChArUco / PuzzleBoard
> detector. This note walks through how the `projective-grid` crate solves
> that problem, situates each step in the published literature, and surfaces
> the algorithmic gaps worth closing next.*

---

## 1. Problem statement

The detector pipeline starts with a feature detector — typically the
**ChESS** X-junction detector of Bennett & Lasenby [1] — that returns a
list of saddle-like corner candidates:

```text
struct Corner {
    position: Point2<f32>,            // sub-pixel pixel coordinates
    axes:     [AxisEstimate; 2],      // two undirected grid axes (mod π)
    strength: f32,                    // peakiness of the X-junction
    ...
}
```

The detector runs over the whole image and emits a *cloud*: hundreds to
thousands of candidates, where each candidate may

- sit on the calibration target at integer-grid `(i, j)` we want to recover,
- sit inside an ArUco/AprilTag marker (a parasitic X-corner at marker scale),
- be a generic image feature that happens to look like an X.

`projective-grid` answers a single question:

> **Given a cloud of 2D corner candidates with two-axis orientation, return
> the integer `(i, j)` label of every candidate that lies on a regular
> projective grid, with no false labels.**

"No false labels" is the precision-by-construction contract: a wrong `(i, j)`
poisons calibration; a missing `(i, j)` does not. Every algorithm in the
crate is biased toward dropping rather than mislabelling.

---

## 2. The pipeline at a glance

The crate ships **two labelling strategies** behind a single
[`calib_targets_chessboard::GraphBuildAlgorithm`] enum, both producing
the same `(i, j) → corner_idx` map so downstream consumers
(chessboard, ChArUco, marker board, PuzzleBoard) are agnostic to which
ran:

- **Seed-and-grow with global-H boundary extension** (`ChessboardV2`,
  default). Entry point `square::grow::bfs_grow` driven by a 2×2 seed
  and a pattern-specific `GrowValidator`. After BFS converges,
  `square::grow_extension::extend_via_global_homography` extends the
  labelled set past the BFS reach using a single global homography,
  gated on pixel-unit reprojection residuals. The historical default;
  every published detector pipeline used this until April 2026.

- **Topological grid finder** (`Topological`, opt-in). Entry point
  `topological::build_grid_topological`. Delaunay-triangulates the
  corner cloud (Shu/Brunton/Fiala 2009 [14]), classifies each
  Delaunay edge as *grid* / *diagonal* / *spurious* using the
  per-corner ChESS axes (replacing the paper's image-color test),
  merges triangle pairs sharing a diagonal into chessboard-cell
  quads, prunes by quad-mesh degree and parallelogram geometry, then
  flood-fills `(i, j)` labels through the quad mesh. Image-free —
  consumes only positions and axes, so the crate stays standalone.

Both pipelines feed an optional shared **component-merge** pass
(`projective_grid::component_merge`) that reunites disconnected
labelled components using local geometry only — no global homography,
to survive heavy radial distortion that would break a global fit.

ChArUco pins to `ChessboardV2` regardless of caller choice: marker
cells carry sub-cell features whose ChESS axes lock to the marker's
local orientation, defeating the per-cell axis test. The override
lives in
[`calib_targets_charuco::CharucoDetector::new`](../crates/calib-targets-charuco/src/detector/pipeline.rs).

A historical "Pipeline A" (KD-tree slot graph + cleanup +
`assign_grid_coordinates`) was removed in the April 2026 refactor. The
shared primitives that motivated it — circular statistics, homography
estimation, mean-shift cell-size — survive as standalone modules; the
graph/traversal layer was deleted because no production detector
called it.

### 2.1 Life cycle

```text
   raw ChESS corners  →  pre-filter (strength, fit RMS)
                      →  axis clustering              [circular_stats]
                      →  seed search                  [pattern crate, e.g. chessboard::seed]
                      →  BFS grow                     [square::grow::bfs_grow]
                      →  validation pass              [square::validate]
                      →  global-H boundary extension  [square::grow_extension]
                      →  re-validate (loop on new attachments)
                      →  recall boosters              (pattern-specific)
                      →  TargetDetection
```

`projective-grid` owns every stage in this list except clustering and
seed *finding* — those live in the pattern crate because they involve
pattern-specific semantics (parity, axis-slot swap, marker labels).
The **building blocks** they call — `circular_stats`,
`seed_cell_size`, `seed_has_midpoint_violation`, `find_quad` — are
hoisted into `projective-grid` so all three pattern crates share them.

---

## 3. From angles to grid axes — `circular_stats`

Every X-junction comes with two undirected axis angles in `[0, π)`. The two
*global* grid directions are recovered by:

1. **Histogram + low-pass smoothing.** Build a circular histogram of axis
   votes weighted by `strength / (1 + sigma)`. Smooth with the binomial
   `[1, 4, 6, 4, 1] / 16` kernel. The kernel is a discrete approximation
   to a 1-pixel σ Gaussian and preserves total mass exactly.

2. **Plateau-aware peak picking.** Scan for local maxima, detecting
   *plateaus* (runs of equal-valued bins) and reporting their midpoint.
   This handles the case where a physical direction's mass straddles a
   bin boundary and smoothing produces two equal adjacent bins. Without
   plateau awareness the strict `> left && > right` test would drop both
   bins, lose the peak, and silently mis-label the entire grid.

3. **Double-angle 2-means refinement.** Refine the two seed centers via
   weighted 2-means *with the double-angle trick*: accumulate
   `Σ wᵢ·(cos 2θᵢ, sin 2θᵢ)` per cluster and halve the resulting
   `atan2`. This is the standard circular mean for **axial** (mod-π)
   data — see Mardia & Jupp [2], chapter 9. Naïvely accumulating
   `(cos θ, sin θ)` is wrong for mod-π data: votes near 0° and 180°
   cancel rather than reinforcing, giving a garbage centre near 90°.
   This bug actually shipped in an earlier version of the detector and
   the fix is documented in the workspace `CLAUDE.md`.

The combination of plateau-aware peak picking and double-angle 2-means is
what lets the detector tolerate boards rotated to arbitrary angles in the
image without ever crossing a 0°/180° seam.

**Paper context.** The histogram-+-mean-shift / EM-style approach to
finding two dominant directions is classical (Stephens [3], Fisher [4]).
For chessboards specifically, the line-extraction step in Geiger et al.'s
"Automatic Camera and Range Sensor Calibration Using a Single Shot" [5]
also reduces to picking two peaks of the gradient orientation
distribution. Our contribution is purely on the numerical side: making
the peak-picking robust to bin-edge plateaus and using the correct
mod-π circular mean for the iterative refinement.

---

## 4. Cell-size estimation — `global_step` and `local_step`

A naïve "set `min_spacing` and `max_spacing` from priors" approach fails:
real images come at different scales, and ChArUco-style boards have a
**bimodal** nearest-neighbour distance distribution (board-cell scale plus
marker-internal scale ≈ 0.2× board scale). So the crate ships two
data-driven estimators:

### 4.1 Global step — `estimate_global_cell_size`

Algorithm:

1. KD-tree over input positions.
2. For each point, take its single-nearest distance.
3. Sort and seed mean-shift from the 25th, 50th, and 75th percentile.
4. Iterate Epanechnikov-kernel mean-shift with bandwidth =
   `0.15 × seed`.
5. **Score each converged mode by `support × cell_size`**, not just
   support. This breaks ties in favour of the larger cell-size mode —
   exactly the tie-break needed to pick the *board* mode over the
   *marker-internal* mode on ChArUco data, since marker-internal corners
   are more numerous but sit at smaller spacing.

The double-mode awareness is the key practical contribution. Cheng [6]
and Comaniciu & Meer [7] established mean-shift as a non-parametric
mode finder; the `support × cell_size` re-weighting is a
domain-specific extension. See the docstring in `global_step.rs` for
the exact rationale.

### 4.2 Local step — `estimate_local_steps`

For every point, returns `(step_u, step_v, confidence, supporters_u,
supporters_v)` along the point's *own* two axes. The pipeline:

1. KD-tree query of `k_neighbors` (default 8).
2. Coarse outlier reject: drop neighbours past `3 × median(dist)`.
3. Fold the offset angle and the corner's two axes to `[0, π)`, then
   bin each surviving offset into the u-sector or v-sector by
   undirected angular distance, reject offsets whose closest sector
   axis is more than 30° away.
4. Epanechnikov-kernel 1-D mean-shift on the per-sector distance
   histograms; fall back to median if it doesn't converge.

This is the right level of abstraction for *step-aware* outlier
detection later: `find_inconsistent_corners_step_aware` uses
`(step_u + step_v) / 2` as the per-corner threshold scale instead of a
single global pixel value.

**Paper context.** Sector-based local lattice estimation appears in
Fitzgibbon's "Sub-pixel ChESS-like" extensions and in commercial OPC
software for printed-PCB alignment, but the published literature on
calibration-target detectors has historically assumed a single global
homography (Geiger et al. [5], OpenCV's `findChessboardCorners`).
Step-aware per-corner thresholds materially help under barrel/fisheye
distortion that varies the local cell size by 20-30 % across the image.

---

## 5. Building the graph

The crate offers **two labelling pipelines** selected by the
`GraphBuildAlgorithm` enum. Both consume positions + axes and emit a
`(i, j) → corner_idx` map. Their post-stages are identical: validate
(line + local-H residual), then optionally merge components.

### 5.1 `bfs_grow` (seed-and-grow, default)

The chessboard (when `graph_build_algorithm = ChessboardV2`), ChArUco,
and PuzzleBoard detectors all build their grid this way. Algorithm:

1. **Find a 4-corner seed quad** — pattern-specific. The chessboard
   detector picks one Canonical-cluster corner `A`, classifies its
   Swapped-cluster neighbours into axis-u and axis-v sectors, and
   tries pairs `(B, C)` such that `A, B, C, D = A + (B−A) + (C−A)`
   form a parallelogram with edge ratios within tolerance. The seed's
   own mean edge length is the `cell_size` carried forward — there is
   no global-cell-size *input*; cell size is an *output* of seed
   discovery.
2. **Grid-vector estimate.** `grid_u = (B − A)/|B−A|`, `grid_v` the
   same for `(C − A)`. These two unit vectors are the grow stage's
   only memory of the global grid orientation.
3. **BFS over `(i, j)` cells.** Initialise the boundary queue with the
   four cardinal neighbours of every seed cell. For each cell `(i, j)`
   pulled off the queue:

   a. Collect already-labelled neighbours within a 3×3 window
      around `(i, j)`.
   b. Predict the pixel position as the average of
      `pos(neigh_k) + (di_k · u + dj_k · v) · cell_size`.
   c. KD-tree query within `attach_search_rel × cell_size`
      (default 0.35) of the prediction.
   d. **Ambiguity check.** If the second-nearest candidate is within
      `ambiguity_factor × first_nearest` (default 1.5), mark the
      position ambiguous and refuse to attach — better a hole than a
      wrong label.
   e. Pattern-validator decides accept/reject of the chosen
      candidate. The chessboard's validator demands axis-cluster match
      (corner's two axes within `attach_axis_tol_deg` of the two
      global cluster centres) AND label match (the parity-required
      Canonical/Swapped at this `(i, j)` cell).
   f. Soft per-edge check: the just-attached corner needs at least
      one cardinal neighbour where the chessboard `edge_ok`
      invariant holds (axis-slot swap parity). Otherwise mark a
      hole and roll back.
   g. Enqueue the four cardinal neighbours of the just-attached
      cell.

4. **Rebase to origin.** Translate the labelled set so
   `min(i) = min(j) = 0`. This is a hard invariant on the workspace
   `LabeledCorner` API.

**Paper context.** The seed-and-grow strategy descends directly from
Geiger et al. [5] and Place et al. [10], who grow a chessboard from
a saddle-point seed by predictive nearest-neighbour search. ROCHADE
[11] adds saddle-point sub-pixel refinement on top. The novel pieces
in our implementation are:

- **Self-consistent cell size from the seed itself.** The seed's mean
  edge length replaces the bimodal-histogram `estimate_global_cell_size`
  call when ChArUco markers are present. This is documented in the
  workspace `CLAUDE.md` under "Cell-size estimation gotcha".

- **Hard ambiguity rejection.** Most published detectors take the
  nearest candidate above some confidence threshold. We refuse
  attachment whenever a second candidate sits inside a 1.5× distance
  ratio of the first — at the cost of recall, this kills the dominant
  false-label failure mode where a marker-internal corner sits roughly
  where a board corner would be.

- **`required_label_at` parity gate.** The chessboard-specific validator
  enforces that the corner at `(i, j)` carries the parity required by
  the seed's convention. Combined with the axis-cluster match and the
  rolled-back hole rule, this is what gives the detector its
  precision-by-construction property.

### 5.2 `extend_via_global_homography` (Stage 6)

After BFS has converged and validation has cleaned up outliers,
`square::grow_extension::extend_via_global_homography` extends the
labelled set past the BFS reach. BFS predictions are *local* — they
work off finite differences of labelled neighbours, which on the
boundary of the labelled set are one-sided. Under perspective
foreshortening, that one-sided estimate overshoots the next true
corner by more than the BFS search radius, and growth terminates.
Stage 6 fixes this by predicting cells from a **globally-fit
homography**.

Algorithm:

1. **Fit `H : (i, j) → image_pixel`** over the labelled set with DLT
   + Hartley normalisation (≥ `min_labels_for_h = 12`).
2. **Compute reprojection residuals** of the labelled corners against
   `H`, in pixel units.
3. **Trust gate** — refuse to extrapolate when median residual exceeds
   `max_median_residual_rel × cell_size` (default 0.10) or worst
   residual exceeds `max_residual_rel × cell_size` (default 0.30).
   This is the right knob for lens distortion: a moderate radial
   term inflates residuals, the gate fires, and Stage 6 becomes a
   safe no-op.
4. **Enumerate extension cells**: every interior hole, plus one step
   beyond the labelled bbox in each cardinal direction.
5. For each cell, predict via `H`, KD-tree query within
   `search_rel × cell_size` (default 0.40) of the prediction, and
   apply the **same gates BFS uses**:
   - parity (`required_label_at` × `label_of`),
   - tighter ambiguity (`ambiguity_factor = 2.5` vs BFS's 1.5 —
     boundary errors are unrecoverable),
   - `accept_candidate` (axis-cluster on chessboard),
   - `any_cardinal_edge_ok` (per-edge invariant).
6. **Single-claim attachment** — `labelled` and `by_corner` update
   immediately, so two cells whose predictions both land near the
   same corner cannot both attach it.
7. Iterate up to `max_iters = 5` until no new attachments.

Why the trust gate uses pixel residuals, not SVD ratios: the
homography matrix's singular-value spread depends on coordinate
scale and translation magnitude. Pixel-unit residuals are
scale-aware and behave consistently across image scales.

The function is a no-op when the labelled set is too small, the DLT
solver fails, or the residual gate fires. In all three cases the
labels survive untouched and `ExtensionStats::h_trusted = false`.

### 5.3 `build_grid_topological` (Shu 2009, axis-driven variant)

Selected via `graph_build_algorithm = Topological`. Skips axis
clustering and global cell-size estimation entirely — the algorithm
is image-free and works directly off the ChESS corner positions and
per-corner axes. Pipeline:

1. **Pre-filter.** Drop corners whose two axes both have
   `sigma >= max_axis_sigma_rad` (default 34°). The remainder is
   passed to Delaunay.

2. **Delaunay triangulation** of all surviving positions, via the
   `delaunator` crate (Mapbox port of Watson's algorithm [15]). Returns
   triangle vertex indices plus half-edge buddy pointers for O(1)
   neighbour lookup.

3. **Axis-driven edge classification.** For each Delaunay half-edge
   `(a → b)`, compute the edge angle `θ = atan2(p_b − p_a)`. At each
   endpoint, take the minimum angular distance from `θ` to the
   corner's two axes (modulo π — axes are undirected). Classify:

   - **Grid** if the min-distance is within `axis_align_tol_rad`
     (default 15°) at both endpoints — the edge runs along a cell
     side.
   - **Diagonal** if the min-distance is within
     `diagonal_angle_tol_rad` (default 15°) of `π/4` at both
     endpoints — the edge crosses a cell diagonal.
   - **Spurious** otherwise — background, noise, or a corner whose
     local axes don't align with the global grid (e.g. corners
     inside an ArUco marker).

   This replaces Shu's original color test (which sampled triangle
   interior pixels). The axis test naturally rejects background
   corners and stays valid under any image rotation, but breaks
   when the per-corner axes locally rotate fast — see Gap 8 below.

4. **Triangle-pair merging.** A triangle is *mergeable* iff exactly
   one of its three edges is `Diagonal` and the other two are `Grid`.
   For each mergeable triangle, look up the buddy of its diagonal
   half-edge. If the buddy's triangle is also mergeable with the
   same diagonal, the two triangles fuse into a quadrilateral whose
   four perimeter edges are all grid edges — one chessboard cell.
   Quads are stored TL/TR/BR/BL in clockwise order around their
   centroid, matching the workspace `LabeledCorner` quad convention.

5. **Topological filtering** (paper §4). For each corner in the
   quad mesh, count the number of incident perimeter half-edges. A
   regular interior corner has 4 incident quad edges (= 8 half-edge
   incidences). Corners exceeding 8 incidences are *illegal* —
   typically caused by a noise corner near a real grid intersection.
   Quads with ≥ 2 illegal corners are dropped.

6. **Geometric filtering** (paper §4). For each surviving quad,
   compute the ratio of opposing edge lengths. Drop the quad if
   either ratio exceeds `edge_ratio_max` (default 10.0, matching the
   paper). This is a generous bound — the gate fires only on
   pathologically degenerate quads.

7. **Topological walking** (paper §5). Each connected component of
   the quad mesh is labelled independently. Pick an arbitrary seed
   quad, assign its four corners `(0,0), (1,0), (1,1), (0,1)` in CW
   order, then BFS through neighbour quads sharing a perimeter edge.
   For each neighbour, the two shared corners' labels are already
   set; the other two get `current_label + outward_step` where
   `outward_step` is the integer cell-step direction perpendicular
   to the shared edge, away from the parent quad. Pure topology — no
   pixel-coordinate predictions, so radial distortion that bends
   grid lines does not break the labelling.

8. **Bounding-box rebase.** Each component's labels are translated so
   `min(i) = min(j) = 0`. Workspace invariant.

The output is `Vec<TopologicalComponent>` plus a
`TopologicalStats` diagnostic counter for triangle composition
(mergeable / multi-diagonal / has-spurious / all-grid), edge
classification counts, and per-component sizes. The bench harness
surfaces these via `bench diagnose --algorithm topological`.

**Where it shines.** Clean PuzzleBoards and lightly-distorted
chessboards: dense recall *within* its labelled bbox, dramatically
faster than seed-and-grow on sparse boards (no axis clustering, no
seed search, no validation-blacklist loop). On the public
`testdata/puzzleboard_reference/example1-3` images, topological
typically labels 15-30% more corners than ChessboardV2 in 5-20× less
wall-clock time.

**Where it fails.** ArUco markers (corners detected inside marker
bits poison the per-corner axes) and extreme low-view-angle regions
(triangles span multiple cells, breaking the 1-diagonal-2-grid
classification — paper §2.1 Fig 4). The first case is dodged by
pinning ChArUco to seed-and-grow; the second is documented in Gap 8.

**Paper context.** Shu/Brunton/Fiala [14] proposed the
Delaunay-+-color-test pipeline for printed checkerboards. ROCHADE
[11] adds saddle-point sub-pixel refinement orthogonally. Our
contribution is the axis-driven cell test that lifts the image-free
constraint, plus the explicit triangle-composition diagnostic that
tells operators *why* the pipeline missed corners in distorted
regions (Gap 8).

---

## 6. Component merge — `projective_grid::component_merge`

When the labelling stage (either pipeline) leaves multiple
disconnected components — typical when an entire row of corners drops
below the strength threshold, or when topological filtering removes a
noisy quad in the middle of the board — `merge_components_local`
attempts to reunite them into a single labelling. **Local geometry
only**: never a global homography, because strong radial distortion
breaks a single global H across the whole board.

Acceptance criterion for merging two components `(C_p, C_q)`:

1. **Per-component cell size** estimated as the median nearest-
   neighbour pixel distance along the labelled `i` and `j` axes.
   Reject the pair upfront if `|s_p − s_q| / max(s_p, s_q) >
   cell_size_ratio_tol` (default 20%).

2. **Anchor search.** Try every (transform, anchor pair) where
   transform ranges over the eight elements of D4
   ([`crate::GRID_TRANSFORMS_D4`]). Each anchor pair fixes a
   candidate translation `Δ = ij_q − T(ij_p)`.

3. **Position scoring.** For each `(T, Δ)`, transform every label
   in `C_p` into `C_q`'s frame and count overlap. Accept if
   `overlap ≥ min_overlap` (default 2) and the worst per-corner
   position disagreement on overlapping labels is below
   `position_tol_rel × cell_size` (default 0.20 × cell_size).

4. **Greedy merge.** Sort surviving components by labelled count
   descending and merge in that order, re-running the alignment
   search on each pass until no further merges are possible or the
   `max_components` cap (default 4) is hit.

The current implementation (v1) requires ≥ 1 overlapping label
between merge candidates, which handles the most common gap-induced
splits. **Disjoint label sets** (e.g. two patches of the same board
separated by a missing row, where labels never coincide) are
out-of-scope until we add a "predict next corner from each side and
match" boundary check — see Gap 9.

The chessboard crate's historical `enable_component_merge` and
`component_merge_min_boundary_pairs` knobs were the placeholders for
this functionality; the actual implementation now lives in
`projective-grid` and is reused from both pipelines via the
`DetectorParams::component_merge: LocalMergeParams` field.

---

## 7. Validation — `square::validate`

Once the labelled set is built, two independent residual passes flag
outliers:

### 7.1 Line collinearity

For every row (`j = const`) and column (`i = const`) with ≥ 3 members,
fit a total-least-squares line in pixel space (closed-form via the 2×2
covariance eigenvector) and flag members with perpendicular residual
greater than `line_tol_rel × cell_size`.

This is the same axis-aligned line-fit used by Lucchese & Mitra [8].
The crate uses *image-space* TLS rather than fitting a projective line,
because under moderate radial distortion the chord between two true
grid corners is straighter in pixel space than the projective ideal
predicts.

### 7.2 Local-H residual

For every labelled corner, pick its 4 grid-closest non-collinear
labelled neighbours, fit a 4-point homography on those, predict the
corner's pixel position, and flag if the residual exceeds
`local_h_tol_rel × cell_size`.

This is conceptually a single-cell version of Zaragoza et al.'s
"as-projective-as-possible" warping [12]. We do not blend a global +
local model; we just use the local fit as a leave-one-out predictor.
The 4-point variant uses Hartley-normalised LU as a closed-form
4×8 solve (no SVD needed), which is exactly the construction in
Hartley & Zisserman [13], chapter 4.

### 7.3 Attribution

Three rules turn raw flags into a single blacklist of corner indices:

1. ≥ 2 line flags ⇒ outlier.
2. local-H residual > `2 × tol` AND ≥ 1 line flag ⇒ outlier.
3. local-H flag with no line flag, but a base neighbour has ≥ 1 line
   flag ⇒ blame the worst-line-flagged base instead.
4. Otherwise, defer (no blacklist entry this iteration).

The detector then re-runs seed/grow/validate with the blacklist
populated, looping up to `max_validation_iters` times. This is RANSAC's
"reject and re-fit" structure with a deterministic flag-+-attribution
inner loop instead of random sampling — it converges in 2–4 iterations
in practice, which is far cheaper than the hundreds of RANSAC samples
typical of feature-matching pipelines. The trade-off is that a single
catastrophic outlier can hijack the seed and we never re-find a clean
one; the chessboard detector mitigates this with `detect_all`, which
peels off accepted components and re-enters the pipeline.

---

## 8. Gaps and follow-ups

The pipeline ships zero wrong labels on the workspace's regression
datasets. The remaining structural gaps are tracked here. Resolved
items are kept in place as audit trail.

### Gap 1 — Generic seed finder for non-chessboard consumers (PARTIAL)

`projective_grid::square::seed_finder` ships `find_quad`,
`SeedQuadParams`, and `SeedQuadValidator` — the primitives a generic
finder needs. The chessboard's parity-aware finder
(`calib-targets-chessboard::seed`) is built on top.

What's missing: a default seed-finder that doesn't require axis
clusters (uses only edge-length consistency + midpoint violation),
for `SpatialSquareValidator`-style users with unoriented point
clouds. Tracked as deep-dive Phase 3.

### Gap 2 — `circular_stats` is `f32`-only (OPEN)

The rest of the crate is generic in `Float = RealField + Copy`.
`circular_stats` is hard-coded to `f32`. Consumers that want `f64`
precision throughout pay a type cast on every histogram pass.

**Fix.** Promote the helpers to `Float` generic; the only `f32`
constants are `PI` and the histogram smoothing kernel weights.
Tracked as deep-dive Phase 6.

### Gap 3 — `HomographyQuality` is not a stable production metric (OPEN)

`homography::HomographyQuality` returns SVD-derived ratios of the
unnormalised 3×3 H matrix. Those ratios depend on coordinate scale
and translation magnitude — they are not a stable
geometry-degeneracy threshold across image scales.

Status: `extend_via_global_homography` already does **not** use it as
a gate (it uses pixel-unit reprojection residuals — see
`grow_extension.rs:196-200`). The struct is still re-exported at the
crate root, so external callers can still misuse the
`is_ill_conditioned` predicate.

**Fix options.**
- Narrow the doc-comment to "diagnostic only, not a stability gate."
- Or expose DLT design-matrix conditioning instead, with documented
  scale-aware semantics.

### Gap 4 — Hex pipeline has no `bfs_grow` counterpart (OPEN)

`hex/{alignment, mesh, rectify, smoothness}.rs` ship the static
geometry of a hex lattice. There is no `hex::grow::bfs_grow`. Hex
consumers cannot do seed-and-grow today. Tracked as deep-dive
Phase 7.

### Gap 5 — `estimate_local_steps` is implemented but unused (OPEN)

`local_step.rs` returns `(step_u, step_v, confidence,
supporters_u, supporters_v)` per corner. Nothing calls it from
production. Two open framings:

- Wire it as a *prediction-time refinement* in `bfs_grow` —
  finite-difference fallback when the global H isn't fit.
- Wire it as an *outlier signal in validation* — confidence weights
  the blacklist attribution.

Pick one; defer the other. Tracked as deep-dive Phase 5.

### Gap 6 — Booster duplicates BFS prediction logic (OPEN)

`crates/calib-targets-chessboard/src/boosters.rs` has its own
`predict_from_neighbors` and search loop. Any improvement to
`bfs_grow` prediction must be mirrored to the booster, or behaviour
diverges between "during grow" and "after grow."

**Fix.** Promote the booster's "interior gap fill + 1-step line
extension" into a generic `projective_grid::square::extension`
module on top of `extend_via_global_homography`'s machinery, then
delete the duplicate. Tracked as deep-dive Phase 2.

### Gap 7 — No subpixel re-fit pass (out of scope)

Once labels are committed, there is no joint sub-pixel refinement of
corner positions. The ROCHADE saddle-point fit [11] is the canonical
follow-up; OpenCV's `cornerSubPix` is the lighter version. Calling
either on the labelled set, with the current pixel positions as
starting points, would tighten the homography and the calibration
downstream. This is intentionally outside `projective-grid`'s scope —
the crate has no image data — but worth flagging as the natural next
layer.

### Gap 8 — Topological recall in heavy-distortion regions (OPEN)

In severe perspective + radial distortion, the topological pipeline
loses corners in the most foreshortened region. On
`testdata/puzzleboard_reference/example2.png` it labels 173/183
usable corners (versus seed-and-grow's 136), but 8 of the 10 missed
corners cluster in the heavily distorted bottom-left strip. The
`bench diagnose --algorithm topological` triangle-composition
counters (`triangles_mergeable / triangles_multi_diag /
triangles_has_spurious / triangles_all_grid`) localise the failure
to triangle pair-merging: in distorted regions Delaunay produces
triangles whose two long edges classify as both `Diagonal` (because
the cell's diagonal is no longer at 45° to its sides), making the
unique-diagonal merge step refuse the pair.

Loosening `axis_align_tol_rad` and `diagonal_angle_tol_rad` from 15°
to 30° barely helps (173 → 174 corners): it's not classification
noise, it's the strict 1-diagonal-2-grid pairing rule itself.

**Fix options, in order of decreasing scope.**
- *Permissive pairing.* When a triangle has ≥ 2 `Diagonal` edges,
  pick the longest one as the diagonal (longest edge of a
  cell-spanning triangle is its diagonal up to extreme
  foreshortening). Smallest code change; should recover most of the
  multi-diag triangles.
- *Spurious salvage.* Treat one `Spurious` edge in a triangle as
  `Grid` if its length matches the other Grid edge within a ratio.
  Recovers `triangles_has_spurious` boundary cases.
- *Hybrid extension.* After the topological pass, run
  `square::grow_extension::extend_via_local_homography` on
  unlabelled corners adjacent to the topological bbox. Combines
  topological's dense interior with seed-and-grow's reach into the
  distorted boundary.

### Gap 9 — Component merge handles only overlapping label sets (OPEN)

`projective_grid::component_merge::merge_components_local` currently
requires `min_overlap ≥ 1` shared label between two components. This
handles the majority case (gap-induced splits where a few edge
corners straddle both components), but disjoint patches separated by
a missing row never satisfy the overlap test and stay split.

**Fix.** Add a "predict next corner from each side" boundary check:
for each component, walk the labelled bbox boundary outward by one
cell using the local cell-step direction, and accept a merge when
the predicted boundary positions of one component land near actual
labelled positions of the other. Same scoring (cell-size + position
agreement) but applied to predicted-vs-labelled rather than
labelled-vs-labelled pairs.

### Gap 10 — Topological pipeline default vs `ChessboardV2` (OPEN)

`GraphBuildAlgorithm::default()` returns `ChessboardV2`. The
topological pipeline regresses recall on the public ChArUco-style
testdata (`testdata/small0..5.png`, `large.png`) because ChESS
detects corners *inside* marker bits whose axes lock to the
marker's local orientation, not the global grid. ChArUco detection
already pins to seed-and-grow regardless of caller choice, but the
public bench harness exercises `detect_chessboard` directly on
those images and the recall regression makes flipping the default
unsafe today.

**Fix.** Add a "drop corners with axes inconsistent with the global
mode" pre-filter that runs *before* Delaunay, removing the marker-
internal X-corners whose local axes don't agree with the histogram
peak found by `circular_stats`. Once that pre-filter ships, retest
the public bench and flip the default.

### Resolved gaps (April 2026 refactor)

- **Pipeline A removal** (was Gap 1, Gap 2, Gap 5, Gap 9). The
  slot-graph layer (`GridGraph::build`, `connected_components`,
  `assign_grid_coordinates`, `enforce_symmetry`,
  `prune_by_edge_straightness`, `prune_crossing_edges`,
  `prune_isolated_pairs`, `segments_properly_cross`) was removed —
  no production detector called it. The unification gap is closed by
  having only one pipeline.
- **Equal-weight prediction** (was Gap 3).
  `predict_from_neighbours` now uses 1/(Δi² + Δj²) weighting and
  per-neighbour finite-difference local-step estimation; both fixes
  shipped together. Test
  `predict_weights_diagonal_less_than_cardinal` covers it.
- **Reserved-but-unimplemented `projective_line_tol_rel`** (was
  Gap 6). The unused field was removed from `ValidationParams`.
- **Mode-blind multimodality** (was Gap 12).
  `GlobalStepEstimate::multimodal` is now populated; consumers can
  fall back to seed-derived cell size on bimodal clouds.
- **Dead `wrap_pi` import-keepalive** (was Gap 11). Removed.

### Resolved gaps (April 2026 topological pipeline)

- **Topological / Shu 2009 grid finder** (was the open
  "alternative-pipeline-based-on-Shu-2009" item). Shipped as
  `projective_grid::topological::build_grid_topological` with an
  axis-driven cell test (replacing the paper's image-color test) so
  the crate stays standalone. Selectable via
  `DetectorParams::graph_build_algorithm =
  GraphBuildAlgorithm::Topological`. Default is still
  `ChessboardV2` until Gap 10 closes; topological is opt-in for
  PuzzleBoard low-view-angle work where it already wins on
  recall + speed.
- **Shared component merge** (was the long-standing
  `enable_component_merge` flag with no implementation). Now lives
  in `projective_grid::component_merge::merge_components_local`,
  uses local-geometry-only acceptance (D4 + anchor pair + cell-size
  + position-residual gates, no global homography), and is callable
  from both pipelines via `DetectorParams::component_merge:
  LocalMergeParams`.

---

## 9. Summary

`projective-grid` solves the cloud-to-grid combinatorial step in a way
that is:

- **Pattern-agnostic** at the bottom (KD-tree, circular stats,
  mean-shift, DLT homography, Delaunay triangulation) and pattern-
  specific at the top via the `GrowValidator` trait (seed-and-grow)
  or the per-corner axes contract (topological). Each pattern crate
  (chessboard, ChArUco, PuzzleBoard) supplies its own validator
  implementing parity / axis cluster / marker-label rules; the
  generic `bfs_grow` consumes them.
- **Two-pipeline architecture.** A `GraphBuildAlgorithm` enum on
  `DetectorParams` selects either seed-and-grow (current default,
  battle-tested across all pattern types) or topological (opt-in,
  faster + denser on clean PuzzleBoards, image-free). Both produce
  identical `(i, j) → corner_idx` outputs and feed the shared
  validator + component-merge stages. ChArUco overrides to
  seed-and-grow internally because marker-internal corners poison
  the per-cell axis test.
- **Precision-by-construction** rather than precision-by-RANSAC: the
  seed-and-grow loop refuses ambiguous attachments, the topological
  pipeline rejects triangles with ambiguous diagonals or out-of-
  parallelogram quads, the validator attributes outliers
  deterministically, and Stage 6's residual gate refuses to
  extrapolate when the global-H assumption breaks (heavy radial
  distortion, dual-region perspective).
- **Distortion-tolerant** by design: every threshold is expressed in
  units of the locally-estimated cell size, the per-cell homography
  mesh absorbs curvature at *rectification* time, the local-H
  residual check is the leave-one-out version of as-projective-as-
  possible warping, and the topological walk uses pure topology so
  curved grid lines do not break the labelling.

The next architectural moves are closing Gap 8 (topological recall in
heavy-distortion regions) and Gap 10 (default-flip for the
topological pipeline), wiring `estimate_local_steps` into the
production pipeline (Phase 5), unifying the chessboard booster
with the generic extension machinery (Phase 2), and tightening the
homography-quality public surface (Gap 3). Hex-grid grow (Phase 7)
and `circular_stats` `Float`-genericisation (Phase 6) are smaller
incremental items.

---

## References

[1] S. Bennett, J. Lasenby. "ChESS — Quick and Robust Detection of
    Chess-board Features." *CVIU* 2014. The ChESS detector that
    produces the X-junction corners and axis estimates feeding this
    crate.

[2] K. V. Mardia, P. E. Jupp. *Directional Statistics.* Wiley, 2000.
    Chapter 9 covers axial-data circular means and the double-angle
    transformation.

[3] M. Stephens. "Tests for randomness of directions against two
    circular alternatives." *J. Amer. Statist. Assoc.* 64 (1969).
    Foundational paper on bimodal-direction testing.

[4] N. I. Fisher. *Statistical Analysis of Circular Data.* Cambridge,
    1993. Standard textbook on circular statistics.

[5] A. Geiger, F. Moosmann, Ö. Car, B. Schuster. "Automatic Camera
    and Range Sensor Calibration Using a Single Shot." *ICRA* 2012.
    The reference single-shot chessboard pipeline; introduces the
    grow-from-seed strategy this crate follows.

[6] Y. Cheng. "Mean Shift, Mode Seeking, and Clustering."
    *IEEE TPAMI* 17(8), 1995. Foundational mean-shift paper.

[7] D. Comaniciu, P. Meer. "Mean shift: a robust approach toward
    feature space analysis." *IEEE TPAMI* 24(5), 2002.

[8] L. Lucchese, S. K. Mitra. "Using Saddle Points for Subpixel
    Feature Detection in Camera Calibration Targets." *Asia-Pacific
    Conf. on Circuits and Systems*, 2002. The "co-linear triple" line
    test echoes through `square::validate`'s collinearity pass.

[10] J.-P. Place, P. Sturm, R. Horaud. "Camera Calibration from
     Reflective Spheres." *CVPR* 2005. Earlier predictive-grow style
     for non-chessboard targets.

[11] S. Placht, P. Fürsattel, E. Assoumou Mengue, H. Hofmann,
     C. Schaller, M. Balda, E. Angelopoulou. "ROCHADE: Robust Checker-
     board Advanced Detection for Camera Calibration." *ECCV* 2014.
     Saddle-point sub-pixel refinement; the natural follow-up layer
     to this crate's labelled grid output.

[12] J. Zaragoza, T. Chin, M. S. Brown, D. Suter. "As-Projective-As-
     Possible Image Stitching with Moving DLT." *IEEE TPAMI* 36(7),
     2014. Per-cell local homographies; what the
     `GridHomographyMesh` is conceptually doing.

[13] R. Hartley, A. Zisserman. *Multiple View Geometry in Computer
     Vision*, 2nd ed. Cambridge, 2003. Chapter 4 covers normalised
     DLT for homography estimation.

[14] C. Shu, A. Brunton, M. Fiala. "A topological approach to finding
     grids in calibration patterns." *Machine Vision and Applications*
     21(6), 2010. The Delaunay-+-color-test grid finder that
     `topological::build_grid_topological` re-implements with an
     axis-driven cell test.

[15] D. F. Watson. "Computing the n-dimensional Delaunay tessellation
     with application to Voronoi polytopes." *Computer J.* 24(2),
     1981. The Delaunay algorithm underlying the `delaunator` crate
     used in `topological::delaunay`.
