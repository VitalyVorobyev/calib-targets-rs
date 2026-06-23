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

### Gap 4 — Hex post-fit recovery schedule (OPEN, now precisely scoped)

Hex **topological** detection ships: `(Hex, Positions)` and `(Hex, Oriented3)`
run the axis-driven path (`topological/hex.rs` — triangle-as-cell classify +
axial `(q, r)` parallelogram-completion walk, D6 component merge, projective
fit). The hex topological path has **no post-fit recovery schedule** (boundary
extension / interior fill / rescue) — that machinery is ChESS-axis-coupled and
stays square-only — so hex recall is whatever the classify+walk recovers, with
the fit residual as the precision gate. Adding a geometry-only hex recovery
schedule is the remaining work. Tracked as a future deep-dive phase.

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

### Gap 13 — Legacy ChArUco vote alignment commits to the dominant rotation (RESOLVED — legacy matcher retired)

**Was:** `alignment::solve_alignment` (the legacy rotation+translation
**vote** matcher) picked a single D4 rotation up front via a score-weighted
`dominant_rotation` histogram, then solved the best integer translation for
that one rotation, never evaluating the other three rotations — so a frame
whose true board rotation differed from the score-dominant marker rotation
could get the wrong rotation and lose inliers.

**Resolution:** the legacy vote matcher and its `use_board_level_matcher`
toggle were retired; the board-level soft-LL matcher
(`detector/board_match.rs`) is now the sole matcher, and it already
enumerates all (D4 rotation × integer translation) hypotheses and picks the
maximum-likelihood one. The dominant-rotation shortcut no longer exists, so
this gap is closed by deletion rather than by patching the vote solver.

### Gap 18 — PuzzleBoard decode under heavy radial distortion (RESOLVED, PR #61, 2026-06-23)

**Was:** on the public `puzzleboard_reference/` author set (Stelldinger et al.
CC0 oracle frames), only **4 of 10** frames decoded (example0/4/5/6); the other
six failed at the master-match stage, and `example2.png` ("visible radial
distortion") returned *decoding failed: no position match above confidence
threshold*. The chessboard **grid** detected cleanly on all of them — only the
edge-dot → 501×501 master decode failed, because radial distortion shifts the
dot off the raw chord midpoint enough to corrupt the bit reads.

**Fix that landed (PR #61).** Sample each edge bit at distortion-aware candidate
centers instead of only the raw chord midpoint, keeping the highest-confidence
reading (the midpoint is always retained as a fallback):

- the cell's shared-edge midpoint via the local unit-cell→quad homography of
  each adjoining square (**perspective** correction), and
- a cubic-Lagrange interpolation of the edge midpoint along the curved grid line
  through the two flanking corners (`-1/16, 9/16, 9/16, -1/16` at t = ½ — a
  curvature-continuity estimate, **lens** correction).

This realizes the original "decode in a distortion-corrected grid frame"
candidate as a *local-geometry* equivalent: no explicit global radial model is
needed; the already-trusted grid supplies the per-cell correction.
`sweep_for_board` additionally appends a second pass using the legacy
hard-weighted scorer at the paper's 40% BER allowance to recover high-distortion
fragments; `PuzzleBoardParams::for_board` (the single-config default) is
unchanged.

**What stays / what was corrected (see Gap 19).** The distortion-aware edge
*sampling* is a genuine improvement and remains. The 40% BER sweep pass also
remains, but the claim that it "recovers" the distorted author examples did **not
survive a precision audit**: those decodes (example0/1/2/3/8) are *non-unique* —
a distinct master origin matches the distortion-corrupted edge bits as well as
(or within one or two bits of) the chosen origin. They passed a D4-consistency
check only because the tie-break happened to land on the reference origin. Under
the bounded-distance uniqueness gate added in Gap 19 they are correctly **declined
as detection failures** (a non-unique absolute decode is a wrong label waiting to
happen). The clean, full author boards (example4/5/6) decode with enormous
uniqueness margins and are unaffected. The grid still detects on the distorted
frames; only the *decode* declines.

### Gap 19 — PuzzleBoard decode lacked an origin-uniqueness gate (RESOLVED)

**Was.** Neither decode path enforced that the chosen master origin was
*uniquely* the best-supported one. The hard-weighted path (including the 40% BER
sweep pass) emitted `score_margin = ∞` and never computed a runner-up; the
soft-LL default path gated on a normalized log-likelihood *score gap*
(`alignment_min_margin`), which does not enforce origin uniqueness. A witness was
constructed and confirmed: a small or distortion-corrupted fragment frequently
has a *distinct* master origin (a different position and/or D4 transform) that
matches the observed edge bits as well as — or within a bit of — the true origin.
Shipping such a decode is a false positive (an unrecoverable wrong absolute
label), even though every per-frame `(i, j)` is internally D4-consistent.

**Root cause — this is error-correcting-code decoding.** The master edge code is
a De Bruijn torus whose minimum Hamming distance `d(w)` between a `w×w` window's
codeword and its nearest neighbour (over all origins and all 8 D4 transforms)
grows roughly quadratically with `w` but is only **1 at the 4×4 minimum window**
(zero error-correction). So a single corrupted bit can turn a corrupted 4×4
fragment into a *perfect* read of a different location. No acceptance test can
make the 4×4 window safe; the safe window must be sized to the code's distance
for the error budget (bounded-distance decoding).

**Fix.**

1. **Uniqueness gate (both paths).** Compute the matched-bit count of the closest
   *distinct* competing origin (full-master via an exact crossed-CRT top-2;
   fixed-board via the shift-scan top-2) and accept only when
   `margin > k_winner`, where `margin = best_matched − runner_up_matched` and
   `k_winner = edges_observed − best_matched` (the winner's own mismatch count).
   Equivalently, the winner's net score (`matched − mismatched`) must strictly
   exceed the runner-up's matched count. This is **parameter-free** (no magic
   constant): a clean exact read (`k_winner = 0`) passes at any `margin ≥ 1`,
   honoring the code's exact-uniqueness design at any size; a noisy-ambiguous read
   (small `margin`, large `k_winner`) declines. The soft path applies the
   identical matched-count predicate in addition to its score-gap gate (the soft
   default was *separately* vulnerable — it false-accepted a wrong origin at every
   window size in a high-trial sweep, a pre-existing defect in the production
   default, now closed).
2. **Bounded-distance floor.** `min_window` is raised to **7** (84 interior
   edges): a 300k-trial sweep (random origins × random error patterns up to the
   BER budget, per-corner ground-truth checked) finds 7×7 the smallest square
   window with zero false-accepts under the gate at both 30% and 40% BER (5×5 and
   6×6 still alias). A limiting-dimension guard additionally rejects wide-but-short
   strips that meet the edge-count floor yet are too thin on one axis to carry the
   code distance (a 3-corner-tall strip at the floor aliases at a low rate; every
   window spanning ≥ `min_window` corners on *both* axes is alias-free in the
   sweep).

**Honest guarantee.** Safety is **empirical-with-defense-in-depth, not a
worst-case guarantee**: at these BER budgets a worst-case bound would require
`d(w) > 2·⌊BER·N⌋ ≈ 0.8·N`, far above the ~`N/4` the code actually provides, so a
specifically-adversarial error pattern of weight ~`d/2` can still alias at any
practical window. The `min_window` floor keeps *random* corruption below the
aliasing regime, and the gate catches the residual near-aliases — validated to
zero false-accepts across the high-trial sweep. The criterion is structural
(scale-relative, dataset-independent); it is not fitted to any frame.

**Effect on the author set.** example4/5/6 (clean, full boards) decode with huge
uniqueness margins, unaffected. example0/1/2/3/8 (distorted or small) are
*non-unique* and are now correctly declined as detection failures — the
`author_examples_1_2_3_are_declined_as_non_unique` test pins this. The
single-config `for_board` default path inherits the raised floor and the soft
uniqueness gate (a deliberate, documented recall tradeoff on noisy/partial views:
boards below the safe window become correct misses rather than risk a wrong
label). The public regression and the private regression set hold at baseline
with zero wrong labels.

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
