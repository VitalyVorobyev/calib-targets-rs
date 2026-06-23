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

**Validation.** `example1/2/3` — previously among the six master-match failures
— now decode (253/180/28 labelled corners), joining example0/4/5/6 for **7 of
10**. The new `author_examples_1_2_3_decode_when_reference_dataset_present` test
asserts the decoded `(i, j)` labels are consistent with the master positions
under a single D4 transform (mod 501) — i.e. correct, not merely present. See
Gap 19 for the residual precision caveat this pass introduces.

### Gap 19 — 40% BER hard fallback has no uniqueness gate (OPEN)

The high-distortion sweep pass added in Gap 18's fix decodes with the
hard-weighted scorer at a 40% BER allowance and **no best-vs-runner-up margin
gate** (`score_margin = ∞` on the hard path), unlike the soft scorer's
`alignment_min_margin`. On a *full* board this is safe — wrong master origins
sit near 50% BER, comfortably above the gate — and the author-set decode labels
are D4-consistent. The residual risk is a *small fragment*: with few observed
edges, 40% BER is a small absolute bit budget, so a wrong origin could pass. And
because `detect_puzzleboard_best` selects on `(corners.len(), mean_confidence)`,
such a fragment could in principle out-rank a correct decode.

No false positive has been observed: the author D4-consistency test passes, and
the single-config `for_board` regression path is unaffected (the sweep / 40% pass
is reachable only through `detect_puzzleboard_best`). Per the evidence-driven
mandate, the next step is to **construct a witness** frame the 40% pass actually
mislabels before changing the gate. Candidate guards once a witness exists:
require a best-vs-runner-up BER margin on the hard path, or a minimum
observed-edge count for the 40% pass. Deferred pending a witness.

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
