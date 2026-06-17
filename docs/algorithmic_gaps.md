# Open algorithmic gaps — `projective-grid` + chessboard pipelines

> *Internal working notes. Labels like "Phase D" or "Q2 of the
> deep-dive roadmap" are internal milestones from prior debugging
> campaigns and must not appear in source rustdoc, READMEs, or any
> other public surface. When referring to algorithm stages from
> code, use the descriptive names defined in the per-crate stage
> maps.*

This file is the workspace-wide ledger of **open algorithmic gaps**
across `projective-grid` and `calib-targets-chessboard`. It is not a
pipeline reference — those live with the code that owns them:

- **`docs/topological-grid-detection.md`** (repo root) — canonical
  stage map for the `projective_grid::topological` grid finder.
- **`crates/calib-targets-chessboard/docs/PIPELINE.md`** — canonical
  stage maps for both `GraphBuildAlgorithm` variants (SeedAndGrow
  default + Topological opt-in), including the chessboard-side
  topological input adapter and recovery layer.

Read those first for any pipeline question. The remainder of this
file lists what is known to be missing or suboptimal, with a
proposed fix per gap.

---

## Problem context (one paragraph for new readers)

Detectors in this workspace start with a feature detector — typically
the **ChESS** X-junction detector of Bennett & Lasenby [1] — that
emits a *cloud* of corner candidates with sub-pixel position and two
undirected grid axes per corner. `projective-grid` answers a single
question:

> Given a cloud of 2D corner candidates with two-axis orientation,
> return the integer `(i, j)` label of every candidate that lies on a
> regular projective grid, with no false labels.

"No false labels" is the **precision-by-construction** contract: a
wrong `(i, j)` poisons calibration; a missing `(i, j)` does not.
Every algorithm in the crate is biased toward dropping rather than
mislabelling.

Two labelling pipelines are exposed via
`calib_targets_chessboard::GraphBuildAlgorithm`:

- **`SeedAndGrow`** — seed-and-grow with homography boundary extension
  (`seed_and_grow::grow::bfs_grow` +
  `seed_and_grow::extension::extend_via_local_homography`),
  battle-tested across all four target families.
- **`Topological`** — Shu/Brunton/Fiala 2009 grid finder
  (`topological::detect_square_oriented2_topological_all`) with an
  axis-driven cell test that replaces the paper's image-color sampling
  so `projective-grid` stays standalone.

Both pipelines now run the shared **component-merge** pass
(`projective_grid::shared::merge::merge_components_local`) inside their
facade — local geometry only, no global homography (Phase 1.3 unified
this; see Gap 9). The seed-and-grow facade merges its per-seed
components; the topological facade merges its per-walk components. The
chessboard adapter keeps a *distinct* post-booster corner-identity merge
in its recovery layer, not a second copy of this initial merge.

---

## Gaps and follow-ups

The pipeline ships zero wrong labels on the workspace's regression
datasets. The remaining structural gaps are tracked here. Resolved
items are kept in place as audit trail.

### Gap 1 — Generic seed finder for non-chessboard consumers (PARTIAL)

`projective_grid::seed_and_grow::seed::finder` ships `find_quad`,
`SeedQuadParams`, and the `SquareSeedPolicy` seam — the primitives a
generic finder needs. The chessboard's parity-aware finder
(`calib-targets-chessboard::seed`) is built on top.

**Progress (verified 2026-06-11, Phase 3).** A default, parity-free
seed+attach policy for unoriented point clouds now ships:
`projective_grid::seed_and_grow::positions_policy::PositionsAttachPolicy`
implements both `SquareSeedPolicy` and `SquareAttachPolicy` with
eligibility = all, no axis-cluster prior, and no parity hooks. It is the
shipped `Evidence::Positions` path. **What it still consumes:** the
*synthesized* per-corner axes (the facade runs `orient::synthesize_*` up
front), so the seed chord-pairing reads `policy.axes(idx)` rather than a
purely geometric "nearest + most-orthogonal chord" construction.

What remains: a fully axis-free seed-finder (edge-length consistency +
midpoint violation only, no synthesized axes at all) for inputs where no
reliable axis can be synthesized. The synthesized-axis path covers the
common case; the pure-geometric fallback is the remaining incremental
item. Tracked as a future deep-dive phase.

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
`extension.rs`). The struct is still re-exported at the crate root,
so external callers can still misuse the `is_ill_conditioned`
predicate.

**Fix options.**
- Narrow the doc-comment to "diagnostic only, not a stability gate."
- Or expose DLT design-matrix conditioning instead, with documented
  scale-aware semantics.

### Gap 4 — Hex seed-and-grow has no implementation (OPEN, now precisely scoped)

Hex **topological** detection ships: `(Hex, Positions)` and `(Hex, Oriented3)`
run the axis-driven path (`topological/hex.rs` — triangle-as-cell classify +
axial `(q, r)` parallelogram-completion walk, D6 component merge, projective
fit). What remains open is **hex seed-and-grow**: there is no
`seed_and_grow` counterpart for hex (no hex seed-cell shape, no hex
neighbour-prediction grow, no hex recovery schedule), so `(Hex, *)` under
`SquareAlgorithm::SeedAndGrow` is a typed `UnsupportedCombination`. The hex
topological path also has **no post-fit recovery schedule** (boundary
extension / interior fill / rescue) — that machinery is seed-and-grow- and
ChESS-axis-coupled and stays square-only — so hex recall is whatever the
classify+walk recovers, with the fit residual as the precision gate. Adding hex
seed-and-grow (and a geometry-only hex recovery schedule) is the remaining work.
Tracked as a future deep-dive phase.

### Gap 5 — `estimate_local_steps` wired into production (RESOLVED, verified 2026-06-11)

The old standalone `local_step.rs` / `estimate_local_steps` helper no
longer exists; the local-step *concept* it tracked is now realized in
production in **both** framings the gap proposed:

- **Prediction-time refinement.** `seed_and_grow::grow::predict`'s
  `local_step_at` computes a per-neighbour finite-difference grid step
  and `predict_from_neighbours` uses it inside `bfs_grow` (with the
  global `(u, v, cell_size)` as the fallback) — this is the
  foreshortening-aware prediction that closed the old "BFS overshoots on
  the far edge" recall stall.
- **Validation outlier signal.** `shared::validate::step::local_step_per_corner`
  computes a per-corner scalar local step that
  `find_inconsistent_corners_step_aware` uses to gate edge-length
  outliers against the *local* pitch rather than a global cell size.

Both are exercised by tests (`predict_uses_local_step_when_neighbour_has_own_neighbours`,
`local_step_per_corner_central_diff`). No standalone confidence-scored
helper remains to wire; the gap is closed.

### Gap 6 — Booster duplicates BFS prediction logic (LARGELY RESOLVED)

The original duplication — `boosters.rs` carrying its own
`predict_from_neighbors` and search loop — has been removed. The
structural skeleton (cell enumeration, KD-tree, per-cell attachment
ladder, fixed-point iteration, and the adaptive per-cell prediction)
now lives in `projective_grid::seed_and_grow::fill::fill_grid_holes`.
`crates/calib-targets-chessboard/src/boosters.rs` is a policy wrapper:
it supplies a chessboard-specific `SquareAttachPolicy` (weak-cluster
rescue + optional directional edge scale) and delegates the prediction
and search to `fill_grid_holes`. Any improvement to the shared
prediction therefore reaches both the grow and booster paths.

**Status (verified 2026-06-10, Phase 2d).** The Phase-2d merge-unify /
dedup work did **not** touch this — the prediction skeleton was already
shared via `fill_grid_holes` before Phase 2d. What remains is a
deliberate policy seam, not a duplicate. Residual follow-up: the booster
still owns the line-extrapolation pass (1-step boundary extension) as a
chessboard-side policy; folding that into a generic
`projective_grid::seed_and_grow::extension` entry point would let the
two paths share that pass too. Left open as a smaller incremental item.

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
loses corners in the most foreshortened region. The `bench diagnose
--algorithm topological` triangle-composition counters
(`triangles_mergeable / triangles_multi_diag /
triangles_has_spurious / triangles_all_grid`) localise the failure
to triangle pair-merging. The current classifier removes the old
fixed-45° diagonal failure by inferring diagonals from triangles with
two local grid sides. Remaining misses are expected to come from
`all_grid` triangles, real occlusions, or component gaps rather than
the legacy 45° diagonal gate.

**Follow-up options, in order of decreasing scope.**
- *Parity-assisted classification.* Use checkerboard parity when the
  caller has it to distinguish true diagonals from same-axis skips.
- *Hybrid extension.* After the topological pass, run
  `seed_and_grow::extension::extend_via_local_homography` on
  unlabelled corners adjacent to the topological bbox. Combines
  topological's dense interior with seed-and-grow's reach into the
  distorted boundary.

### Gap 9 — Component merge handles only overlapping label sets (OPEN, verified 2026-06-11)

`projective_grid::shared::merge::merge_components_local` (moved here from
the old `component_merge` module) still requires
`min_overlap` shared labels between two components (default `2`). This
handles the majority case (gap-induced splits where a few edge corners
straddle both components), but disjoint patches separated by a missing
row never satisfy the overlap test and stay split — `merge.rs` still
lists that case as explicit out-of-scope.

**Verified unchanged by the Phase 1.3 merge-unification.** That work made
*both* algorithm facades call `merge_components_local` (the topological
facade now merges in-crate via `merge_walk_components`, and the
chessboard adapter dropped its private merge), so the two facades expose
identical multi-component semantics — but it did **not** touch the
overlap requirement. Disjoint-set merge remains unimplemented.

**Fix.** Add a "predict next corner from each side" boundary check:
for each component, walk the labelled bbox boundary outward by one
cell using the local cell-step direction, and accept a merge when
the predicted boundary positions of one component land near actual
labelled positions of the other. Same scoring (cell-size + position
agreement) but applied to predicted-vs-labelled rather than
labelled-vs-labelled pairs.

### Gap 10 — Topological pipeline default vs `SeedAndGrow` (RESOLVED 2026-06-01)

`GraphBuildAlgorithm::default()` now returns `Topological`. The
topological builder gives higher recall than seed-and-grow on the
clean-chessboard regression set with precision held, so it is the
default for plain chessboard / marker / puzzle detection.

**Resolution — flip + scope ChArUco out, rather than the pre-Delaunay
filter originally sketched here.** Topological is *not* precision-safe on
ChArUco-style images: ChESS fires corners *inside* marker bits whose axes
lock to the marker, and the per-cell axis test can admit them as
chessboard corners. The decision (owner, 2026-06-01) is that marker scenes
go through the ChArUco detector — which already pins seed-and-grow
unconditionally — so the topological builder is **never gated against
ChArUco**. Plain `detect_chessboard` on a marker image with default params
is therefore explicitly out of scope (use the ChArUco detector). The
flip's precision/recall gate was the non-marker regression set
(clean-chessboard + puzzle), verified before flipping; the
`graph_build_dispatch::default_dispatch_matches_topological` test pins the
new default, and `marker_internal_rejection` / the private chessboard
precision-regression test pin seed-and-grow explicitly as the marker-scene
guarantee.

### Gap 11 — Off-axis false labels in blurred regions defeat the structural check (OPEN)

Measured on public `testdata/small3.png` (ChArUco, blurred bottom rows):
the production topological output labels `(10, 8)` at `(495.9, 312.4)`,
but column alignment against the adjacent sharp row (constant ≈ −2.4 px
column drift, verified on two neighbouring columns) pins the true
intersection at ≈ `(479.4, ·)` — the labelled corner is a marker-internal
false corner ~16.5 px off-axis. The topological wrong-label structural
check (overlong / off-axis / duplicate-pixel) does not fire: the offending
vertical edge has near-nominal length and the off-axis threshold is kept
deliberately low because aggressive values create diagonals on puzzle
boards (see the wrong-label check notes). The same false-corner family
caused the duplicate-coord ambiguity fixed in the walk (labels colliding
one cell apart); collisions are now dropped, but a false corner whose true
counterpart was never labelled still slips through.

Candidate directions: per-column/row drift-consistency check at the
component level (the measured signature — one corner breaking an otherwise
constant column drift — is strong and cheap), or marker-aware scoring once
ChArUco-adjacent recall work resumes. Tied to the Phase 3 orientation-free
policy work, which needs the same local-geometry discrimination.

### Gap 12 — Orientation-free synthesis on clutter-dense targets (CLOSED BY EVIDENCE 2026-06-11)

Position-only axis synthesis (`projective_grid::orient`) cannot reach recall
parity with ChESS axes on ChArUco-style boards. Measured mechanism: glyph
corners sit at roughly a third of the lattice pitch, so the per-corner
nearest-neighbour pool mixes clutter and lattice chords (1-NN p05 ≈ pitch/3
on the failing boards; 4-NN spread ≈ 2× vs ≈ 1.04 on clean boards), the
global double-angle modes lock onto the clutter geometry, and the best
position-only variant evaluated (generous pool + greedy max-angular-spread
chord selection + local 2-means) still leaves a ~40° gap between synthesized
axes and true lattice edge directions in the in-focus sliver — ~0 % of true
edges within the topological classifier's 15° tolerance, vs 94 % on a clean
control board. A spread-floor variant that improves clutter coherence also
rejects genuine near-collinear axis pairs under strong foreshortening
(regresses the perspective fixture), confirming the tension is structural.

**Resolution:** clutter-dense targets are out of scope for orientation-free
detection; they require intensity-aware axes (the ChESS fit), which the
production default and the ChArUco detector already use. The supported
domain — clutter-free regular grids — is at parity ≥ 1.0 per image and gated
by `calib-targets-bench/tests/orientation_free_parity.rs`. Any future
re-opening should start from intensity-aware seeding, not better
position-only chord statistics.

### Gap 12b — Orientation-free *pipeline* parity on the topological path (RESOLVED 2026-06-17)

Gap 12 closed the *grid-builder* layer; the full chessboard *pipeline* still
under-recovered with `OrientationSource::NeighbourEdges` (e.g. `testdata/mid.png`
stalled at the interior block, ≈ 0.82× recall, missing the boundary row).

**Mechanism:** neighbour-edge axes are synthesized once at the chessboard input
stage (`topological/inputs.rs::corners_with_synthesized_axes`, with a finite
~2° sigma — the π no-info sentinel would make the clusterer skip every axis),
then both orientation sources feed `Evidence::Oriented2`. The recovery that
lifts the noisier synthesized-axis walk to full recall is **projective-grid's
own geometry-only synthesized-axis recovery** (`RecoverySchedule::On`), NOT the
chessboard's ChESS-axis boosters — those gate every attach on `axes_match_centers`
and reject synthesized boundary axes (got only 61→63 on mid.png). The
neighbour-edge recovery keeps local validation on but disables only the global
post-fit homography residual drop (which kills warped grids like the GeminiChess
set); the local-H revalidation rejects the over-extension a fully-disabled
validate would attach one cell past the board edge. The ChESS path is unchanged
(byte-identical). Gated by the pipeline arm of `orientation_free_parity.rs`.

### Gap 12c — Orientation-free seed-and-grow is non-viable (CLOSED BY EVIDENCE 2026-06-17)

`OrientationSource::NeighbourEdges` is intentionally **topological-only** —
`validate()` rejects it with `SeedAndGrow` as a typed error. A measured
head-to-head (synthesized axes wired through `run_pipeline_lean`) confirmed why:
seed-and-grow returned **0 corners on 3 of 6 clutter-free frames** and collapsed
to 19 vs 373 on a dense board, while being slower. The seed finder stakes the
whole grid frame on ~4 seed corners' axes; synthesized axes (noisiest where the
seed quad picks, at the boundary) make the seed fail outright. The topological
builder labels connected components from many local edge classifications, so it
tolerates the noise. Re-opening would require an axis-robust seed selector
(cell-size-consistent or trial-grow-scored seeding), not just wiring.

### Gap 13 — Legacy ChArUco vote alignment commits to the dominant rotation (OPEN, low priority)

`alignment::solve_alignment` (the legacy rotation+translation **vote**
matcher, used only when `CharucoParams::use_board_level_matcher` is `false`)
picks a single D4 rotation up front via a score-weighted
`dominant_rotation` histogram, then solves the best integer translation for
that one rotation. It never evaluates the other three D4 rotations, so a
frame whose true board rotation differs from the score-dominant marker
rotation (e.g. a few high-score noise decodings biasing the histogram) can
get the wrong rotation and lose inliers. The vestigial single-element
`candidate` tuple in `solve_alignment` is the remnant of an earlier
multi-candidate selector.

The stale `// TODO: just run solve_alignment on the full set of markers` at
the former `select_and_refine_markers` call site was misleading: the full
set *is* already passed to `solve_alignment` in one call — the real gap is
the missing per-rotation enumeration, not a per-marker loop. The TODO has
been removed in favour of this entry.

**Why low priority:** the production default is the board-level soft-LL
matcher (`use_board_level_matcher = true`), which already enumerates all
(D4 rotation × integer translation) hypotheses and picks the
maximum-likelihood one — so the dominant-rotation shortcut only affects the
opt-in legacy fallback. A proper fix (enumerate all four rotations in
`solve_alignment`, keep the max-inlier candidate) is small and contained but
must be gated on the private ChArUco regression sweep before landing; it is
deferred until that path needs attention.

### Gap 14 — Topological→ChArUco determinism (PARTIAL, 2026-06-17)

The topological grid is a *correct* ChArUco grid — decode precision is tied with
seed-and-grow (zero self-consistency wrong-ids on every run), refuting the old
"topological poisons charuco decode" premise. The blocker to making it the
ChArUco default is **determinism**: across fresh process seeds the full 120-frame
flagship sweep tips one borderline frame (119 vs 120 detected).

Two `HashMap`-iteration-order tie-breaks in the **decode** path were root-caused
and fixed with deterministic tie-breaks (both shipped): `alignment::best_translation`
(translation vote — smaller `[i,j]` wins on a (weight_sum, count) tie) and
`merge::merge_charuco_results` (the multi-component group selector and
best-alignment pick). These reduce but do **not** eliminate the flake: a residual
seed-dependent source remains, almost certainly **upstream in the chessboard
topological component ordering** — `build_topological_detections` sorts components
by `Reverse(corners.len())` with a *stable* sort, so equal-count components keep
their recovery order, which can be `HashMap`-derived; ChArUco's multi-component
sweep (`detect_all` + consumed-tracking) is sensitive to that order. (Note:
[[project_topo_grid_test_flaky]]'s 2026-05-29 fix hardened the chessboard *bench*
path, not necessarily every order that the ChArUco multi-component route exercises.)

**Resolution:** ChArUco stays pinned to seed-and-grow
(`CharucoParams::for_board`), with `allow_topological_grid` the measurement-only
opt-in. Re-opening the flip requires (1) a deterministic total order on the
topological component list (tie-break the count-sort on a positions-derived key
such as the bbox-min corner index), verified by a multi-seed sweep landing a hard
120/120, and (2) a `min_corner_strength` floor sweep for topological ChArUco to
close the ~10% per-frame corner-count gap. Precision is not a blocker.

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
  GraphBuildAlgorithm::Topological`. Now the **default** (Gap 10
  resolved 2026-06-01); `SeedAndGrow` is pinned for ChArUco and
  available per call elsewhere.
- **Shared component merge** (was the long-standing
  `enable_component_merge` flag with no implementation). Now lives
  in `projective_grid::component_merge::merge_components_local`,
  uses local-geometry-only acceptance (D4 + anchor pair + cell-size
  + position-residual gates, no global homography). Currently
  invoked only by the topological recovery layer; SeedAndGrow
  keeps the labelled set as a single connected component by
  construction. The `DetectorParams::component_merge:
  LocalMergeParams` field is consumed by the topological adapter
  only.

---

## Architectural-direction summary

The next architectural moves are closing Gap 8 (topological recall in
heavy-distortion regions), wiring `estimate_local_steps` into the
production pipeline (Gap 5), unifying the chessboard booster with the
generic extension machinery (Gap 6), and tightening the homography-
quality public surface (Gap 3). Hex-grid grow (Gap 4) and
`circular_stats` `Float`-genericisation (Gap 2) are smaller
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
