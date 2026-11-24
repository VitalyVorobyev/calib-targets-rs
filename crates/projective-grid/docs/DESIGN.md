# projective-grid design

This document is the architectural north star for `projective-grid`: **what the
crate is for**, **the three orthogonal axes** the module structure is organised
around, and **the public surface** it ships. Companion stage maps:

- `docs/algorithms/topological-grid-detection.md` (repo root) — the Topological grid
  finder, step by step.
- `crates/calib-targets-chessboard/docs/PIPELINE.md` — the topological path
  as composed by the chessboard detector.
- `crates/projective-grid/docs/ORIENTATION.md` — where per-corner orientation
  enters the strategy and how it can run orientation-free.

Open work on this crate is tracked in the gaps ledger
(`docs/algorithms/algorithmic_gaps.md`) and in `ROADMAP-1.0.md`.

## Mission

> Given a cloud of 2D feature points — some spurious — optionally carrying
> local orientation, conservatively recover integer lattice coordinates and
> a projective fit, preferring a missing label to an unsupported assignment.

A wrong `(i, j)` poisons downstream calibration and is unrecoverable; a
**missing** `(i, j)` is acceptable. Every algorithm in the crate is biased
toward dropping rather than mislabelling. This is the *precision-by-
construction* contract.

The crate is **image-free**: no image, pixel-buffer, or camera types appear in
the public surface, and no target-specific identifiers (marker IDs, ring IDs).
Corner *detection* (e.g. the ChESS X-junction detector in `chess-corners`)
happens upstream; this crate only solves the **graph problem**: connect nodes →
reject outliers → assign labels.

## The three design axes

Everything in the crate is a point in a 3-dimensional design space. The module
structure is organised so each axis is legible and each is varied
independently.

### Axis 1 — lattice family

`Square` and `Hex`. The family is captured by a **sealed** `Lattice` trait
carrying the family-specific primitives — neighbour offsets, model-plane
mapping (`model_point`), the symmetry group (D4 / D6), the axis-family count
(2 / 3), the model-plane axis directions, and the cell topology
(`TrianglePairToQuad` for square, `TriangleIsCell` for hex). The strategy and the
shared back-half are written against the trait, **not** duplicated per family.
The seal (a crate-private `Sealed` supertrait, implemented only for `Square` /
`Hex`) means no external crate can implement `Lattice`, so required methods can
be added without a breaking change.

### Axis 2 — strategy

One strategy recovers the grid: **Topological** (Delaunay → classify edges →
merge cells → flood-fill labels). It has no global cell-size dependency, and
every square target in the workspace — chessboard, ChArUco, PuzzleBoard, marker
board — runs it. There is no algorithm choice in the request.

The strategy is precision-biased, but does not claim complete recall or a
mathematical zero-wrong guarantee for arbitrary input. Both membership and
label correctness are measured properties of each evidence/domain campaign.

ChArUco marker-bit corners are kept out of the grid by a `min_corner_strength`
floor; see `crates/calib-targets-charuco/docs/PIPELINE.md`.

### Axis 3 — input-feature kind

How much orientation each feature carries, modelled by `Evidence`. The
less-oriented kinds synthesize the missing axes from neighbour geometry
(`expert::orientation::synthesize_*`) and then run the same strategy:

- `Positions` — position only (0 axes); the **dot-grid** input. All local grid
  directions are recovered from neighbour chords.
- `Oriented1` — one local lattice direction. The supplied axis and sigma are
  kept (trusted as evidence, anchored); square recovers one missing family and
  hex recovers two from six-neighbour chord evidence.
- `Oriented2` — two local directions (chessboard / ChArUco corners); the native
  square shape, no synthesis. `(Hex, Oriented2)` returns
  `UnsupportedCombination`.
- `Oriented3` — three local directions, the **hex-native** shape (a hexagonal
  lattice has three axis families). `(Square, Oriented3)` returns
  `UnsupportedCombination`.

The full `(LatticeKind, Evidence)` support matrix is in the rustdoc of
`detect_grid`.

Orientation is an **optional cue**, not a requirement. The *universal* cue is
the grid structure itself — rows are lines, columns are lines, local
homographies are consistent — which needs no orientation at all. Orientation,
when present, sharpens edge classification. See `ORIENTATION.md` for exactly
where the strategy consumes it and how it runs without it.

Only `(Square, Oriented2)` has real-image evidence; every other combination is
experimental — see Gap 21 in `docs/algorithms/algorithmic_gaps.md`.

## The pipeline: a strategy front-half + a shared back-half

The strategy's job is to **build components** — connected, integer-labelled
patches of grid. Everything after that is **shared** and lattice-parameterised.
For square lattices (`topological/square_detector/`):

```text
detect_grid_all(request):
    features   = synthesize missing axes (Positions / Oriented1 only)
    components = topological walk + shared::merge     // reunite split patches
    components = shared::recovery_schedule(...)       // synthesized-axis evidence only
    for each component:
        kept      = shared::validate(component)       // drop outliers
        canonical = normalize(kept)                   // stable public frame
        solution  = shared::fit(canonical, lattice)   // projective fit + residuals
```

- `merge` reunites disconnected labelled components using **local geometry
  only** (no global homography), so it tolerates heavy radial distortion
  (`merge_components_local`; `merge_components_local_for` selects D4 or D6).
- `recovery_schedule` is the geometry-only extend → fill → revalidate → drop
  fixed point. `RecoverySchedule::Auto` (the default) enables it only for
  synthesized axes; see `ORIENTATION.md`.
- `validate` is the structural-cue gate: row/column collinearity
  ("lines are lines") + per-corner local-homography residual, with an optional
  edge-shape gate. It is **orientation-free**.
- `fit` estimates the model-plane → image projective transform and reports
  per-corner residuals.

Pattern-specific target detectors may branch after the merge via
`expert::square::assemble_oriented2_components`. That seam preserves the
topological axis-slot frame and intentionally does not promise the public
canonical frame or a fit. The chessboard detector uses it, then runs its own
parity-aware recovery and a further two-step merge (shared corner identity,
then a second `merge_components_local`); see the chessboard
`docs/PIPELINE.md`, *Multi-component dispatch*.

Orientation enters **only** inside component building, via the edge
**classifier** (Grid / Diagonal / Spurious). Lattice enters **only** via the
`Lattice` trait.

## Scalar precision: detection is `f32`, numeric kernels stay generic

The **detection surface** is pinned to concrete `f32`: `feature`
(`PointFeature`, `LocalAxis`, `OrientedFeature<N>`, `CoordinateHypothesis`),
`detect` (`DetectionParams`, `DetectionRequest`, `Evidence`, `GridDetection`),
the `topological` engine, and the shared grid-growth machinery (grow, fill,
extension, recovery, validate) carry no scalar type parameter. Only the
pure-numeric kernels stay generic over the scalar: `geometry`
(`estimate_projective`, `apply_projective`, `estimate_homography`,
`Homography<F>`) and `predict_grid_position`.

**Why.** The ChESS corner front-end and every detector feed and consume the grid
surface at `f32`, so a scalar parameter on the detection stack would buy only
conversion noise on every literal and a bound on every signature. The
projective-fit kernels are standalone and reusable at `f64` by a calibration
consumer, so they stay generic; `estimate_projective` solves in `f64` whatever
the caller's scalar.

## Module tree

```text
src/
  lib.rs            crate-root facade (re-exports) + crate docs
  expert.rs         curated composition namespace (see "Public surface")
  diagnostics.rs    opt-in (`diagnostics` feature) stage traces + rejection reports
  detect.rs         detect_grid / detect_grid_all, DetectionParams, DetectionTuning;
                    routes (LatticeKind, Evidence) → synthesis → topological strategy
  feature/          AXIS 3: PointFeature, LocalAxis, OrientedFeature<N>, CoordinateHypothesis
  orient/           AXIS 3: synthesize local axes from positions / one axis
  cluster/          double-angle axis 2-means (cluster_axes, AxisClusterCenters)
  geometry/         projective / homography estimators + quality (one DLT core)
  lattice/          AXIS 1: sealed Lattice trait + Square / Hex impls,
                    Coord, LatticeKind, GridTransform, predict_grid_position
  spatial.rs        PointIndex: static kd-tree behind every neighbour query
  result.rs · error.rs · float.rs
  check/            consistency check (caller-supplied hypotheses)
  topological/      AXIS 2, the strategy
    square_detector/    square orchestration (TopologicalParams) + walk-output merge (mod.rs);
                        component validate / fit / triangulation helpers (solution.rs)
    delaunay.rs · classify.rs · quads.rs · filter.rs · walk.rs · axis.rs
    trace.rs            diagnostics-only traced walk
    hex.rs              hex lattice math: triangle-as-cell classify + axial walk
    hex_detect.rs       hex orchestration: D6 merge + fit + assembly
  shared/           lattice-parameterised back-half + square grid-growth engine
    merge.rs · validate/ · fit.rs
    grow/ (mod · params · predict) · grow_extend.rs · fill.rs · extension/
    recovery_schedule.rs · positions_policy.rs
```

`shared/` holds the back-half (merge + validate + fit) **and** the
pattern-agnostic grid-growth + recovery engine (grow / fill / extension /
recovery schedule). The topological synthesized-axis path runs that engine;
the chessboard crate composes the grow / fill primitives for its own recovery.
`topological/` is the sole strategy directory. Every module except `expert` and
`diagnostics` is private; the public surface is the re-exports below.

## Public surface

The crate ships three entry points (canonical statement: the `lib.rs` crate
docs and `expert.rs` module docs):

- **Crate root** — `detect_grid`, `detect_grid_all`, `check_consistency`, the
  request / params / evidence types, the result model (`GridDetection`,
  `GridEntry`, `LabelledGrid`, `LatticeFit`, …), `Coord`, `LatticeKind`,
  `GridDimensions`, and `GridError`. The supported surface for an ordinary
  caller who wants to detect a grid and read labels back.
- **`expert`** — the curated composition namespace for detector builders that
  add pattern-specific admission, recovery, or validation:
  `orientation` (`cluster_axes`, `synthesize_*`), `geometry`, `lattice`
  (`Lattice`, `Square`, `Hex`, `GridTransform`, D4/D6 tables,
  `predict_grid_position`), `square` (`assemble_oriented2_components`),
  `component` (`merge_components_local`), `attachment` (`SquareAttachPolicy`,
  `GrowResult`, …), `fill` (`fill_grid_holes`), `validation` (`validate`,
  `wrong_label_filters`), and the tuning types `DetectionTuning`,
  `TopologicalParams`, `RecoverySchedule`, `RecoveryParams`. Its stage-shaped
  types may evolve faster than the crate root (Gap 22 in
  `docs/algorithms/algorithmic_gaps.md`).
- **`diagnostics`** (feature `diagnostics`) — exact stage traces and rejection
  reports, kept out of the ordinary result.

**The composition contract.** A consumer supplies an
`expert::attachment::SquareAttachPolicy` — the seam between the geometry-only
growth machinery and the pattern's rules, answering `is_eligible` /
`required_label_at` / `accept_candidate` / `edge_ok` per candidate — and drives
the growth primitives (`expert::fill::fill_grid_holes`) under it. After its
front-half builds integer-labelled components, it composes the back-half:
local-geometry merge (`expert::component`), the structural-cue
`expert::validation` gate, and the projective fit (`expert::geometry`).
**Guarantee:** every stage is *drop-only* with respect to labels — a corner
whose geometry does not cohere is removed, never relabelled by that stage. This
limits damage but is not proof that an earlier assignment was correct; an
external composition needs its own target-domain quality gate. The in-workspace
chessboard detector is the reference composition.

### Production path vs. library-only breadth

The public surface divides into a **production** path that every in-workspace
detector exercises and a **library-only** breadth that is intended external
product but exercised only by this crate's own tests:

| | Production path | Library-only breadth |
|---|---|---|
| Lattice | `LatticeKind::Square` | `LatticeKind::Hex`, `expert::lattice::{Hex, D6_TRANSFORMS, HEX_AXIAL_OFFSETS}` |
| Evidence | `Evidence::Oriented2` | `Evidence::Positions` · `Evidence::Oriented1` · `Evidence::Oriented3` |
| Axis synthesis | none (native two-axis input) | `expert::orientation::synthesize_*` |
| Recovery | off on the native path | `RecoverySchedule` (the synthesized-axis recall path) |
| Exercised by | every in-workspace detector (chessboard, ChArUco, PuzzleBoard, marker) | `projective-grid`'s own tests only |

The library-only breadth is published so a downstream user can detect a dot
grid, a circle grid, or a hex target without re-deriving the machinery.

**It stays compiled in, not feature-gated.** Gating `Hex`,
`Evidence::Positions`, the synthesis helpers, and the recovery schedule behind
off-by-default features would be a semver break for every downstream user and
add a CI feature-matrix obligation, against a compile-time or surface-size
benefit that is unmeasured. Revisit only if that cost is measured.

## Hex

Hex detection runs on the topological path by filling in the `Lattice` trait,
not by duplicating folders:

1. `impl Lattice for Hex` in `lattice/hex.rs` — axial neighbour offsets,
   `model_point` (axial → model plane), D6 symmetry, `axis_family_count` (3),
   `model_axis_directions` (0°/60°/120°), and `cell_topology`
   (`TriangleIsCell`).
2. `shared/` merge and fit depend only on the lattice: `fit` uses `model_point`
   unchanged; `merge_components_local_for(lattice)` selects D4 or D6 via
   `symmetry_transforms`. Hex precision rests on the projective fit residual
   (the square-oriented row/column validate does not run on hex).
3. The hex topological arm lives in `topological/hex.rs`: on a hex **point**
   lattice the Delaunay triangles *are* the unit cells, so there is no diagonal
   class and `quads.rs` (triangle-pair-to-quad merge) is **bypassed**.
   Classification treats each feature's three slots as an unordered local set:
   an edge may match different slot indices at its endpoints, while the two
   incident edges at each vertex must use different local families. The walk
   labels axial `(q, r)` by parallelogram completion
   (`d = a + b − c` across each shared edge). Delaunay and the projective fit
   are shared with the square path.

`Oriented1` hex input anchors one measured physical family and internally
estimates the two missing local families without assuming an image-frame ±60°
rotation. `Oriented3` is modelled as an unordered set of **local lattice
directions**, not as globally stable slot identities. Hex has no recovery
schedule (Gap 4 in `docs/algorithms/algorithmic_gaps.md`).

## Invariants any change must preserve

- **Grid labels are non-negative.** Every returned component is rebased so its
  labelled bounding-box minimum is `(0, 0)`.
- **Quad / homography corner order is TL, TR, BR, BL** (clockwise); never a
  self-crossing order.
- **Zero wrong labels** on each named regression set is a release gate, not a
  universal guarantee; recall is tracked but may move. Validate independently
  of the pass that produced a label (an independent geometric predicate), and
  inspect overlays for crossing / non-cardinal edges — a passing position-only
  check does not validate new `(i, j)` assignments.
