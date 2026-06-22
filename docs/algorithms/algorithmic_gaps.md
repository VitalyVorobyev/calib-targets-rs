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

- **`docs/algorithms/topological-grid-detection.md`** (repo root) — canonical
  stage map for the `projective_grid::topological` grid finder.
- **`crates/calib-targets-chessboard/docs/PIPELINE.md`** — canonical
  stage map for the `GraphBuildAlgorithm::Topological` pipeline,
  including the chessboard-side topological input adapter and recovery
  layer.

Read those first for any pipeline question. The remainder of this
file lists what is known to be missing or suboptimal, with a
proposed fix per gap.

> Related upstream-defect note:
> [`docs/algorithms/diskfit-antipodal-sector.md`](diskfit-antipodal-sector.md)
> records why `RingFit` stays the default `OrientationMethod` (a latent
> axis-slot inversion in `chess-corners`' `DiskFit` fitter).

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

The sole labelling pipeline exposed via
`calib_targets_chessboard::GraphBuildAlgorithm` is:

- **`Topological`** — Shu/Brunton/Fiala 2009 grid finder
  (`topological::detect_square_oriented2_topological_all`) with an
  axis-driven cell test that replaces the paper's image-color sampling
  so `projective-grid` stays standalone. Used for all four target
  families. The `SeedAndGrow` variant was removed.

The topological facade runs the shared **component-merge** pass
(`projective_grid::shared::merge::merge_components_local`) — local
geometry only, no global homography (Phase 1.3 unified this; see Gap 9).
The chessboard adapter keeps a *distinct* post-booster corner-identity
merge in its recovery layer, not a second copy of this initial merge.

---

## Gaps and follow-ups

The pipeline ships zero wrong labels on the workspace's regression
datasets. The remaining structural gaps are tracked here. Resolved
items are kept in place as audit trail.

### Gap 1 — Generic axis-free lattice finder for non-chessboard consumers (PARTIAL)

The chessboard's parity-aware finder (`calib-targets-chessboard::seed`)
is built on top of the topological grid builder.

**Status (verified 2026-06-11, Phase 3).** The `Evidence::Positions`
path synthesizes per-corner axes up front (`orient::synthesize_*`) and
then feeds the topological builder. What remains: a fully axis-free
topological path (edge-length consistency only, no synthesized axes at
all) for inputs where no reliable axis can be synthesized. The
synthesized-axis path covers the common case; the pure-geometric fallback
is the remaining incremental item. Tracked as a future deep-dive phase.

### Gap 2 — `circular_stats` is `f32`-only (CLOSED — moot, 2026-06-22)

The detection surface is pinned to `f32` (`detect.rs`: "The detection surface
is pinned to `f32`"), so a `Float`-generic clustering histogram would be dead
generality — no consumer instantiates that path at `f64`. The two duplicate
undirected-angle helper sites (`shared/angle.rs` vs `cluster/circular.rs`) were
also consolidated to a single `f32` source of truth in `cluster::circular`, and
the chessboard `circular_stats.rs` re-export module was deleted. Re-open only if
a future `f64` detection surface appears; until then this is closed.

### Gap 3 — `HomographyQuality` is not a stable production metric (RESOLVED 2026-06-22)

`homography::HomographyQuality` returns SVD-derived ratios of the unnormalised
3×3 H matrix; the absolute-magnitude fields (`min_singular_value`,
`determinant`) depend on coordinate scale and translation magnitude, so they
are not a scale-stable geometry-degeneracy threshold across image scales.

Two things resolve the misuse risk:

- It is **not** re-exported at either crate root. `projective-grid` exposes it
  only under `projective_grid::geometry::`, and the production extension path
  gates on pixel-unit reprojection residuals (`extension.rs`), not on this
  struct. (The earlier note that it was "re-exported at the crate root" was
  stale — corrected here.)
- The rustdoc on both copies (projective-grid `geometry/homography.rs` and the
  `calib-targets-core` copy) now states **diagnostic only — not a scale-stable
  stability gate**, and `is_ill_conditioned` carries the same caveat.
  `is_ill_conditioned` has no production caller (test-only).

The deeper "expose DLT design-matrix conditioning with documented scale-aware
semantics" option is left as a future enhancement, not a blocker.

### Gap 4 — Hex post-fit recovery schedule (OPEN, now precisely scoped)

Hex **topological** detection ships: `(Hex, Positions)` and `(Hex, Oriented3)`
run the axis-driven path (`topological/hex.rs` — triangle-as-cell classify +
axial `(q, r)` parallelogram-completion walk, D6 component merge, projective
fit). The hex topological path has **no post-fit recovery schedule** (boundary
extension / interior fill / rescue) — that machinery is ChESS-axis-coupled and
stays square-only — so hex recall is whatever the classify+walk recovers, with
the fit residual as the precision gate. Adding a geometry-only hex recovery
schedule is the remaining work. Tracked as a future deep-dive phase.

### Gap 5 — `estimate_local_steps` wired into production (RESOLVED, verified 2026-06-11)

The old standalone `local_step.rs` / `estimate_local_steps` helper no
longer exists; the local-step *concept* it tracked is now realized in
production in **both** framings the gap proposed:

- **Prediction-time refinement.** `local_step_at` computes a
  per-neighbour finite-difference grid step used in prediction (with the
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

### Gap 6 — Booster boundary extension not in shared extension module (LARGELY RESOLVED)

The original duplication — `boosters.rs` carrying its own
`predict_from_neighbors` and search loop — has been removed. The
structural skeleton (cell enumeration, KD-tree, per-cell attachment
ladder, fixed-point iteration, and the adaptive per-cell prediction)
now lives in `projective_grid::topological` shared infrastructure.
`crates/calib-targets-chessboard/src/boosters.rs` is a policy wrapper:
it supplies a chessboard-specific `SquareAttachPolicy` (weak-cluster
rescue + optional directional edge scale) and delegates the prediction
and search to the shared fill machinery. Any improvement to the shared
prediction therefore reaches the booster path.

**Status (verified 2026-06-10, Phase 2d).** What remains is a
deliberate policy seam, not a duplicate. Residual follow-up: the booster
still owns the line-extrapolation pass (1-step boundary extension) as a
chessboard-side policy; folding that into a generic shared extension
entry point would let the booster path share that pass too. Left open as
a smaller incremental item.

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
- *Hybrid extension.* After the topological pass, run a local-homography
  extension step on unlabelled corners adjacent to the topological bbox,
  combining topological's dense interior with boundary reach into the
  distorted region.

### Gap 9 — Component merge handles only overlapping label sets (OPEN, verified 2026-06-11)

`projective_grid::shared::merge::merge_components_local` (moved here from
the old `component_merge` module) still requires
`min_overlap` shared labels between two components (default `2`). This
handles the majority case (gap-induced splits where a few edge corners
straddle both components), but disjoint patches separated by a missing
row never satisfy the overlap test and stay split — `merge.rs` still
lists that case as explicit out-of-scope.

**Verified unchanged by the Phase 1.3 merge-unification.** That work
made the topological facade call `merge_components_local` (via
`merge_walk_components`, with the chessboard adapter dropping its private
merge), but did **not** touch the overlap requirement. Disjoint-set merge
remains unimplemented.

**Fix.** Add a "predict next corner from each side" boundary check:
for each component, walk the labelled bbox boundary outward by one
cell using the local cell-step direction, and accept a merge when
the predicted boundary positions of one component land near actual
labelled positions of the other. Same scoring (cell-size + position
agreement) but applied to predicted-vs-labelled rather than
labelled-vs-labelled pairs.

### Gap 10 — Topological pipeline as sole algorithm (RESOLVED 2026-06-01, extended)

`GraphBuildAlgorithm` now has only `Topological`; the `SeedAndGrow` variant
was removed. Topological gives higher recall than seed-and-grow did on the
clean-chessboard regression set with precision held, and it is now the
algorithm for all four target families including ChArUco.

**Resolution — topological for all targets.** Plain `detect_chessboard` on
a marker image with default params is explicitly out of scope (use the
ChArUco detector); marker scenes go through the ChArUco detector, which
uses the topological builder with its own marker-aware decode path. The
`graph_build_dispatch::default_dispatch_matches_topological` test pins the
expected algorithm, and `marker_internal_rejection` / the private chessboard
precision-regression test cover the marker-scene precision contract.

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

**Residual precision caveat (follow-up):** enabling the recovery + chessboard
boosters for NeighbourEdges adds two attachment passes that run on the noisier
synthesized axes. On *sparse* grids (< `MIN_EDGE_SHAPE_LABELS` = 40 labelled
corners) the targeted topological wrong-label check (`Test 2.5`,
`topological_wrong_label_drops`) is gated off, so the only wrong-label net there
is the local-H `validate()` pass. For dense clutter-free boards (the gated test
set) this is fine, but a partially-occluded NeighbourEdges board could in
principle ship a mislabel the sparse-grid gate would have caught. NeighbourEdges
is experimental/opt-in and the default ChESS path is unaffected; before promoting
it, either lower the `Test 2.5` density gate for this path or add a sparse
NeighbourEdges precision fixture. Same weak-net family as Gap 15.

### Gap 12c — Orientation-free path is topological-only (CLOSED BY EVIDENCE 2026-06-17)

`OrientationSource::NeighbourEdges` is **topological-only**. Evidence from a
measured head-to-head (synthesized axes wired through `run_pipeline_lean`)
confirmed why a seed-and-grow approach failed: the seed finder stakes the
whole grid frame on ~4 seed corners' axes; synthesized axes (noisiest at
the boundary, where the seed quad picks) cause the seed to fail outright.
The topological builder labels connected components from many local edge
classifications, tolerating the noise. The `SeedAndGrow` variant no longer
exists, so this gap is definitively closed.

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

### Gap 14 — Topological→ChArUco determinism (RESOLVED 2026-06-21)

The topological grid is a *correct* ChArUco grid — decode precision lands at zero
self-consistency wrong-ids on every run. ChArUco now uses the topological builder
(the `SeedAndGrow` variant was removed).

Two `HashMap`-iteration-order tie-breaks in the **decode** path were root-caused
and fixed with deterministic tie-breaks (both shipped): `alignment::best_translation`
(translation vote — smaller `[i,j]` wins on a (weight_sum, count) tie) and
`merge::merge_charuco_results` (the multi-component group selector and
best-alignment pick). A residual flake from topological component ordering was
also resolved: `build_topological_detections` now uses a stable sort with a
positions-derived tie-break key so equal-count components have a deterministic
order across process seeds.

### Gap 15 — Topological boundary false-positive under strong barrel distortion (RESOLVED 2026-06-17)

On a heavily barrel-distorted physical board (the `GeminiChess1` regression
frame), the topological + ChESS-axes (production) path produced a **false-positive
labelled corner on the curved left edge** — a single frontier leaf labelled one
cell past the true board edge, i.e. a wrong `(i, j)` label the mandatory geometry
check failed to drop. This violated the hard no-mislabel invariant and was a
**blocker** (a miss is acceptable; a false positive is a contract violation).

**Root cause (second-order, measured).** The false corner's edge to the board
was **normal length and on-axis**, so it passed all three *first-order* criteria
of `topological_wrong_label_drops` (overlong-edge, off-axis-direction,
duplicate-pixel). The only signature was *second-order*: along its grid line the
cell spacing must vary smoothly and shrink toward the periphery, but this corner's
outermost edge was **larger** than the next edge inward — it reversed the smooth
spacing trend (normalised second difference ≈0.34 vs ≈0.07–0.13 on legitimate
interior). No edge-length ratio can separate it: the false edge was *shorter* than
many legitimate centre edges, which is exactly why an ad-hoc `continuation_length_
ratio_max`-style constant could not catch it (it would be simultaneously too loose
and too tight).

**Fix (general, not a tune).** Added a fourth, second-order criterion to
`projective_grid::shared::validate::recovery::topological_wrong_label_drops`:
**frontier line-spacing smoothness**. For every grid line whose outermost four
members are consecutive, the frontier edge is compared to the linear extrapolation
`2·e1 − e2` of the next two inner edges; a frontier member of cardinal degree ≤ 2
whose edge *overshoots* the extrapolation by more than `TOPO_FRONTIER_CURV_TOL`
(0.30, a dimensionless smoothness bound) is dropped — only it, not its neighbours.
The criterion is scale-free and distortion-model-agnostic (it assumes only that
spacing varies smoothly, true for radial *and* perspective), so the example is a
*consequence* of a sounder predicate, not a fitted target. Verified to flag
**exactly** the one false corner across all six public topo-grid frames (zero
flags on mid/large/GeminiChess2/3/gptchess1) and, by pixel-diff of the overlay,
to drop precisely the left-edge leaf at pixel ≈(210,163) and nothing else.

**Regression status:** topo-grid manifest gate corrected 53→52 / holes 3→4 (the
honest count after removing a false positive); 130x130_puzzle (topological),
ChArUco contract, orientation-free parity, and all public gates hold at baseline.
The criterion runs only on the topological builder (the sole builder). The
committed `baselines/chessboard.json` was re-blessed for this frame (it had
encoded the false positive plus a phantom top-left "miss"). **Residual (not
blockers, tracked):** a real bottom-left corner on this frame is detected but
not reconstructed (a recall miss, acceptable under the contract) — folds into
the Gap 8 distortion-recall family and the Gap 16 follow-up.

### Gap 16 — Global smooth-warp precision backstop (CLOSED BY EVIDENCE 2026-06-22 — approach falsified)

The proposed second phase to Gap 15 was a *global* precision backstop: model the
whole labelled lattice as a low-order smooth warp (biquadratic / TPS) and reject
corners whose reprojection residual is a high outlier, on the hypothesis that one
global model subsumes the distortion-recall (Gap 8), off-axis-false-label
(Gap 11), and frontier-false-positive (Gap 15) families under one predicate.

**The premise is falsified by measurement.** Fitting `(i, j) → pixel`
(biquadratic and affine, with leave-one-out leverage correction) over the
production labelled set on every public frame shows the global-residual gate is
*simultaneously too loose and too tight* — the exact smell CLAUDE.md names:

- On the Gap-15 witness `GeminiChess1.png`, the known false positive (the
  left-edge leaf one cell past the true board edge, pixel ≈ (210.7, 163.6)) has a
  leave-one-out residual of **0.096 cell (z = −0.96, 3rd-*smallest* of 53)** — a
  global low-order polynomial extrapolates the lattice through a one-cell-past-edge
  leaf almost exactly, giving the false corner a *tiny* residual. Legitimate
  barrel-distorted periphery corners meanwhile reach **0.5–0.58 cell
  (z = +4.5…+5.5)**: the false positive's residual is ~6× *smaller* than the
  legitimate ones. No threshold separates the classes because the separation does
  not exist in this feature.
- On `GeminiChess2/3` the biquadratic fits so tightly that legitimate corners
  reach z = 6.5–8.0, so any robust z-gate strict enough to be useful elsewhere
  manufactures false *drops* here (recall regression on the ratchets).

The Gap-15 signal is a **local second-order** spacing kink, which a global fit
averages away. So a global-residual drop gate is not precision-safe and cannot be
made safe by threshold choice; it was **not implemented** (zero source change).
The full per-corner numbers are recorded in the calibration-target-detector agent
memory (`gap16_smooth_warp_backstop.md`).

**What remains real (re-scoped).** Keep the Gap-15 *local* second-order criterion
— it is already the right shape. The genuinely open item is the *recall* half:
recovering the legitimate-but-unreconstructed bottom-left frontier corner on
`GeminiChess1` (the Gap-15 recall residual), a distinct problem a global-residual
*drop* gate never addressed. A *local* boundary-extension predicate (the Gap 6
family) is the right tool; folded back into the Gap 8 / Gap 6 distortion-recall
line.

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
  the crate stays standalone. Now the **sole algorithm** for all four
  target families (Gap 10 resolved 2026-06-01; `SeedAndGrow` variant
  subsequently removed).
- **Shared component merge** (was the long-standing
  `enable_component_merge` flag with no implementation). Now lives
  in `projective_grid::component_merge::merge_components_local`,
  uses local-geometry-only acceptance (D4 + anchor pair + cell-size
  + position-residual gates, no global homography). Invoked by the
  topological recovery layer. The `DetectorParams::component_merge:
  LocalMergeParams` field is consumed by the topological adapter.

---

## Architectural-direction summary

The next architectural move is the **distortion-recall** line: recovering the
legitimate-but-unreconstructed frontier corners in heavy radial distortion
(Gap 8) via a *local* boundary-extension predicate unified with the generic
extension machinery (Gap 6). The global smooth-warp backstop once proposed as
Gap 16 was **falsified by measurement** (a global residual gate cannot separate a
one-cell-past-edge false positive from legitimate distorted corners) and is
closed; the Gap-15 *local* second-order criterion stays the precision tool there.
The disjoint-set component merge (Gap 9) also remains. Gap 3 (homography-quality
surface) and Gap 2 (`circular_stats`) are closed; the hex-grid recovery schedule
(Gap 4) is a smaller incremental item.

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
