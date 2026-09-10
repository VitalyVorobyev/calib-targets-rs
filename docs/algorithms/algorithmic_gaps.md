# Open algorithmic gaps — `projective-grid` + chessboard pipelines

> *Internal working notes. This file holds **only open gaps** — a gap that is
> closed is deleted, not archived. What the pipeline does belongs in the stage
> maps below; what changed belongs in the changelog.*

This file is the workspace-wide ledger of **open algorithmic gaps**
across `projective-grid` and `calib-targets-chessboard`. It is not a
pipeline reference — those live with the code that owns them:

- **`docs/algorithms/topological-grid-detection.md`** (repo root) — canonical
  stage map for the `projective_grid::topological` grid finder.
- **`crates/calib-targets-chessboard/docs/PIPELINE.md`** — canonical
  stage map for the chessboard topological pipeline,
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

The sole labelling pipeline is:

- **`Topological`** — Shu/Brunton/Fiala 2009 grid finder
  (`topological::detect_square_oriented2_all`) with an
  axis-driven cell test that replaces the paper's image-color sampling
  so `projective-grid` stays standalone. Used for all four target
  families. The `SeedAndGrow` variant was removed.

The topological facade runs the shared **component-merge** pass
(`projective_grid::shared::merge::merge_components_local`) — local
geometry only, no global homography (see Gap 9). The chessboard adapter
keeps a *distinct* post-booster corner-identity merge in its recovery
layer, not a second copy of this initial merge.

---

## Gaps and follow-ups

The pipeline ships zero wrong labels on the workspace's regression
datasets. Every gap below is **open**. A gap that gets closed is
deleted from this file — what the pipeline *does* belongs in the stage
maps above, and what changed belongs in the changelog.

### Gap 1 — Fully axis-free lattice finder

The `Evidence::Positions` path synthesizes per-corner axes up front
(`orient::synthesize_*`) and then feeds the topological builder, which covers
the common case. **Missing:** a fully axis-free topological path (edge-length
consistency only, no synthesized axes) for inputs where no reliable axis can be
synthesized at all.

### Gap 4 — Hex post-fit recovery schedule

Hex **topological** detection ships: `(Hex, Positions)` and `(Hex, Oriented3)`
run the axis-driven path (`topological/hex.rs` — triangle-as-cell classify +
axial `(q, r)` parallelogram-completion walk, D6 component merge, projective
fit). The hex topological path has **no post-fit recovery schedule** (boundary
extension / interior fill / rescue) — that machinery is ChESS-axis-coupled and
stays square-only — so hex recall is whatever the classify+walk recovers, with
the fit residual as the precision gate.

**Fix.** A geometry-only hex recovery schedule, mirroring the square one but
predicting from the axial lattice step rather than from ChESS axes.

### Gap 9 — Component merge handles only overlapping label sets

`projective_grid::shared::merge::merge_components_local` requires `min_overlap`
shared labels between two components (default `2`). This handles the majority
case — gap-induced splits where a few edge corners straddle both components —
but disjoint patches separated by a missing row never satisfy the overlap test
and stay split. `merge.rs` lists that case as explicit out-of-scope.

**Fix.** Add a "predict next corner from each side" boundary check:
for each component, walk the labelled bbox boundary outward by one
cell using the local cell-step direction, and accept a merge when
the predicted boundary positions of one component land near actual
labelled positions of the other. Same scoring (cell-size + position
agreement) but applied to predicted-vs-labelled rather than
labelled-vs-labelled pairs.

### Gap 11 — Off-axis false labels in blurred regions defeat the structural check

Measured on public `testdata/small3.png` (ChArUco, blurred bottom rows):
the production topological output labels `(10, 8)` at `(495.9, 312.4)`,
but column alignment against the adjacent sharp row (constant ≈ −2.4 px
column drift, verified on two neighbouring columns) pins the true
intersection at ≈ `(479.4, ·)` — the labelled corner is a marker-internal
false corner ~16.5 px off-axis. The topological wrong-label structural
check does not fire: the offending vertical edge has near-nominal length, its
orientation parity matches the rest of the component, and the off-axis
threshold is kept deliberately low because aggressive values create diagonals
on puzzle boards. A false corner whose true counterpart was never labelled
therefore still slips through.

**Fix.** A per-column/row drift-consistency check at the component level: the
measured signature — one corner breaking an otherwise constant column drift —
is strong and cheap. Marker-aware scoring is the alternative, at the cost of
coupling `projective-grid` to a target family.

---

### Gap 21 — Hex and low-orientation square lattices have no real-image evidence

`projective-grid`'s hex `Positions`, `Oriented1` and `Oriented3` modes, and the
square `Positions` / `Oriented1` modes, have deterministic synthetic coverage
and nothing else. They are exercised by construction, not by a camera, so they
remain **experimental**: nothing here should be read as a production-readiness
claim for them until a representative real-image campaign exists.

**Fix.** A real-image campaign per mode, blessed the way the chessboard
baselines are. Until then, `book/src/overview.md#gaps-and-early-stage-areas`
should keep saying so.

### Gap 22 — The `expert` namespace is not yet a compatibility boundary

`projective_grid::expert` is deliberately useful to detector builders, and it is
the seam a sibling crate reaches through. But it has never been reviewed as a
*stable* surface, so it is not a 1.0-quality compatibility boundary and the
crate should not be called 1.0 while that is true.

**Fix.** An API-surface pass over `expert` specifically: what is genuinely a
building block, what leaked out of a refactor, and what should be `#[doc(hidden)]`.

### Gap 23 — The global projective fit is a diagnostic approximation

Under strong lens distortion no single homography represents the board, so the
global projective fit is an approximation and its residual is a diagnostic
signal rather than a gate. The chessboard's final geometry check is deliberately
local for that reason and does not produce a second global-fit residual — so
there is no global number to compare across frames, by design.

**Fix.** None needed as a defect; recorded so the absence of a global residual
is not read as an oversight. See the architectural-direction note below on why
precision at the frontier stays local.

### Gap 24 — Straight-line validation on a curved target — **measured, not observed**

**The hypothesis.** `shared/validate/lines.rs` fits a total-least-squares
*straight* line to every grid row and column and flags members beyond
`line_tol_rel × scale`. On a cylinder the axial grid lines are generatrices and
project to straight lines, but the circumferential ones lie on circles and
project to **conics**. A sagitta estimate suggested the check would start
flagging once the pole's axis tilted more than a few degrees out of the image
plane, which would drop genuine corners — and `run_geometry_check` is mandatory
on the chessboard path, so there is no opting out.

**The measurement.** `calib-targets-puzzleboard`'s
`puzzlepole_synthetic::the_grid_builder_survives_a_cylinder` renders analytic
cylinders and runs the real grid builder over them, sweeping the two parameters
that strain a straight-line prior: axis tilt, and circumference (a row's sagitta
in *cell* units goes as the radius in cells, `p / 2π`, so `p = 48` bows twice as
hard per visible arc as `p = 24`).

At every geometry where corners are plentiful the grid builds in a **single
component** and labels essentially all of them — 77/77, 53/55, 30/31 at `p = 48`
across tilts of 0°, 15° and 30°, and at or above the detected count at
`p = 24`. The only row that yields nothing is `p = 48` at 45°, where **three**
corners are visible at all: a visibility limit, not a validator failure.

**So the gap does not fire, and the sagitta estimate was pessimistic.** The
likely reason is that the tolerance is relative to the *local* cell size, which
foreshortens along with the residual it is bounding, so the two move together.

**Left open rather than closed**, because the measurement covers what the crate
ships and not more. Re-check if any of these change: a visible sector wider than
about ±60° (the fairness line the test uses), a tighter `line_tol_rel`, or a
circumference beyond 48. The structural fix, if it is ever needed, is stated
above: a straight line is the degenerate conic, so the general predicate is
curvature continuity along the grid line, which subsumes the planar case rather
than special-casing the cylinder.

---

## Architectural-direction summary

The next architectural move is the **distortion-recall** line: recovering
legitimate-but-unreconstructed frontier corners in heavy radial distortion via a
*local* boundary-extension predicate unified with the generic extension
machinery. Precision at the frontier stays with *local* second-order criteria —
a global residual gate cannot separate a one-cell-past-edge false positive from
legitimately distorted corners, because a global homography does not represent
distortion. The disjoint-set component merge (Gap 9) is the other structural
item; the hex recovery schedule (Gap 4) is a smaller incremental one.

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
