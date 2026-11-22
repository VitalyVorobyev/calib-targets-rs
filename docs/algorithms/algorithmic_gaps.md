# Open algorithmic gaps — `projective-grid` + chessboard pipelines

> *Internal working notes. This file holds **only open gaps** — a gap that is
> closed is deleted, not archived. What the pipeline does belongs in the stage
> maps below; what changed belongs in the changelog.*

This file is the workspace-wide ledger of **open algorithmic gaps**
across `projective-grid` and `calib-targets-chessboard`. It is not a
pipeline reference — those live with the code that owns them:

- **`docs/algorithms/topological-grid-detection.md`** — canonical
  stage map for the `projective_grid::topological` grid finder.
- **`crates/calib-targets-chessboard/docs/PIPELINE.md`** — canonical
  stage map for the chessboard topological pipeline,
  including the chessboard-side topological input adapter and recovery
  layer.

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

The sole grid builder is **`Topological`** — the Shu/Brunton/Fiala grid
finder [2] with an axis-driven cell test in place of the paper's image-colour
sampling, so `projective-grid` stays standalone. Every target family uses it
(chessboard, ChArUco, marker board, PuzzleBoard, PuzzlePole); a hex variant
shares the front half. The stage maps above own the description of the
stages, the component merge and the chessboard recovery layer.

---

## Gaps and follow-ups

Every gap below is **open**. The two known wrong-label witnesses on public
images are Gap 11 and Gap 27.

### Gap 1 — Fully axis-free lattice finder

The `Evidence::Positions` path synthesizes per-corner axes up front
(`orient::synthesize_*`) and then feeds the topological builder, which covers
the common case. **Missing:** a fully axis-free topological path (edge-length
consistency only, no synthesized axes) for inputs where no reliable axis can be
synthesized at all.

### Gap 4 — Hex post-fit recovery schedule

Hex **topological** detection ships: `(Hex, Positions)`, `(Hex, Oriented1)`
and `(Hex, Oriented3)` run the axis-driven path (`topological/hex.rs` — triangle-as-cell classify +
axial `(q, r)` parallelogram-completion walk, D6 component merge, projective
fit). The hex topological path has **no post-fit recovery schedule** (boundary
extension / interior fill / rescue) — that machinery is ChESS-axis-coupled and
stays square-only — so hex recall is whatever the classify+walk recovers, with
the fit residual as the precision gate.

**Fix.** A geometry-only hex recovery schedule, mirroring the square one but
predicting from the axial lattice step rather than from ChESS axes.

### Gap 9 — Component merge handles only overlapping label sets

`merge_components_local` (`projective-grid` `shared/merge.rs`, public as
`expert::component::merge_components_local`) requires `min_overlap`
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
baselines are. Until then, `book/src/crates.md#maturity`
should keep saying so.

### Gap 22 — The `expert` namespace is not yet a compatibility boundary

`projective_grid::expert` is deliberately useful to detector builders, and it is
the seam a sibling crate reaches through. But it has never been reviewed as a
*stable* surface, so it is not a 1.0-quality compatibility boundary and the
crate should not be called 1.0 while that is true.

**Fix.** An API-surface pass over `expert` specifically: what is genuinely a
building block, what leaked out of a refactor, and what should be `#[doc(hidden)]`.

### Gap 23 — No global residual; the local factor has known biases

Under strong lens distortion no single homography represents the board, so
there is no global-fit residual, by design. The frame-comparable number is
`DetectionQuality::{local_residual_rms_px, variance_factor}`
(`calib-targets-core/src/quality.rs`): local, leave-one-out, report-only.
Its limits, all measured by `bench accuracy`:

- The local predictor bends with the grid only to first order: at k₁ = 0.15
  (synthetic, 16 px cells) the residual has a ≈ 0.035 px RMS floor from the
  lines' curvature (local RMS 0.041 px against a true 0.019 px with
  SaddlePoint), so for very precise corners the local factor reads high
  (0.28 vs a true 0.07).
- A localizer bias that is smooth across the board is partly absorbed by the
  neighbours, so the local factor reads low (CenterOfMass on the synthetic
  chessboards: median 0.16 vs a true 0.27).
- Non-planar targets (PuzzlePole) skip the local check; a cylinder-aware
  predictor would close that.

### Gap 24 — Straight-line validation on a curved target — **does not fire in the shipped range**

`shared/validate/lines.rs` fits a *straight* line to every grid row and
column; on a cylinder the circumferential rows project to conics.
`puzzlepole_synthetic::the_grid_builder_survives_a_cylinder` measures it: at
every geometry where corners are plentiful the grid builds in one component
and labels essentially all of them (the tolerance is relative to the local
cell size, which foreshortens with the residual it bounds).

Re-check if any of these change: a visible sector wider than about ±60°, a
tighter `line_tol_rel`, or a circumference beyond 48. The structural fix, if
needed: a straight line is the degenerate conic, so the general predicate is
curvature continuity along the grid line.

### Gap 25 — Telling one PuzzlePole from another — **open, unmeasured**

**The claim.** Poles cut from *disjoint* axial column windows of the master
share no corner at all, so a decoded corner identifies its pole. That is true by
construction — it is arithmetic on the window offsets, and
`PuzzlePoleSpec::distinct_poles` enumerates the family — and it is the basis of
the paper's "71 distinct poles".

**What is not established.** The claim is about *ids not colliding*. It is not a
statement about the detector, and the detector cannot currently make it: a pole
decodes through `BoardRect::pole`, whose column axis is `Clamped` to the
configured pole's own window. Origins outside that window are never enumerated,
so a fragment of a *different* pole is never given the chance to land where it
belongs and be seen not to belong here. What happens instead — refusal on bit
error, refusal on the uniqueness margin, or a wrong id — has not been measured,
for either axis of difference: a different axial window on the same code stripe,
or a different start row at the same circumference.

The gates *should* refuse: a foreign axial window reads different `map_a`
columns, so the bit-error rate should be high and the margin gate should decline
it. But "should" is not a measurement, and the contract this workspace gives is
asymmetric — a miss is acceptable, a wrong id is not.

**What would close it.** Sweep ordered pairs over the shipped family: render
pole B, decode with pole A's params, require refusal at every placement and
azimuth. Same shape as the window-floor sweep, and it belongs in
`research/puzzleboard-rings` beside it.

**Why it is open rather than blocking.** One pole in frame is the shipped use,
and a multi-pole scene is an explicit per-spec loop in the caller's code rather
than something reached by accident. The tutorial states the limit where a user
would meet it.

### Gap 27 — Boundary recovery labels the board's outer-edge T-junctions — **open, measured**

On the public `testdata/markerboard.png` (22 × 22 inner corners, seen whole)
the chessboard detector delivers 485 corners: all 484 inner corners plus a
T-junction on the board's top edge at `(724.4, 63.1)`, where a black and a white
outer square meet the margin. It is not an X-junction. Trace
(`trace_topological_detection`): ChESS strength 44.8, axis σ 0.23 / 0.22 rad
(inside the 15° admission), touches no Delaunay quad, and is attached by the
**axis-aware recovery booster** (`axis_aware_recovery_additions`, not geometry-only) at
the lattice row one past the board. It lies 2.6 px (0.06 of the 43 px step)
off the quadratic extrapolation of its column — on the lattice, but not an
inner corner, and localised by a T-junction response.

Effect: a plain chessboard detection carries one extra row-end point; an
absolute-label detector maps it outside the board. The marker board drops
every corner outside its `rows` × `cols` extent; ChArUco and PuzzleBoard have
not been checked for the same corner.

**Fix.** The booster attaches a corner on position and axes alone, and both
are consistent here. What separates the two is structural: an X-junction has
opposite sectors of equal colour, a T-junction has one side uniform. A recovery
predicate on that sector symmetry — not on a strength ratio, which would be a
threshold fitted to this frame — closes the gap; it needs the image, or a
symmetry descriptor from the corner detector.

### Gap 29 — ChArUco validation measures a corner against a global homography at a nominal scale — **open, measured**

ChArUco corner validation keeps a corner if it lies within
`corner_validation_threshold_rel × px_per_square` of the position predicted
by the board homography fitted to the decoded markers. Both parts of that are
global:
- one homography for the whole board, which is wrong under lens distortion
  and on non-planar views;
- the *nominal* `px_per_square` (60 px by default), not the cell size actually
  measured at the corner.

On small cells the threshold (≈ 4.8 px) is a large fraction of a cell. On
real blurred, oblique images some delivered corners sit several pixels off the
position a local homography of their own neighbours predicts. They pass
because the global prediction is off by about as much.

Re-detected corners pass the same test; the remaining cases are front-end
corners that validation keeps.

**Fix.**
- Validate against a *local* prediction: the neighbours' homography, or the
  line-intersection prediction of `DetectionQuality`'s local check.
- Express the tolerance in the locally measured cell size, as
  `roi_half_px_at` already does for the window.

This is structural, not a smaller threshold. It needs the regression gate and
a real-data before/after; the held-out calibration harness used for the
localizer study answers the accuracy side.

### Gap 30 — No saddle-point fit window suits both sharp and blurred corners — **open, measured**

The saddle-point refiner fits a fixed 5 × 5 window (radius 2). On real images:
- **Blurred corners need a larger window.** A larger window turns their loss
  against the center of mass into a win.
- **Small cells next to ChArUco markers need a smaller one.** The window
  starts to take in the marker border, which sits about an eighth of a cell
  from the corner.

Two emulated per-corner and per-image rules were tested on held-out
calibration and neither held everywhere:
- the radius scaled with a measured edge blur;
- the radius that minimizes the local grid residual.

This is why the center of mass stays the default (see the book's uncertainty
page).

**Fix direction.** Make the window respect the scene's structure, not only
its blur: stop it at the marker border or the nearest strong non-corner
edge. Then re-measure on the same held-out harness. It belongs upstream, in
`chess-corners`.

### Gap 31 — Per-corner blur: shipped for chessboard, PuzzleBoard and marker boards; ChArUco, non-Gaussian PSFs and in-patch distortion open

**What exists.** `calib_targets::blur::{chessboard_blur, puzzleboard_blur,
marker_board_blur}` return one `Result<CornerBlur, BlurDecline>` per corner of
a detection, from the original image. The estimator is
`calib_targets_core::blur` (pattern-agnostic: signed polygons in board units,
the known-cell region, a starting homography). Per corner it fits the raw
pixels of a board-space window with
`I = (1 + α dx + β dy)(C + A(1 − B_Σ))`, `B_Σ` the exact Gaussian mass of the
printed polygons (Owen's T, tabulated) under a free local homography,
integrated over the pixel box (four-point moment-matched mixture below
σ = 1.3 px, one Gaussian above). Σ is the full 2 × 2 *total* kernel
covariance (lens and unit pixel box; truth on the suite's renderer is
`Σ_lens + I/12`). Levenberg–Marquardt runs on the observable directions only:
homography directions the window does not constrain keep the neighbours'
values. Standard errors are a Bartlett HAC sandwich with Andrews' automatic
bandwidth from the residuals' lag-1 autocorrelation.

**Window and declines.** Window `ρ ∈ {½, ¾, 1, 1½, 2}` cells; it grows while
the photometric parameters inflate Var(σ) more than 4× (VIF) or while it is
narrower than `5σ + ½` px (the model's own neglect radius) along the local
frame's narrow axis; past 2 cells → `Confounded`. Pixels stay `3σ + ½` px
clear of cells not adjacent to a labelled corner (and of PuzzleBoard dots on
the known region's border). The delivered fit must beat the isotropic σ
profile at the caller's geometry on the same pixels (a nested model), else it
restarts there. `Σ̂` with `λ_min < 1/12` → `SharperThanPixel`.

**Measured (synthetic, `bench accuracy --only-blur`: chessboard, PuzzleBoard,
marker board; cells 16/32/64 px; blur 0.5–6 px; rotation 0/8/30°; tilt 0/30°;
noise 1/4; k₁ = 0.15 on 64 px).**

- Bias of σ: |median| ≤ 0.1 % in every group with estimates; robust scatter
  0.1–0.7 %. Over all 66 167 delivered estimates the SD of
  `(σ̂ − σ_true)/SE` is 1.03, 0.40 % beyond 3 SE; per-group robust honesty
  0.6–1.3.
- Coverage ≈ 1 up to σ/cell ≈ 0.08 (all families) and ≈ 0.13 (32 px cells);
  0.78–0.95 at σ/cell 0.1–0.16 on 16 px cells; ≈ 0 from σ/cell 0.19 (32 px)
  or 0.25 (16 px): `Confounded`.
- Pixel box: one Gaussian instead of the mixture biases σ by ≈ 0.06 %/σ⁴
  (σ in px; 0.6 % at 0.58 px); with the mixture ≤ 0.1 %, the rest being the
  renderer's own sub-sample variance.
- Anisotropic Gaussian (σ 2 × 1 px at 0/30/60/135°, 1.2 × 0.8 at 45°): every
  Σ component within 0.2 %, major axis within 0.02°; component honesty
  0.7–1.6 (Σxy on a chessboard the least determined, as expected from two
  edge directions).
- Defocus disk (r = 1/2/3 px): σ above the disk's second moment by
  +4.4/+8/+9 %; the Gaussian-equivalent width of a flat-topped PSF.
- Remaining failures: 2 of 66 167 estimates more than 5 SE off, both on the
  boundary row of 16 px boards at σ/cell ≈ 0.38 where the detector itself
  places the corners 1.5 px off.
- Cost (single thread): 3.0 ms/corner on a 14 px PuzzleBoard at 1 px blur,
  1.4 ms on a 64 px chessboard (the prototype took 40 and 184 ms); ~8 ms at
  2 px blur on 14 px cells (window grows).

**Real images.** On tilted-sensor PuzzleBoard frames most corners get an
estimate, the σ field is smooth with second differences consistent with the
stated standard errors, roughly a third of corners are anisotropic beyond
3 SE, and a corner costs a few tens of ms (wider windows, slower convergence
on structured misfit). Public phone photographs (`puzzleboard_reference/`) and
`mid.png` are sharpened in camera: their in-focus corners decline as
`SharperThanPixel`.

**What the number means.** The covariance of the Gaussian that best
reproduces this pattern's blur over the window: a width over the patch's
spatial frequencies, not the PSF's second moment; for a sharp-core,
heavy-tailed PSF smaller than the second moment and not additive in
quadrature; it depends on cell size and pattern.

**Open.**

- **ChArUco**: printed geometry deviates measurably from the nominal one and
  the residuals are structured; the standard errors are not honest there yet.
  No entry point until the marker print is modelled from the image or
  calibrated.
- **Non-Gaussian PSFs**: a core + tail (two-Gaussian) or disk model would
  report a second moment where the Gaussian reports an equivalent width.
- **Lens-distortion curvature** inside the window (the homography cannot
  follow it): +0.1 % at k₁ = 0.15 on 64 px cells; larger windows on strongly
  distorted lenses will show more.
- **Cost on real frames**: tens of ms per corner where blur forces 1–1.5-cell
  windows; per-σ circle polygonisation and a cheaper Owen's-T path are the
  levers.
- **Bindings** (Python, C, WASM): not exposed yet; they need a
  `GrayImageView`-based entry point (the facade takes `image::GrayImage`).

### Gap 32 — Wrong ChArUco labels under strong blur on the synthetic suite — **open, not yet localized**

`bench accuracy` (full grid) delivers 5 wrong labels on one ChArUco
condition: 32 px cells, no rotation or tilt, rendered blur 4 px, noise 4 gray
levels, center-of-mass localizer. Every other condition of the grid is
clean. A 4 px blur on a 32 px cell leaves marker cells of 4 px per bit, so
the marker decode is the first suspect, but which stage admits the labels
has not been measured.

**Fix.** Localize it with the debugging protocol (which corners, wrong grid
label or wrong id, which stage) before proposing a predicate; a wrong label
is a contract violation, not a tuning matter.

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

[2] C. Shu, A. Brunton, M. Fiala. "A topological approach to finding
    grids in calibration patterns." *Machine Vision and Applications*
    21(6), 2010. The Delaunay + colour-test grid finder that the
    topological builder re-implements with an axis-driven cell test.
