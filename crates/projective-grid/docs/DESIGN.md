# projective-grid design

This document is the architectural north star for `projective-grid`. It
describes **what the crate is for** and **the three orthogonal axes** the
module structure is organised around. It is the companion to the two
per-strategy stage maps:

- `docs/topological-grid-detection.md` (repo root) — the Topological grid
  finder, step by step.
- `crates/calib-targets-chessboard/docs/PIPELINE.md` — the SeedAndGrow path
  as composed by the chessboard detector.
- `crates/projective-grid/docs/ORIENTATION.md` — where per-corner orientation
  enters each strategy and how each can run orientation-free.

> Status note. This document describes the **target** architecture the crate
> is converging on. Some of the structure below (e.g. the `shared/` back-half,
> the `Lattice` trait, the flat `seed_and_grow/` + `topological/` siblings) is
> being introduced incrementally; where current and target differ, the text
> says so.

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

`Square` today; `Hex` is a roadmap stub (`LatticeKind::Hex`, `D6_TRANSFORMS`,
`HEX_AXIAL_OFFSETS` exist; detection does not yet). The family is captured by a
`Lattice` trait carrying the family-specific primitives — neighbour offsets,
model-plane mapping (`model_point`), the symmetry group (D4 / D6), and the seed
/ cell shape. The strategies and the shared back-half are written against the
trait, **not** duplicated per family. See *Extending to hex* below.

### Axis 2 — strategy

Two strategies recover the grid; both are first-class and both are kept. They
produce the **same** output (`GridSolution`), so downstream consumers stay
agnostic to which ran.

| | Topological | SeedAndGrow |
|---|---|---|
| Build | Delaunay → classify edges → merge cells → flood-fill labels | seed quad → BFS predict-and-attach (+ fill/extend boosters) |
| Speed | ~10× faster | slower |
| Robustness | sensitive to corners *inside* marker bits (their axes lock to the marker, not the grid) | tolerant — required for ChArUco |
| Default for | PuzzleBoard (today); plain chessboard (planned, gated) | ChArUco (always) |

Both strategies recover the **full** pattern with zero wrong labels — there is
no "denser" quality. Detection is binary per pattern; the only real
differences are **speed** and **marker-bit robustness**. The choice of default
per target follows from those two properties alone.

### Axis 3 — input-feature kind

How much orientation each feature carries, modelled by `Evidence`:

- `Positions` — position only (0 axes); the planned **dot-grid** input.
- `Oriented1` — one local lattice direction.
- `Oriented2` — two orthogonal local directions (chessboard / ChArUco corners).

Orientation is an **optional cue**, not a requirement. The *universal* cue is
the grid structure itself — rows are lines, columns are lines, local
homographies are consistent — which needs no orientation at all. Orientation,
when present, sharpens seeding and edge classification. See `ORIENTATION.md`
for exactly where each strategy consumes it and how each runs without it.

## The pipeline: a strategy front-half + a shared back-half

A strategy's only job is to **build components** — connected, integer-labelled
patches of grid. Everything after that is **shared** and lattice-parameterised:

```text
detect_grid_all(request):
    components = strategy.build_components(features, policy_or_classifier)
    merged     = shared::merge(components, lattice)     // reunite split patches
    validated  = shared::validate(merged, lattice)      // drop outliers
    solutions  = shared::fit(validated, lattice)        // homography + residuals
```

- `merge` reunites disconnected labelled components using **local geometry
  only** (no global homography), so it tolerates heavy radial distortion. It is
  shared by **both** strategies. (Historically only the topological path called
  it and SeedAndGrow kept a single connected component by construction; that
  asymmetry is being removed — SeedAndGrow may return multiple components and
  run merge like topological.)
- `validate` is the structural-cue gate: row/column collinearity
  ("lines are lines") + per-corner local-homography residual + edge-length
  band. It is **orientation-free**.
- `fit` estimates the model-plane → image projective transform and reports
  per-corner residuals.

Orientation enters **only** inside `build_components`, via a **policy**
(SeedAndGrow: seed + attach rules) or a **classifier** (Topological: edge
Grid/Diagonal/Spurious). Lattice enters **only** via the `Lattice` trait.

## Scalar precision: detection is `f32`, `geometry` stays generic

The **detection surface** is pinned to concrete `f32`: `feature`
(`PointFeature`, `LocalAxis`, `OrientedFeature<N>`, `CoordinateHypothesis`),
`detect` (`DetectionParams`, `DetectionRequest`, `Evidence`, `GridSolution`,
the `Square` strategy front-halves and the shared back-half), and the whole
`advanced::square` engine (seed, grow, validate, topological) no longer carry
a `F: Float` type parameter. Only the pure-numeric `geometry` module
(`estimate_projective`, `apply_projective`, `Homography<F>` and the residual
helpers) remains generic over `F: Float`.

**Why.** The single-precision pin is not a precision regression. The ChESS
corner front-end, the chessboard / charuco / puzzleboard detectors, and every
cross-crate caller already feed and consume the grid surface at `f32`; the
chessboard adapter calls `geometry` at `f32` too. The only consumer that ever
instantiated the detection stack at `F = f64` was the now-deleted generic-`F`
"Impl-1" (`src/{seed,grow,validate}/`). With Impl-1 gone there is no remaining
`f64` detection path, so the type parameter bought nothing but `F::from(...)`
noise on every literal and a `T: Float` bound on every signature. `geometry`
stays generic because the projective-fit math is a standalone, reusable kernel
a future `f64` calibration consumer may legitimately want at double precision —
and keeping it generic costs nothing once the detection layer commits to `f32`
at its boundary.

## Target module tree (≤ 2 levels; the three axes are legible)

```text
src/
  lib.rs            facade + this design summary
  feature.rs        AXIS 3: PointFeature, OrientedFeature<N>, Evidence
  geometry.rs       homography / projective estimators + quality (one home)
  result.rs · error.rs · float.rs
  lattice/          AXIS 1: the parameter, not a copy
    mod.rs          trait Lattice { offsets, model_point, symmetry, seed/cell };
                    Coord, LatticeKind, GridTransform
    square.rs       impl Lattice for Square (D4, 4-neighbour, 2×2 seed, quad cell)
    hex.rs          impl Lattice for Hex   (D6, 6-neighbour, …) — roadmap stub
  shared/           SHARED back-half (generic over Lattice), used by BOTH strategies
    mod.rs · merge.rs · validate.rs · fit.rs
  seed_and_grow/    AXIS 2, strategy 1: build_components = (multi-seed) grow [+ boosters]
    mod.rs · seed.rs · grow.rs · policy.rs · boosters.rs
  topological/      AXIS 2, strategy 2: build_components = delaunay → classify → cells → walk
    mod.rs · delaunay.rs · classify.rs · cells.rs · walk.rs
  detect.rs         facade: detect_grid / detect_grid_all, DetectionParams,
                    SquareAlgorithm; routes Evidence kind → policy/classifier;
                    runs strategy.build_components → shared merge/validate/fit
  check.rs          consistency check (caller-supplied hypotheses)
```

seed / grow / policy / boosters belong to `seed_and_grow/` (they are *not*
shared — topological never uses them). `shared/` holds only merge + validate +
fit. Each strategy is a sibling directory; `shared/` is the third sibling.

> Current layout (pre-restructure) differs: the production SeedAndGrow
> mechanics live under `detect/advanced/square/*`, the topological strategy
> under `detect/square/topological/`, and a second, unused generic
> seed-and-grow implementation lives under top-level `seed/`, `grow/`,
> `validate/` behind the public `detect_grid*` facade. Collapsing the
> duplicate and flattening to the tree above is the structural work in
> progress.

## Extending to hex (without copying machinery)

Hex is added by **filling in the trait**, not by duplicating folders:

1. `impl Lattice for Hex` in `lattice/hex.rs` — axial neighbour offsets,
   `model_point` (axial → model plane), D6 symmetry, and the hex seed / cell
   shape.
2. `shared/` (merge / validate / fit) already depends only on `Lattice`: `fit`
   uses `model_point`; `merge` uses the symmetry group; `validate`'s
   "lines are lines" becomes the three axial line families the trait exposes.
   **No new files.**
3. Each strategy's lattice-specific arm is filled behind the trait:
   - SeedAndGrow — the neighbour set and seed-cell shape.
   - Topological — `cells.rs` assembles a hexagon (six triangles) instead of a
     quad (two triangles); Delaunay, the BFS skeleton, and the label walk are
     unchanged.

The geometry-only hex primitives (D6 alignment, per-cell mesh, rectification,
smoothness) existed previously and can be resurrected from history as the
starting point. If oriented hex support is added later, model orientation as a
set of **local lattice directions**, not as square-style x/y axes.

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
