# projective-grid design

This document is the architectural north star for `projective-grid`. It
describes **what the crate is for** and **the three orthogonal axes** the
module structure is organised around. It is the companion to the per-strategy
stage maps:

- `docs/topological-grid-detection.md` (repo root) — the Topological grid
  finder, step by step.
- `crates/calib-targets-chessboard/docs/PIPELINE.md` — the topological path
  as composed by the chessboard detector.
- `crates/projective-grid/docs/ORIENTATION.md` — where per-corner orientation
  enters the strategy and how it can run orientation-free.

> Status note. The structural target below is realized: the `shared/` back-half,
> the sealed `Lattice` trait, and the `topological/` strategy directory all
> exist. **Topological is the sole square-grid assembler** — the historical
> `SeedAndGrow` strategy was retired (its geometry-only recovery schedule
> survived and now powers the topological synthesized-axis path). **Hex
> detection is wired on the topological path** — `(Hex, Positions)` and
> `(Hex, Oriented3)` are supported (square stays byte-identical); see
> *Extending to hex* below.

## Mission

> Given a cloud of 2D feature points — some spurious — optionally carrying
> local orientation, label every point that lies on a regular projective
> grid with its integer `(i, j)` lattice coordinate, with **zero wrong
> labels**.

A wrong `(i, j)` poisons downstream calibration and is unrecoverable; a
**missing** `(i, j)` is acceptable. Every algorithm in the crate is biased
toward dropping rather than mislabelling. This is the *precision-by-
construction* contract.

The crate is **image-free**: no image, pixel-buffer, or camera types appear in
the public surface, and no target-specific identifiers (marker IDs, ring IDs).
Corner *detection* (e.g. the ML `chess-corners` X-junction detector) happens
upstream; this crate only solves the **graph problem**: connect nodes → reject
outliers → assign labels.

## The three design axes

Everything in the crate is a point in a 3-dimensional design space. The module
structure is organised so each axis is legible and each is varied
independently.

### Axis 1 — lattice family

`Square` and `Hex` are both implemented. The family is captured by a **sealed**
`Lattice` trait carrying the family-specific primitives — neighbour offsets,
model-plane mapping (`model_point`), the symmetry group (D4 / D6), the
axis-family count (2 / 3), the model-plane axis directions, and the cell topology
(`TrianglePairToQuad` for square, `TriangleIsCell` for hex). The strategy and the
shared back-half are written against the trait, **not** duplicated per family.
The seal (a crate-private `Sealed` supertrait, impl'd only for `Square` / `Hex`)
let the hex work add new required trait methods without breaking external
callers, since no external crate can implement `Lattice`. See *Extending to hex*
below.

### Axis 2 — strategy

One strategy recovers the grid: **Topological**. It produces a `GridSolution`
that downstream consumers read uniformly.

| | Topological |
|---|---|
| Build | Delaunay → classify edges → merge cells → flood-fill labels |
| Speed | no global cell-size dependency; image-free |
| Used by | every square target — chessboard, ChArUco, puzzleboard, marker board |

The strategy recovers the **full** pattern with zero wrong labels — there is no
"denser" quality. Detection is binary per pattern.

`SquareAlgorithm` is a single-variant `#[non_exhaustive]` enum (only
`Topological`), kept as a reserved seam so a future alternative builder can be
added without a breaking change. The historical **SeedAndGrow** strategy — a
self-consistent 4-corner seed plus BFS predict-and-attach with fill / extend
boosters — was retired once the topological builder matched or beat it on every
shipping path, including the marker-bit-heavy ChArUco scenes it was once kept
for. ChArUco's old marker-bit fragility (ChESS corners *inside* marker bits
whose axes lock to the marker, not the grid) is now handled with a
`min_corner_strength` floor that keeps the topological grid out of the marker
interior; see `crates/calib-targets-charuco/docs/PIPELINE.md`. SeedAndGrow's
geometry-only recovery schedule (`RecoverySchedule`) outlived the strategy and
now powers the topological synthesized-axis path (see `ORIENTATION.md`).

### Axis 3 — input-feature kind

How much orientation each feature carries, modelled by `Evidence`. For square
lattices **all three are implemented** — the less-oriented kinds synthesize the
missing axes from neighbour geometry (`orient::synthesize_oriented2`,
`orient::synthesize_oriented2_from_oriented1`) and then run the same strategy:

- `Positions` — position only (0 axes); the **dot-grid** input. Both local grid
  directions are recovered from neighbour chords.
- `Oriented1` — one local lattice direction. The supplied axis is kept (trusted
  as evidence, anchored); the second is recovered from neighbours.
- `Oriented2` — two local directions (chessboard / ChArUco corners); the native
  shape, no synthesis.

- `Oriented3` — three local directions, the **hex-native** shape (a hexagonal
  lattice has three axis families). Consumed by the hex topological path;
  `(Square, Oriented3)` returns `UnsupportedCombination`.

(`CoordinateHypotheses` is a decode-feedback roadmap slot and returns
`UnsupportedCombination` for detection.)

Orientation is an **optional cue**, not a requirement. The *universal* cue is
the grid structure itself — rows are lines, columns are lines, local
homographies are consistent — which needs no orientation at all. Orientation,
when present, sharpens seeding and edge classification. See `ORIENTATION.md`
for exactly where each strategy consumes it and how each runs without it.

> Recall note. Zero wrong labels holds for all three kinds. The
> orientation-free path reaches **recall parity** with the two-axis path: the
> facade synthesizes the missing axes from neighbour geometry up front and runs
> the topological strategy on them, and the post-convergence recovery schedule
> closes the gap that a hard axis-voucher would otherwise leave under strong
> perspective. The parity is measured per-image (labelled-free /
> labelled-oriented) and gated; see `docs/development/detection-pipeline.md` and
> `ORIENTATION.md`.

## The pipeline: a strategy front-half + a shared back-half

The strategy's only job is to **build components** — connected, integer-labelled
patches of grid. Everything after that is **shared** and lattice-parameterised:

```text
detect_grid_all(request):
    components = strategy.build_components(features, classifier)
    merged     = shared::merge(components, lattice)     // reunite split patches
    validated  = shared::validate(merged, lattice)      // drop outliers
    solutions  = shared::fit(validated, lattice)        // homography + residuals
```

- `merge` reunites disconnected labelled components using **local geometry
  only** (no global homography), so it tolerates heavy radial distortion. The
  topological facade runs `merge_components_local` (`merge_walk_components`) on
  its per-component walk output; the chessboard adapter carries no private merge
  call.
- `validate` is the structural-cue gate: row/column collinearity
  ("lines are lines") + per-corner local-homography residual + edge-length
  band. It is **orientation-free**.
- `fit` estimates the model-plane → image projective transform and reports
  per-corner residuals.

Orientation enters **only** inside `build_components`, via the **classifier**
(Topological: edge Grid/Diagonal/Spurious). Lattice enters **only** via the
`Lattice` trait.

## Scalar precision: detection is `f32`, `geometry` stays generic

The **detection surface** is pinned to concrete `f32`: `feature`
(`PointFeature`, `LocalAxis`, `OrientedFeature<N>`, `CoordinateHypothesis`),
`detect` (`DetectionParams`, `DetectionRequest`, `Evidence`, `GridSolution`,
the `Square` topological strategy and the shared back-half), and the whole
`topological` engine plus the shared grid-growth machinery (grow, fill,
extension, recovery, validate) carry no `F: Float` type parameter. Only the
pure-numeric `geometry` module (`estimate_projective`, `apply_projective`,
`Homography<F>` and the residual helpers) remains generic over `F: Float`.

**Why.** The single-precision pin is not a precision regression. The ChESS
corner front-end, the chessboard / charuco / puzzleboard detectors, and every
cross-crate caller already feed and consume the grid surface at `f32`; the
chessboard adapter calls `geometry` at `f32` too. The only consumer that ever
instantiated the detection stack at `F = f64` was the long-removed generic-`F`
"Impl-1"; with it gone there is no remaining `f64` detection path, so the type
parameter bought nothing but `F::from(...)` noise on every literal and a
`T: Float` bound on every signature. `geometry` stays generic because the
projective-fit math is a standalone, reusable kernel a future `f64` calibration
consumer may legitimately want at double precision — and keeping it generic
costs nothing once the detection layer commits to `f32` at its boundary.

## Target module tree (≤ 2 levels; the three axes are legible)

```text
src/
  lib.rs            facade + two-tier docs
  feature/          AXIS 3: PointFeature, OrientedFeature<N>, CoordinateHypothesis
  orient.rs         AXIS 3: synthesize local axes from positions / one axis
  geometry/         homography / projective estimators + quality (one home)
  cluster/          double-angle axis 2-means (cluster_axes / AxisClusterCenters)
  result.rs · error.rs · float.rs
  lattice/          AXIS 1: the parameter, not a copy (sealed Lattice trait)
    mod.rs          trait Lattice { offsets, model_point, symmetry } + Sealed;
                    Coord, LatticeKind, GridTransform
    square.rs       impl Lattice for Square (D4, 4-neighbour, quad cell)
    hex.rs          impl Lattice for Hex   (D6, 6-neighbour, 3 families, triangle cell)
  topological/      AXIS 2, the strategy: delaunay → classify → quads → walk (square)
    mod.rs · delaunay.rs · classify.rs · quads.rs · filter.rs · walk.rs · axis.rs · trace.rs
    hex.rs          hex topological lattice math: triangle-as-cell classify + axial walk
    hex_detect.rs   hex orchestration: entry point + D6 merge + fit + assembly
  shared/           SHARED back-half (over Lattice) + grid-growth + recovery engine
    mod.rs · merge.rs (D4/D6) · validate/ · fit.rs (fit is pub(crate))
    grow/ (mod·params·predict — SquareAttachPolicy contract) · grow_extend.rs
    extension/ · fill.rs · recovery.rs · positions_policy.rs (private) · angle.rs
  detect.rs         facade: detect_grid / detect_grid_all, DetectionParams,
                    SquareAlgorithm; routes Evidence kind → synthesis + strategy;
                    runs strategy.build_components → shared merge/validate/fit
  check/            consistency check (caller-supplied hypotheses)
```

`shared/` holds the back-half (merge + validate + fit) **and** the
pattern-agnostic grid-growth + recovery engine (grow / fill / extension /
recovery). That engine was relocated from the retired `seed_and_grow/`
directory: it is no longer a separate strategy but the geometry-only recovery
schedule the topological synthesized-axis path runs (and that the chessboard
crate composes directly for its own recovery). `topological/` is the sole
strategy directory. The flat layout is realized — the old
`detect/advanced/square/*` nesting and the duplicate generic seed-and-grow
implementation are gone.

### Two public tiers

The crate exposes two tiers (see the `lib.rs` module docs):

- **Stable tier** — the crate-root facade re-exports (`detect_grid*`,
  `check_consistency`, the request / result / evidence / lattice types, the
  `orient::synthesize_*` helpers). The supported surface for external callers.
- **Advanced tier** — the `shared` and `topological` engine modules,
  semver-exempt pre-1.0, for in-workspace consumers (the chessboard detector)
  that compose the engine directly with their own policies. This includes the
  grid-growth and recovery primitives under `shared` (grow / fill / extension /
  recovery) the chessboard crate drives for its own recovery path. Items here
  change shape as the engine is refactored; engine items with no external
  consumer are `pub(crate)`.

## Extending to hex (without copying machinery)

Hex detection is wired on the **topological path** by filling in the trait, not
by duplicating folders. What was done:

1. `impl Lattice for Hex` in `lattice/hex.rs` — axial neighbour offsets,
   `model_point` (axial → model plane), D6 symmetry, plus the three trait
   additions the detection path needs: `axis_family_count` (3),
   `model_axis_directions` (0°/60°/120°), and `cell_topology`
   (`TriangleIsCell`).
2. `shared/` (merge / validate / fit) depends only on `Lattice`: `fit` uses
   `model_point` unchanged; `merge` is lattice-parameterized —
   `merge_components_local_for(lattice)` selects D4 or D6 via
   `symmetry_transforms`. Hex precision rests on the projective fit residual
   (the square-oriented row/column validate is skipped on hex).
3. The hex topological arm lives in `topological/hex.rs`: on a hex **point**
   lattice the Delaunay triangles *are* the unit cells, so there is no diagonal
   class and `quads.rs` (triangle-pair-to-quad merge) is **bypassed**.
   Classification keeps triangles whose three edges align with three distinct
   axis families; the walk labels axial `(q, r)` by parallelogram completion
   (`d = a + b − c` across each shared edge). Delaunay and the projective fit
   back-half are shared with the square topological path.

**What hex does *not* get:** the post-convergence recovery schedule (boundary
extension / interior fill / rescue) is square-only — its grid-growth primitives
assume a 4-neighbour D4 lattice. Hex has no recovery stage. The geometry-only hex
primitives (per-cell mesh, rectification, smoothness) remain a roadmap item (see
`docs/algorithmic_gaps.md` Gap 4). If oriented hex support is added later, model
orientation as a set of **local lattice directions**, not as square-style x/y
axes.

## Invariants any change must preserve

- **Grid labels are non-negative.** Every returned component is rebased so its
  labelled bounding-box minimum is `(0, 0)`.
- **Quad / homography corner order is TL, TR, BR, BL** (clockwise); never a
  self-crossing order.
- **Zero wrong labels** on the regression sets is non-negotiable; recall is
  tracked but may move. Validate independently of the pass that produced a
  label (an independent geometric predicate), and inspect overlays for crossing
  / non-cardinal edges — a passing position-only check does not validate new
  `(i, j)` assignments.
