# Changelog

All notable changes to this project will be documented in this file.

This project follows [Semantic Versioning](https://semver.org/).

Older releases are archived under [`docs/changelog/`](docs/changelog/);
see [Older releases](#older-releases) at the bottom for the index.

## Unreleased

### Changed

- **The detector structs and the facade free functions are one symmetric
  surface.** Breaking; the full before/after is in
  [`docs/migrations/0.13.0.md`](docs/migrations/0.13.0.md).

  `CharucoParams`, `PuzzleBoardParams` and `MarkerBoardParams` each carried a
  `chess: DetectorConfig` field that no detector ever read. Only the facade
  free functions ran the ChESS corner pass, so a caller who wanted to configure
  corner detection *and* keep a reusable detector had to abandon the detector
  API, run the corner pass by hand, and feed the corners back in — and every
  binding crate hand-rolled that same glue, one of them down to a private copy
  of the corner-descriptor conversion.

  Detectors now run the corner pass themselves, from the field they already
  owned. `detect(image)` is the whole pipeline; `detect_with_corners(image,
  corners)` is the injection variant for sharing one corner pass across several
  target detectors; `detect_corners(image)` exposes the pass a given detector
  would run, so the corners you inject are the ones `detect` would have
  produced. That identity is a test, not a promise:
  `d.detect(img) == d.detect_with_corners(img, &d.detect_corners(img))`, and
  likewise `detect_t(img, &p) == TDetector::new(p)?.detect(img)`.

  `detect_with_diagnostics` becomes `diagnose_with_corners`, joined by
  `diagnose(image)`, so the `_with_corners` suffix means exactly one thing
  everywhere. The same rename reaches the Python, WASM and C ABI surfaces.

  Successful detections are unchanged — bit-identical, because
  `chess_corners::Detector::detect` is literally its own `detect_u8` on the
  same three values. Every break is a compile error at the call site; nothing
  keeps compiling with a changed meaning.

  `ChessboardDetector` is deliberately untouched. It consumes a corner cloud by
  design, and `ChessboardParams` has no `chess` field because that struct is
  embedded inside all three composite params types, where a nested
  corner-detector config would be dead in exactly the way this release removes.

- **MarkerBoard reports why it failed.** `MarkerBoardDetector` returned
  `Option`, collapsing "no chessboard grid" and "grid found but the three
  circle markers did not agree with the board" into one `None`. It now returns
  `Result<_, MarkerBoardDetectError>` like ChArUco and PuzzleBoard, and its
  `diagnose*` methods return diagnostics **even on failure** — which is when
  the scored circle candidates and attempted matches are most worth having.
  The set of inputs that succeed is unchanged; only the failure value carries
  information now.

  `MarkerBoardDetector` still has no `detect_all`, and that is intentional: a
  marker board is localised by its three circle markers, which realistically
  fall inside a single connected grid component, whereas ChArUco and PuzzleBoard
  can be anchored from two disjoint fragments and therefore consume every
  component.

- **PuzzleBoard decodes only physically reachable board orientations by
  default.** A camera imaging the printed side of an opaque planar board can
  see it rotated by a multiple of 90°, but never mirrored: a rigid pose plus a
  perspective projection preserves handedness. The decoder nevertheless
  searched all eight dihedral relabellings, so four of every eight hypotheses
  were unreachable — and they were not free. An unreachable hypothesis that
  happens to match the observed bits competes in the uniqueness gate and turns
  a correct decode into a rejection. The default search is now the four
  rotations (`PuzzleBoardDecodeConfig::symmetry_mode`, new
  `PuzzleBoardSymmetryMode::Rotations`), which halves the decode work *and*
  removes that class of spurious ambiguity: clean-window uniqueness begins at a
  smaller fragment than it did under the dihedral search.

  Set `symmetry_mode = PuzzleBoardSymmetryMode::RotationsAndReflections` to
  restore the previous behaviour. That is the correct setting when the optical
  path flips handedness — a mirror or beam splitter in the path, or an image
  mirrored before detection. Under the default, a mirrored view declines to
  decode rather than returning a wrong absolute labelling.

### Added

- **Five matching entry points per compound target on the facade** —
  `detect_t`, `detect_t_with_corners`, `diagnose_t`,
  `diagnose_t_with_corners` and `detect_t_best`, for ChArUco, PuzzleBoard and
  marker boards. `detect_puzzleboard_with_corners` previously existed but was
  private. The `diagnose_*` free functions return
  `(Result<TDetection, DetectError>, Option<TDiagnostics>)`; the `Option` is
  `None` in exactly one case, where the facade constructed the detector for you
  and the params were rejected before the pipeline ran.

- **Chessboard diagnostics are reachable from the facade.**
  `trace_topological` and `trace_topological_detection` are re-exported under
  the `diagnostics` feature. A Rust caller previously had to depend on
  `calib-targets-chessboard` directly to reach them.

- **`calib_targets::chessboard::detect_corners(image, cfg)`** — one shared
  corner pass over a borrowed `GrayImageView`, replacing four copies of the
  corner-descriptor conversion that had accumulated across the facade, the WASM
  bindings, and two examples/tests.

- **`MarkerBoardParams::sweep_for_board`** — `detect_marker_board_best`
  existed with no preset to feed it. The new preset sweeps the shared
  grid-build axis only; no circle-scoring or matching constants are varied.

### Fixed

- **Python's parameter defaults matched Rust's in name only.** The Python
  package mirrors the Rust params structs as dataclasses, and their literal
  default values had drifted. Two of the five were precision defects rather
  than preferences, so a Python caller who constructed params directly was
  running with weaker guarantees than the equivalent Rust caller and had no
  way to know:

  | field | was | now |
  |---|---|---|
  | `PuzzleBoardDecodeConfig.min_window` | 4 | **7** |
  | `ChessboardParams.min_corner_strength` | 0.0 | **33.0** |
  | `CircleScoreParams.min_contrast` | 60.0 | 10.0 |
  | `CharucoParams.min_marker_inliers` | 3 | 1 |
  | `ScanDecodeConfig.min_border_score` | 0.45 | 0.75 |

  `min_window` is the bounded-distance uniqueness floor. At 4 the decoder
  accepted fragments far too small to be provably unique — a latent
  **false-positive** path, and a wrong absolute corner ID is unrecoverable
  downstream. `min_corner_strength` is the floor that clears false corners
  produced by marker bits; at 0.0 they come back, and because
  `ChessboardParams` is embedded in the ChArUco and marker-board params, that
  one leaked into three detectors.

  Callers who set these fields explicitly are unaffected. The Rust pipeline
  never changed.

- **The Python multi-config sweep presets are computed by Rust now**, instead
  of being re-implemented by hand. `PuzzleBoardParams.sweep_for_board` had
  silently drifted onto a different axis entirely — it varied the ChESS
  corner-detector threshold where Rust varies the grid-graph angular
  tolerances — so `detect_puzzleboard_best` explored a different configuration
  space from Python than from Rust while its docstring claimed the two
  matched. The config count is unchanged; the axis is now Rust's.
  `ChessboardParams.sweep_default` and `CharucoParams.sweep_for_board`, which
  had no Python surface at all, are now exposed the same way. A parity test
  fails if either side moves alone.

## 0.12.1

Precision fix. No public API change in the `calib-targets*` crates; the
workspace now builds against `projective-grid` 0.14 (see that crate's
changelog for its own breaking change).

### Fixed

- **Grid labels are always projectively self-consistent.** The map from a
  detection's `(grid.u, grid.v)` to its `position` is a homography by
  construction; the detector could break that, emitting a labelled set in
  which several lattice nodes collapsed onto one pixel or a sub-population was
  displaced by a whole lattice step. Such a detection still carries enough
  correspondences to pass a count check while its pose solve is meaningless
  ([#86](https://github.com/VitalyVorobyev/calib-targets-rs/issues/86)).

  Two producers are fixed at the source:

  - **A lattice-orientation parity invariant.** A homography preserves
    orientation over any region that does not cross its vanishing line, which
    an imaged planar board never does. The sign of the local basis
    `e_u × e_v` is therefore the same at every cell of a correctly labelled
    component, under any viewpoint and any amount of smooth lens distortion. A
    component merge that accepts a *mirroring* symmetry element on thin overlap
    glues a sub-block in with a reversed axis, which reverses that sign while
    leaving edge lengths, edge directions and pixel separations plausible —
    invisible to every first-order wrong-label check. The parity check compares
    a sign, so it has no tolerance to tune, and it drops only labels it can
    prove wrong.
  - **Two-way injectivity in both component merges.** One lattice coordinate
    holds one corner, and one corner sits at one coordinate. Only the
    coordinate direction was guarded before, so a thin overlap could re-label a
    single physical corner at a run of lattice coordinates.

- **ChArUco runs the chessboard's wrong-label geometry check.**
  `CharucoParams::for_board` used to disable it, on the reasoning that the
  downstream board alignment would catch mislabels. It cannot: the alignment
  searches `D4 × integer translation`, a *rigid* relabelling of whatever
  lattice the chessboard produced, so it applies the healthy region's alignment
  to a broken region rather than detecting one.

- **ChArUco corner re-detection cannot merge two board corners.** The search
  window is now sized from the cell pitch predicted at that corner rather than
  the nominal `px_per_square`, so a foreshortened region cannot open a window
  wide enough to reach the neighbouring corner, and a re-detected corner may be
  adopted by at most one board id.

- **`projective-grid`: detection no longer depends on hash iteration order.**
  `detect_grid` could return different labellings for byte-identical input
  across processes — most runs labelling a 24 × 24 dot grid in full, roughly one
  in thirty dropping a whole component
  ([#77](https://github.com/VitalyVorobyev/calib-targets-rs/issues/77)). Three
  reductions over the labelled set read `HashMap` iteration order, which `std`
  reseeds per process: the boundary-extension BFS seeded its queue from the map
  and then claimed corners first-come-first-served, and `ensure_axes` and
  `cell_size_of` accumulated `f32` sums, which are not associative. All three
  now iterate in sorted cell order. Measured on a 144-regime sweep, one regime
  returned four different labellings over 40 repeats of the same input before
  the fix and one after.

- **PuzzleBoard: declaring the board is no longer a pessimisation.**
  `PuzzleBoardSearchMode::FixedBoard` matched observations against a
  materialised copy of the declared board's bit pattern, costing
  `O(8 · (rows+1)(cols+1) · N)` — slower than not declaring the board at all
  above roughly 22 squares, and pathologically so at scale. A declared board is
  a sub-rectangle *cut from* the master, so its bit at board cell `(r, c)` is
  the master bit at `(origin + r, origin + c)`: the same scoring problem,
  restricted to a rectangle of origins, sharing the master search's cyclic
  class tables. Restricting the origins also restricts the residue classes they
  can reach, so declaring the board is now genuinely *cheaper* than not
  declaring it, at every board size below the master's own.

  The restricted scan also considers only shifts that keep every observation on
  the board. An observation exists only where a dot was sampled, and a dot is
  only sampled where the corner neighbourhood bounding it was detected, so
  every observation does lie on the printed board and a shift placing one
  outside it cannot describe the physical scene. Excluding those keeps
  physically impossible placements out of the uniqueness gate, where they could
  only ever suppress a correct decode.

### Changed

- **`projective-grid`: `merge_components_local` takes the components plus one
  shared `positions` slice; `ComponentInput` is removed.** Every component now
  indexes the same corner array, which is what makes "the same corner appears
  in two components" a well-defined question — and therefore what makes the
  injectivity guarantee above checkable. The previous shape let each component
  carry its own index space, where the question has no answer.

- **PuzzleBoard decode is substantially faster, with the same output.** Two
  changes beyond the fixed-board rework above, neither of which alters which
  origin is decoded:

  - *The cyclic class precompute is bounded by residue groups, not observation
    count.* An observation reaches the tables only through
    `(bit, lookup mod period)`; observations agreeing on those credit the same
    cells with the same shape of contribution and are summed once. There are at
    most `2 · 167 · 3` such groups per orientation, so the precompute goes from
    `O(501 · N)` to `O(N + min(N, 6w) · 501)` for a `w`-square window — flat in
    the observation count in practice.
  - *The soft scorer's log-likelihood is accumulated in fixed point.* The
    crossed-CRT separation that collapses the `501²` origin scan needs an
    integer key: a table entry below the maximum must be at least one below it,
    so that it provably cannot reach the maximum sum. `f32` rounding breaks that
    step, which is why the soft path previously walked every origin while the
    hard path did not. Fixed-point accumulation restores the property — and
    makes the table sums exactly reproducible regardless of accumulation order.

  On a public fixture the default `Full` + `SoftLogLikelihood` path drops from
  6.3 ms to 3.0 ms end to end, and decode is no longer the leading stage at any
  board size a caller would realistically print.

- **PuzzleBoard is instrumented under the `tracing` feature.** The crate
  declared the feature but instrumented nothing, so there was no way to see
  where a `detect` call spent its time. Spans now cover the chessboard grid
  build, per-component decode, edge sampling, the class precompute, and the
  origin scan. `calib-targets-bench` gains a `puzzleboard_stage_timing` binary
  alongside the topo / charuco / full ones.

- **PuzzleBoard documentation now matches the code.** The book chapters, crate
  docs, and internal spec had drifted: the two code maps' roles were swapped
  (map A governs vertical edges, map B horizontal), the dot polarity was
  inverted (`0` is black), `min_window` was documented as 4 rather than 7, and
  a `HardMajority` scoring mode was described that does not exist. Two claims
  were wrong rather than merely stale — the shipped maps are *imported* from the
  reference implementation (PStelldinger/PuzzleBoard, CC0) rather than generated
  by this crate's tool, and the "a 4 × 4 fragment identifies its absolute
  position" property holds only at a fixed orientation. Since the decoder must
  search all eight D4 transforms, clean uniqueness begins at 6 × 6, which is
  what justifies the `min_window` default of 7. A new book chapter covers the
  code-map construction and the registration path from origin to absolute IDs.

## 0.12.0

Breaking API-consolidation release. See the
[0.12 migration guide](docs/migrations/0.12.0.md).

### Changed

- **One canonical affine `GridTransform`.** `projective-grid` now owns the
  lattice kind, row-major integer matrix, and translation in one structure.
  `calib_targets_core::GridTransform` re-exports it and `GridAlignment` is a
  semantic type alias. Detector results no longer compose two structures for
  the same grid-coordinate mapping.
- **Python/WASM/Serde alignment shape is now
  `{ lattice, matrix, translation }`.** Python and TypeScript keep
  `GridAlignment` as an alias. The C ABI stays unchanged through an adapter.
- **`projective-grid` reusable primitives use the expert composition seam.**
  Import prediction and transforms from `expert::lattice`, and homography
  estimation from `expert::geometry`; `Coord` remains at the crate root.

### Fixed

- Removed duplicate square midpoint prediction, modulo-π angular helpers, and
  D4 matrix literals. Compatibility entry points and historical tie-breaking
  order now delegate to the canonical implementations.
- Affine grid-transform inversion now includes translation and rejects
  non-unimodular integer matrices.

## 0.11.2

Dependency-cleanup patch: no public API or behaviour changes.

### Changed

- **Migrated to `chess-corners` 1.2 and dropped the direct
  `chess-corners-core` dependency.** The ChArUco local corner re-detection
  and the FFI config lowering previously hand-composed low-level
  `chess-corners-core` primitives (patch response + origin-offset image view
  + refiner plumbing); they now drive the facade's single-scale ROI entry
  point (`Detector::detect_u8_roi`, new in `chess-corners` 1.2). One fewer
  direct dependency, one strategy-lowering site upstream, identical
  detection results (full regression suites pass unchanged).

Metadata-only patch: no code or behaviour changes.

### Fixed

- **MSRV corrected to 1.91.** The declared `rust-version = "1.88"` had gone
  stale: `chess-corners` 1.1 requires rustc 1.91, so 0.11.0 could not actually
  be built on 1.88–1.90 despite its metadata claiming otherwise (Cargo does
  not cross-check `rust-version` against dependencies). The workspace now
  declares 1.91 — the same floor as `chess-corners` — and CI gained an `msrv`
  job that builds with 1.91 on every PR so the claim cannot rot silently
  again.
- **Dropped the unused `fixed` dependency.** `kiddo`'s default features pull
  in fixed-point axis support that no crate here uses (all k-d trees are
  `f32`); the workspace now depends on `kiddo` with
  `default-features = false, features = ["tracing"]`. This also removes the
  one transitive that would otherwise have forced the MSRV to 1.93.

## 0.11.0

This release is dominated by the migration to **`chess-corners` 1.0**, an
upstream API freeze that removed or reshaped most of the corner front-end's
public surface. Riding along with it: the projective-grid generalization, a
batched public-surface cleanup, and the resolution of five RustSec
advisories. The workspace is still `0.x`, so breaking changes are expected.

The ChESS response kernel is unchanged between `chess-corners` 0.11.2 and
1.0.0, so the migration itself is pure API reshaping with byte-identical
detections. The **one** deliberate behavioural change is the chessboard
pre-filter: `contrast` and `fit_rms` no longer exist upstream, so the
fit-RMS admission rule was replaced by a gate on the axes' own reported
angular uncertainty (see *Changed* below).

See the [migration guide](docs/migrations/0.11.0.md) for before/after
snippets across Rust, JSON, C, Python, and TypeScript.

### Added

- **The `image` crate is re-exported as `calib_targets::image`** (behind the
  default `image` feature). Import `image` types through the re-export instead
  of adding a separate `image = "0.25"` dependency to guarantee your
  `GrayImage` type matches the one the `detect_*` helpers accept — a version
  mismatch otherwise produces a confusing "expected `GrayImage`, found
  `GrayImage`" error. A direct `image` dependency still works; the re-export is
  purely additive.

- **Every detector's params struct carries its own ChESS corner front-end
  (`chess: DetectorConfig`).** `CharucoParams`, `PuzzleBoardParams`, and
  `MarkerBoardParams` gain a `chess` field defaulting to
  `default_chess_config()`. Previously the facade helpers
  `detect_charuco` / `detect_puzzleboard` / `detect_marker_board` hardcoded
  `detect_corners_default(img)`, so the coarse-to-fine
  `MultiscaleConfig::Pyramid` and the pre-pipeline `UpscaleConfig::Fixed`
  stages were reachable for chessboard detection only — an asymmetry, not a
  decision (the C ABI and the Python bindings had both already grown their own
  way to pass one). Overriding the corner pass is now a matter of setting one
  field, and it travels with the serialized config through JSON, Python, the C
  ABI, and TypeScript.

  `UpscaleConfig::Fixed` is the knob for low-resolution boards whose corners
  land inside the ChESS ring margin, and it rescales output coordinates back
  to input pixels — unlike upscaling the image yourself, which leaves every
  corner in the upscaled frame for the caller to undo.

  All three target-detector sweep helpers — `detect_charuco_best`,
  `detect_marker_board_best`, and `detect_puzzleboard_best` — honour each
  config's own `chess` front-end and deduplicate corner detection across configs
  that request the same one. A sweep may freely mix corner front-ends (e.g. a
  single-scale config alongside an `UpscaleConfig::Fixed(2)` config), while a
  sweep whose configs share `chess` still runs exactly one corner pass.

- **`projective_grid::predict_grid_position` / `PredictedPosition`** — the
  neighbour-midpoint position predictor is back on the stable tier, as one
  lattice-generic function (`lattice::predict`). It averages the midpoints of
  the available opposite neighbour pairs (2 axis families on square, 3 on hex
  axial) from a `HashMap<Coord, Point2<F>>`, returning `None` on the labelled
  frontier — interpolation only, never extrapolation. This restores the
  capability of the removed 0.9 `square_predict_grid_position` /
  `hex_predict_grid_position` helpers (same midpoint-pair math, so downstream
  smoothness gates keep their numerical behaviour) for consumers that run
  local, homography-free outlier checks or seed searches for missed lattice
  points; `n_axis_pairs` reports how constrained each prediction is.
- **`Evidence::Oriented1`** — single-supplied-axis input is now a first-class
  evidence kind for `projective_grid::detect_grid`; the second axis is
  recovered from neighbour-chord geometry.
- **Hexagonal lattice detection** — `projective_grid` now detects hex
  dot/marker grids via the topological builder for `Positions` and `Oriented3`
  evidence (`Lattice::Hex`).
- **`projective_grid::cluster::cluster_axes`** and `AxisClusterCenters` — the
  axis-clustering primitive is exposed from the facade.
- **`calib_targets_core::cell_rect_corners_at`** — the single shared definition
  of the canonical unit-cell corner order (TL, TR, BR, BL), used by the ArUco
  and ChArUco cell samplers.

### Breaking

- **Every facade `detect_*` entry point returns `Result<_, DetectError>`.**
  `detect_chessboard`, `detect_chessboard_best`, `detect_marker_board`, and
  `detect_marker_board_best` previously returned `Option`, while the ChArUco and
  PuzzleBoard helpers returned `Result` — so consuming more than one target type
  meant switching control-flow shape for no user-visible reason. A board that is
  simply not present is now reported as the new
  `DetectError::NoDetection { target }` variant. The `_from_gray_u8` pair for
  chessboard and marker additionally flattens from `Result<Option<_>, _>` to
  `Result<_, _>`. `detect_chessboard_all` still returns `Vec`, and the
  crate-level detectors (`ChessboardDetector::detect`,
  `MarkerBoardDetector::detect`) still return `Option` — only the facade
  unifies. Idiomatic bindings are behaviour-preserved: Python returns `None` on
  a miss, WASM returns `null`, the C ABI keeps its existing "not found" status.

- **`CharucoParams::for_board` and `PuzzleBoardParams::for_board` take the board
  by value** (`for_board(spec)` instead of `for_board(&spec)`), matching
  `MarkerBoardParams::for_board`. The rule is uniform: constructors that store
  the board take it by value; `sweep_for_board(&spec)` presets, which clone into
  N configs, keep taking it by reference. The board specs are `Copy`, so most
  call sites just drop the `&`.

- **`calib_targets::chessboard::AdvancedTuning` is renamed to
  `ChessboardAdvancedTuning`**, matching the `CharucoAdvancedTuning` /
  `PuzzleBoardAdvancedTuning` family. Pure rename: the serde key stays
  `"advanced"` and the JSON shape is unchanged.

- **`chess-corners` 0.11 → 1.0.** The corner front-end froze its API, and the
  removed surface reaches every layer of this workspace:

  - **The ChESS threshold is a plain `f32`, not a tagged enum.** The
    `Threshold` type is gone; the ChESS strategy reads a single absolute
    floor on the raw response. `with_threshold(Threshold::Absolute(v))`
    becomes `with_threshold(v)`, and the JSON shape
    `{"threshold": {"absolute": 15.0}}` becomes `{"threshold": 15.0}`.
    **There is no ChESS relative mode any more** — relative thresholding
    survives only on the Radon strategy, so `Threshold::Relative(_)` on a
    ChESS config has no mechanical translation; choose an absolute floor.
    The workspace default is unchanged at `15.0`
    (`calib_targets::detect::default_chess_config()`), which is now
    explicitly *lower* than upstream's own new default of `30.0`.
  - **The low-level `ChessParams` collapses `threshold_rel` +
    `threshold_abs` into one `threshold`.** Pre-1.0 the floor resolved as
    `threshold_abs.unwrap_or(threshold_rel * max_response)` with
    `threshold_abs: Some(0.0)` by default — so code that defaulted a
    `ChessParams` and set only `threshold_rel` was already running at an
    absolute floor of `0.0`. Two workspace redetect paths were in exactly
    that state; they map to `threshold = 0.0`, which is behaviour-preserving.
  - **`nms_radius` / `min_cluster_size` moved out of the ChESS strategy
    config** into a strategy-independent `detection` block:
    `with_chess(|c| c.nms_radius = …)` → `with_detection(|d| d.nms_radius = …)`,
    and the two JSON keys move from `strategy.chess` to a top-level
    `detection` object. The low-level `ChessParams` keeps both fields.
  - **`DescriptorRing` is removed.** The descriptor always follows the
    detector ring radius, which is what the previous default
    (`FollowDetector`) already did — no behaviour change at the default.
  - **`chess_corners::low_level` / `unstable` are removed.** The contract
    they exposed (`ChessParams`, `RefinerKind`, `ImageView`, `Roi`,
    `Refiner`, `chess_response_u8_patch`,
    `detect_corners_from_response_with_refiner`) now lives at the root of the
    **`chess-corners-core`** crate, which is a new direct dependency of the
    ChArUco, PuzzleBoard, and FFI crates. `PyramidParams` is not re-exported
    by 1.0 (it moved to `box-image-pyramid`); construct
    `MultiscaleConfig::Pyramid { levels, min_size, refinement_radius }`
    directly.
  - **`CornerDescriptor::axes` is now `Option<[AxisEstimate; 2]>`**, and
    `contrast` / `fit_rms` are no longer reported. `None` maps to the
    existing no-information sentinel (`AxisEstimate::default()`, `sigma = π`).

- **`calib_targets_chessboard::ChessCorner` drops `contrast` and `fit_rms`,
  and `AdvancedTuning::max_fit_rms_ratio` is removed.** They are unbacked
  now that upstream stopped reporting the underlying scalars. Nothing
  replaced the knob — see *Changed* for the derived gate that took over its
  job. The removal propagates through the FFI (`ct_chessboard_advanced_t`),
  Python (`ChessboardParams`), WASM, and Studio surfaces.

- **The ONNX-backed ML refiner is now opt-in (`ml-refiner` cargo feature,
  off by default).** Every crate previously enabled `chess-corners/ml-refiner`
  unconditionally, which linked the `tract` ONNX runtime into every build —
  taking the published WASM bundle from ~0.85 MB to ~10 MB — while nothing in
  this workspace ever selects `ChessRefiner::Ml`. It also broke the
  `wasm32-unknown-unknown` build outright once `chess-corners` 1.1 moved to
  `tract` 0.23, whose transitive `getrandom` 0.4 requires a backend chosen at
  build time. With the feature off the WASM bundle is ~1.5 MB.

  Turn it back on with `features = ["ml-refiner"]` on `calib-targets` (or on
  any individual detector crate; the Python bindings expose the same feature).
  While it is off, the serde value `{"refiner": "ml"}` does not deserialize —
  it is rejected, never silently downgraded. Every other refiner
  (`center_of_mass`, `forstner`, `saddle_point`) is unaffected.

- **`nalgebra` 0.34 → 0.35.** `nalgebra` types appear in the public API of
  `projective-grid` and `calib-targets-core` (`Point2<f32>`, `Matrix3<f32>`,
  `LabeledCorner.position`, …), so consumers must move to the same major
  version to interoperate. No workspace source change was required and
  detection output is byte-identical; the bump also drops the unmaintained
  `paste` crate from the `nalgebra` → `simba` path (see *Security* below).

- **`GridCoords` is removed; `projective_grid::Coord` (`{ u, v }`) is the single
  canonical grid-coordinate type.** The workspace-local `GridCoords { i, j }`
  type and its re-exports (from `calib-targets-core` and the `calib-targets`
  facade) are deleted with no deprecated alias. `Coord` is now re-exported from
  `calib-targets-core` and the facade in its place, and every output struct that
  carried a grid label (`LabeledCorner.grid`, `ChessboardCorner.grid`,
  `CharucoCorner.grid`, `MarkerBoardCorner.grid`, `PuzzleBoardCorner.grid`,
  `MarkerDetection.gc`, `ResolvedTargetPoint.grid`) now carries a `Coord`. The
  test-only conversion shims `grid_coords_to_next` / `grid_coords_from_next` /
  `grid_alignment_to_next` / `grid_alignment_from_next` are removed. The axis
  convention is preserved exactly: the old `i` (grid-right / first axis) is
  `Coord::u`, and the old `j` (grid-down / second axis) is `Coord::v`.
  - **Report JSON keys change `{ "i", "j" }` → `{ "u", "v" }`** for every
    grid-coordinate field (`grid`, `gc`). The marker-board cell index
    (`MarkerCircleSpec.cell`, Rust `CellCoords`) is unaffected and keeps its
    `{ "i", "j" }` shape. The Python and TypeScript/WASM binding types are
    updated to match (`Coord` with `u`/`v` for grid coordinates; `CellCoords`
    retained for the marker cell).
  - **The FFI C ABI is unchanged.** `ct_grid_coords_t` keeps its historical
    `{ i, j }` C field names and struct layout (no churn after the ffi 2.0.0
    release); the Rust→C conversion maps `Coord::u → i` and `Coord::v → j`.

- **`calib_targets_chessboard::ChessboardDetector::new` is now fallible**
  (`-> Result<Self, ChessboardParamsError>`), validating the configuration up
  front; the previous infallible `new` + `try_new` pair and the internal
  debug-assert/empty-result fallback are removed. This mirrors the fallible
  constructors on the sibling detectors. `MarkerBoardDetector::new` is likewise
  fallible (reusing `ChessboardParamsError`), and `PuzzleBoardSpecError` gains a
  `Chessboard` variant so `PuzzleBoardDetector::new` surfaces an invalid
  embedded chessboard configuration.

- **The seed-and-grow grid builder is retired; `Topological` is the sole
  builder.** Both seed-and-grow engines — the chessboard pipeline's own and
  `projective-grid`'s `SquareAlgorithm::SeedAndGrow` — are deleted, and ChArUco,
  PuzzleBoard, and marker boards all run the topological builder now (a
  `min_corner_strength` floor pre-filters the marker-bit corners that the old
  ChArUco pin had guarded against). `GraphBuildAlgorithm` and `SquareAlgorithm`
  collapse to single-variant `#[non_exhaustive]` enums (only `Topological`),
  retained as reserved config seams; the wire string `"seed_and_grow"` no longer
  deserializes. The chessboard `AdvancedTuning` block drops its
  seed-and-grow-only stage knobs (`seed_*`, `rescue_*`, `refit_*`,
  `boundary_extension_*`, `partial_slot_flip_*`, and the dead BFS-validate
  tolerances), with the removal propagated through the FFI / Python / WASM /
  Studio surfaces. The chessboard rich `DebugFrame` diagnostics and the
  experimental `OrientationSource::NeighbourEdges` path are removed with the
  engine.

- **The legacy ChArUco vote matcher is retired; the board-level matcher is the
  sole marker-to-board matcher.** The board-level matcher was already the
  default, so detection behaviour is unchanged. The
  `CharucoParams::use_board_level_matcher` field is removed (an unknown serde
  key is now ignored on deserialization, with no behaviour change). The
  rotation+translation vote solver is deleted; `CharucoAlignment` (the
  alignment result the board-level matcher returns) is retained. The
  `MatcherDiagKind` enum and the `ComponentDiagnostics.matcher` field are
  removed, so the `matcher` key is dropped from the ChArUco diagnostics JSON —
  the WASM diagnostics type and Python overlay are updated to match.
  `CharucoParams::max_hamming` is removed in the same change: it fed only the
  retired hard-decode vote matcher and became a no-op once that matcher was
  deleted (the board-level matcher uses soft-bit scoring and a margin gate, with
  no Hamming cap). The Rust field, the WASM `CharucoParams` type entry, and the
  Python `CharucoParams` dataclass field are removed; an unknown `max_hamming`
  serde / config key is now ignored on deserialization.

- **`calib-targets-ffi` is bumped to 3.0.0 — three struct-layout-breaking C
  ABI changes.** Recompile C/C++ consumers against the regenerated
  `calib_targets_ffi.h`; `ct_version_string()` and the CMake config-version
  now report `3.0.0`.
  1. `ct_chess_params_t` collapses `threshold_rel` (`float`) +
     `threshold_abs` (`ct_optional_f32_t`) into a single absolute
     `threshold` (`float`), and drops `descriptor_use_radius10`.
  2. `ct_chessboard_advanced_t` drops `max_fit_rms_ratio` along with the rest
     of the pre-filter section.
  3. `ct_charuco_detector_params_t` drops `max_hamming` (it only fed the
     retired ChArUco vote matcher).

  The `ct_optional_bool_t` type is **removed entirely** — nothing declares an
  optional boolean any more, so delete any local `none_bool()`-style helper.
  `ct_grid_coords_t` is unchanged (see the `Coord` entry above).

- **ChArUco and PuzzleBoard diagnostics moved behind an opt-in `diagnostics`
  cargo feature** (default off), matching `calib-targets-chessboard`. The
  `diagnostics` module, the diagnostics type re-exports, and
  `detect_with_diagnostics` reach the public surface only with the feature
  enabled; the facade `calib-targets/diagnostics` feature now forwards to all
  three detector crates.

- **`ChessboardParams.min_labeled_corners` / `max_components` are now defaulted on
  deserialization** (`8` / `3`), so partial and legacy configs that omit them
  deserialize again. Values and serialization are unchanged.

- **Naming-family rename across the four detector crates** (API revision
  decision 1; see the [migration guide](docs/migrations/0.11.0.md#11-api-revision-s1-the-naming-family-rename)
  for the full table and before/after snippets). No deprecated aliases.
  - `calib-targets-chessboard`: `Detector` → `ChessboardDetector`,
    `DetectorParams` → `ChessboardParams`.
  - `calib-targets-charuco`: `CharucoDetectionResult` → `CharucoDetection`.
  - `calib-targets-puzzleboard`: `PuzzleBoardDetectionResult` →
    `PuzzleBoardDetection`.
  - `calib-targets-marker`: `MarkerBoardDetectionResult` →
    `MarkerBoardDetection`; `MarkerBoardParams.layout` field → `.board`
    (serde key `"layout"` → `"board"`); `MarkerBoardParams::new(layout)` →
    `MarkerBoardParams::for_board(board)` (uniform with
    `CharucoParams::for_board` / `PuzzleBoardParams::for_board`);
    `MarkerBoardDetector::detect_from_image_and_corners[_with_diagnostics]`
    → `detect[_with_diagnostics]` (`detect_from_corners`
    `[_with_diagnostics]`, the corners-only secondary path, is unchanged).

- **Dead public-API items removed** (API revision decision 2; see the
  [migration guide](docs/migrations/0.11.0.md#12-api-revision-s2-dead-item-removal)
  for the full removed-items table and replacements). Every item had zero
  real consumers, verified by a workspace-wide sweep.
  - `PuzzleBoardParams.corner_redetect_params` is removed — the field was
    never read by the detector; its C-ABI mirror
    (`ct_puzzleboard_params_t.corner_redetect_params`) is removed too, and
    the FFI header is regenerated. (ChArUco's `corner_redetect_params` is
    unaffected — it is live there.)
  - `calib-targets-core`'s five internal crate-bridge functions
    (`homography_to_next`, `homography_from_next`, `axis_estimate_from_next`,
    `grid_transform_to_next`, `grid_transform_from_next`) are removed; each
    had zero consumers outside the crate's own round-trip tests.
    `axis_estimate_to_next` is retained (used by the chessboard pipeline and
    the bench harness).
  - `calib-targets-core::{init_with_level, init_tracing}` and the `logger`
    module are removed. Every consumer was an example; each now installs its
    own `env_logger` / `tracing_subscriber` locally (or, where neither is a
    dependency, drops the call — the `log`/`tracing` macros already used
    throughout those examples degrade to no-ops without an installed
    logger). The crate drops its now-unused `log`, `tracing`, and
    `tracing-subscriber` dependencies; the `tracing` cargo feature is
    unchanged.
  - `calib_targets::detect::default_puzzleboard_params` is removed — inline
    `PuzzleBoardParams::for_board(&PuzzleBoardSpec::new(rows, cols, 1.0)?)`
    at the call site. The Python and WASM `default_puzzleboard_params`
    bindings keep their existing behaviour.
  - `ChessboardParams::topological()` is removed — it was already identical
    to `default()` (`Topological` is the sole grid builder).
  - `calib_targets_chessboard::detect_all_topological` is no longer
    re-exported from the crate root (`pub(crate)` now); it had no external
    callers. `ChessboardDetector::detect_all` is the public production path.
    `trace_topological` is unaffected.

- **Tuning knobs moved behind an opt-in `advanced` tier for ChArUco and
  PuzzleBoard** (API revision decision 3; see the
  [migration guide](docs/migrations/0.11.0.md#13-api-revision-s3-tuning-knobs-moved-to-advanced)).
  This brings both configs onto the small-stable-core-plus-unstable-`advanced`
  discipline the chessboard config already uses. **Detection is byte-identical
  at default params** — the moved knobs keep their defaults and an unset
  `advanced` behaves exactly like the advanced struct's `Default`.
  - `CharucoParams.advanced` becomes `Option<Box<CharucoAdvancedTuning>>`
    (`None` = defaults), set via `CharucoParams::with_advanced(...)` and read
    via `effective_tuning()`. `grid_smoothness_threshold_rel`,
    `corner_validation_threshold_rel`, and `min_secondary_marker_inliers` move
    off the stable core into `CharucoAdvancedTuning`, serializing under the
    nested `"advanced"` object.
  - `CharucoParams.corner_redetect_params` becomes `pub(crate)` — it leaked the
    upstream `chess_corners` parameter type and was `#[serde(skip)]`, so it was
    never settable through JSON or the bindings. Its C-ABI mirror
    (`ct_charuco_detector_params_t.corner_redetect_params`) is dropped and the
    header regenerated; corner refit behaviour is unchanged.
  - `PuzzleBoardDecodeConfig` gains
    `advanced: Option<Box<PuzzleBoardAdvancedTuning>>`; the soft-scorer knobs
    `bit_likelihood_slope` / `per_bit_floor` / `alignment_min_margin` move into
    it (serializing under `decode.advanced`). The 5-argument positional
    `PuzzleBoardDecodeConfig::new(...)` is removed — build with `Default` +
    struct-update.
  - C ABI: the ChArUco `grid_smoothness_threshold_rel` /
    `corner_validation_threshold_rel` and the PuzzleBoard
    `bit_likelihood_slope` / `per_bit_floor` / `alignment_min_margin` stay as
    flat C fields; the conversion routes them into the new advanced locations
    internally, so no C caller change is needed for those.

- **Diagnostics are feature-gated uniformly across all four detectors**
  (API revision decision 4; see the
  [migration guide](docs/migrations/0.11.0.md#14-api-revision-s4-diagnostics-are-feature-gated-uniformly)).
  `calib-targets-marker` gains an opt-in `diagnostics` cargo feature (default
  off), matching `calib-targets-charuco` / `-puzzleboard`; without it,
  `MarkerBoardDiagnostics` and `MarkerBoardDetector::detect_with_diagnostics`
  are absent from the public API. `calib-targets-chessboard`'s
  `trace_topological` moves behind the crate's existing (previously
  code-gating-nothing) `diagnostics` feature. The facade
  `calib-targets/diagnostics` feature now forwards to all four detector
  crates. Python, WASM, and FFI already enable the facade feature (WASM also
  enables each detector crate's feature directly) unconditionally, so this is
  manifest-only for them.
  - `MarkerBoardDetector::detect_from_corners` and
    `detect_from_corners_with_diagnostics` are **removed** — zero consumers
    workspace-wide; they duplicated `calib-targets-chessboard`'s image-free
    detector for the corners-only case. Detect directly against
    `calib-targets-chessboard` for that case.

- **Example/CLI file-config types left the library API** (API revision
  decision 5; see the
  [migration guide](docs/migrations/0.11.0.md#15-api-revision-s5-examplecli-config-types-left-the-library-api)).
  `calib-targets-charuco`, `calib-targets-puzzleboard`, and
  `calib-targets-marker` each had a public `io` module holding a JSON config
  type and a JSON report type (`image_path` / `output_path` fields, plus
  params-construction logic duplicating `{X}Params::for_board`) that existed
  only to drive that crate's own example or the `charuco_stage_timing` bench
  harness from a JSON file — tooling, not part of the calibration-detection
  contract.
  - `CharucoDetectConfig`, `CharucoDetectReport`, `CharucoConfigError`,
    `CharucoIoError` are removed from `calib-targets-charuco`'s public API;
    trimmed private copies now live in the charuco crate's own
    `examples/charuco_detect.rs` and in
    `calib-targets-bench/src/charuco_config.rs` (used by
    `charuco_stage_timing`).
  - `PuzzleBoardDetectConfig`, `PuzzleBoardDetectReport`,
    `PuzzleBoardIoError` are removed from `calib-targets-puzzleboard`'s
    public API; a trimmed private copy now lives in
    `calib-targets/examples/detect_puzzleboard.rs`.
  - `MarkerBoardDetectConfig`, `MarkerBoardDetectReport`,
    `MarkerBoardIoError` are removed from `calib-targets-marker`'s public
    API; a trimmed private copy now lives in
    `calib-targets-marker/examples/marker_detect.rs`.
  - `resolve_dictionary` moves to `calib-targets-aruco`;
    `calib-targets-charuco` re-exports it at its crate root, so existing
    `calib_targets_charuco::resolve_dictionary` call sites are unaffected.
  - `load_board_spec_any` / `BoardSpecLoadError` stay public — they are a
    genuinely useful tolerant board-spec loader, not an example type — but
    move from `calib-targets-charuco::io` to `calib-targets-charuco::board`
    (still re-exported at the crate root).
  - `calib_targets_core::io::{load_json, write_json, IoError}` — the generic
    JSON helpers the removed types were built on — are unaffected and stay
    public.
  - py/wasm/studio never depended on any of the removed types (verified by a
    workspace-wide consumer sweep); this step is a no-op for every binding.

- **Unified multi-config sweep semantics** (API revision decision 6; see the
  [migration guide](docs/migrations/0.11.0.md#16-api-revision-s6-unified-multi-config-sweep-semantics)).
  `detect_charuco_best`, `detect_marker_board_best`, and
  `detect_puzzleboard_best` converge on one contract: each now honours every
  config's own `chess` front-end instead of forcing a single corner pass for the
  whole sweep, so a sweep may mix front-ends (e.g. a default pass alongside an
  `UpscaleConfig::Fixed(2)` pass). Corner detection runs once per *distinct*
  `chess` front-end and is memoized across configs that request the same one, so
  a same-`chess` sweep still pays for exactly one corner pass. Configs are tried
  in their given order and per-helper scoring / tie-breaks are unchanged.
  - `DetectError::InconsistentChessConfig` is **removed** — it existed only to
    reject the mixed-front-end sweeps the helpers can now serve, leaking the
    "one corner pass" implementation detail into the user-facing contract.
    `detect_chessboard_best` is unchanged — it takes one explicit `chess_cfg`
    for the whole sweep.
  - The Python and WASM `detect_*_best` bindings delegate to these facade
    helpers, so the browser and Python surfaces share the unified per-config
    `chess` + deduplicated-corner-pass semantics (previously the WASM sweeps ran
    one workspace-default corner pass and ignored each config's `chess`).

- **Convention fixes: `#[non_exhaustive]`, result serde, a dead error variant,
  and uniform `deny_unknown_fields`** (API revision decision 7; see the
  [migration guide](docs/migrations/0.11.0.md#17-api-revision-s7--s8-convention-fixes-and-the-chess-config-rule)).
  - `MarkerDetection`, `MarkerCell`, `CellSamples` (`calib-targets-aruco`),
    `RectifiedView` (`calib-targets-core`), and `ChessCorner`
    (`calib-targets-chessboard`) become `#[non_exhaustive]` and gain named
    constructors (`MarkerDetection::new` + `with_scores`/`with_inverted`/
    `with_corners_img`; `MarkerCell::new`; `CellSamples::new`;
    `RectifiedView::new`; `ChessCorner::new`). Reading code is unaffected;
    cross-crate literal construction moves to the constructor.
  - `ChessboardDetection`/`ChessboardCorner`, `CharucoDetection`/`CharucoCorner`,
    and `PuzzleBoardDetection`/`PuzzleBoardCorner`/`PuzzleBoardDecodeInfo` now
    derive `Deserialize` in addition to `Serialize`, so a saved detection JSON
    round-trips back into its typed struct (marker results already did). Purely
    additive.
  - `CharucoDetectError::UnsupportedAlgorithm` is **removed** — the chessboard
    configuration validator accepts every configuration the public surface can
    express, so the variant was unreachable. ChArUco errors stringify at every
    binding boundary, so no binding surface or C header changed.
  - `#[serde(deny_unknown_fields)]` is now uniform across the params/config
    families: `CharucoParams`, `PuzzleBoardParams`, `PuzzleBoardDecodeConfig`,
    `MarkerBoardParams`, `CircleScoreParams`, `CircleMatchParams`, and every
    `advanced`-tuning block reject unknown keys (matching `ChessboardParams`,
    which already did). A config file with a stray/misspelled key now fails to
    load with a clear `unknown field` error instead of silently ignoring it;
    first-party producers (Python config layer, Studio, checked-in fixtures)
    already emit only valid keys. The aruco `ScanDecodeConfig` / `ArucoScanConfig`
    keep their existing tolerance, as shared aruco surfaces.

### Changed

- **The chessboard Stage 1 pre-filter admits axes on their reported
  uncertainty instead of a fit residual.** With `contrast` and `fit_rms` gone
  upstream, the rule `fit_rms ≤ max_fit_rms_ratio · contrast` is no longer
  expressible. It is replaced by

  ```text
  strength ≥ min_corner_strength   AND   max(σ₀, σ₁) ≤ axis_align_tol_rad
  ```

  where `σ` is the per-axis 1σ angular uncertainty the corner already carries
  and `axis_align_tol_rad` (default 15°) is the tolerance the topological
  builder's cell test *already* applies to those same axes. The gate is
  therefore **derived rather than fitted** — it introduces no new constant.
  The argument is dimensional: an axis estimate whose own spread is wider
  than the alignment window cannot answer the question the cell test poses of
  it, so admitting it adds noise to edge classification rather than evidence.
  Loosening the cell test now loosens admission by exactly the same amount.
  (The builder's internal `max_axis_sigma_rad` is a much looser backstop, not
  an admission gate, and is unrelated.)

  This is the only place in 0.11.0 where detection output moves. Across the
  regression corpus the change is recall-positive in aggregate and — the
  property that actually matters — **no frame gained a wrong `(i, j)`
  label**: the wrong-label, over-long-edge, and duplicate-pixel counters are
  unchanged. Dense, high-corner-count boards are where the two criteria
  disagree most and can end up with slightly fewer labelled corners than the
  fit-RMS rule produced; a pinned minimum-corner expectation on such a board
  is worth re-measuring.

### Security

- **Resolved four RustSec advisories.** `pyo3` 0.28 → 0.29 and `numpy` 0.28 →
  0.29 clear RUSTSEC-2026-0176 (out-of-bounds read in `PyList` / `PyTuple`
  iterator `nth`) and RUSTSEC-2026-0177 (missing `Sync` bound on
  `PyCFunction::new_closure`); lockfile updates to `crossbeam-epoch` 0.9.20,
  `anyhow` 1.0.104, and `memmap2` 0.9.11 clear RUSTSEC-2026-0204,
  RUSTSEC-2026-0190, and RUSTSEC-2026-0186 respectively. None required a
  source change, and detection output is unchanged.

- **Supply-chain policy is now enforced pre-merge.** A root `deny.toml`
  defines the advisory, licence, ban, and source policy, and a `supply-chain`
  CI job runs `cargo deny check` on every pull request — previously advisories
  were only caught by a weekly scheduled scan that filed an issue after the
  fact. `deny.toml` is the single source of truth for policy exceptions; each
  one is dated and states what would unblock its removal.

  One documented exception remains: RUSTSEC-2024-0436 (`paste`, unmaintained,
  informational, no patched release). It is unreachable from workspace code
  and arrived as `paste ← rav1e ← ravif ← image` via `image`'s default-on
  `avif` codec feature.

  **Now resolved.** Narrowing our own `image` features was not enough on its
  own, because `chess-corners` also depended on `image` without
  `default-features = false` and Cargo unifies features across the graph. With
  the upstream change shipped in `chess-corners` 1.1
  ([chess-corners-rs#68](https://github.com/VitalyVorobyev/chess-corners-rs/issues/68))
  plus `default-features = false` on our own `image` dependency, the AVIF
  *encoder* is out of the graph and `paste` is no longer compiled. No input
  format is lost: AVIF *decoding* is a separate `avif-native` feature that was
  never on by default, and nothing here encodes AVIF.

  Note `cargo audit` still reports the advisory. It scans `Cargo.lock`, which
  by design lists every *optional* dependency whether or not a feature enables
  it, so it cannot tell the edge is dead; `cargo tree -i paste` is empty.
  `cargo deny` resolves features first and reports what is actually built,
  which is why it — not `cargo audit` — is the CI gate.

### Fixed

- **Topological false-positive under strong barrel distortion.** The topological
  builder's final precision gate gained a fourth, second-order criterion —
  *frontier line-spacing smoothness*: a frontier (line-endpoint) corner whose
  edge overshoots the smooth spacing extrapolation of its own grid line is a
  false attachment past the true board edge and is dropped. This catches a wrong
  `(i, j)` label that is normal-length and on-axis (so the existing first-order
  overlong / off-axis / duplicate-pixel checks could not see it) without any
  ad-hoc edge-length constant. The criterion is scale-free and
  distortion-model-agnostic (radial and perspective) and runs inside the
  topological builder's final precision gate.
- **ChArUco decode determinism.** Deterministic tie-breaks in the marker
  alignment (`best_translation`) and multi-component merge (`merge_charuco_
  results`) — both previously resolved (weight, count) / marker-count ties by
  `HashMap` iteration order, so a borderline frame's alignment and corner IDs
  could flip run-to-run. Decode precision was never affected (zero
  self-consistency wrong-ids throughout); this fix only stabilises the
  tie-breaks in the decode path.

### Internal

- ArUco / ChArUco cell corner enumeration is de-duplicated through
  `cell_rect_corners_at`.

## 0.10.0

This release finalizes the public API surface ahead of a stable tag. The
breaking changes group into five themes: (1) public config / spec / result
types are `#[non_exhaustive]` with named constructors; (2) chessboard
diagnostics moved behind an opt-in `diagnostics` cargo feature and
`cell_size` returned to `ChessboardDetection`; (3) chessboard per-stage
tuning moved behind an opt-in, semver-exempt `advanced` block;
(4) language bindings were re-mirrored to match; and (5) the chessboard
graph-build algorithm `ChessboardV2` was renamed `SeedAndGrow`. Detection
behaviour and the default-config serialized JSON are unchanged. See the
[migration guide](docs/migrations/0.10.0.md) for before/after snippets
(Rust, JSON config, Python).

### Breaking

- **`GraphBuildAlgorithm::ChessboardV2` renamed to `SeedAndGrow`** — the
  chessboard grid-build algorithm now carries a method-descriptive name
  (wire string `chessboard_v2` → `seed_and_grow`; C ABI constant
  `CT_GRAPH_BUILD_ALGORITHM_CHESSBOARD_V2` → `..._SEED_AND_GROW`; WASM
  `GraphBuildAlgorithm` union now `"topological" | "seed_and_grow"`). This
  is a clean break with **no compatibility alias**: a config that explicitly
  sets the old `"chessboard_v2"` value now fails to parse and must be
  updated. `SeedAndGrow` is still the default, so configs that omit the key
  (the common case) are unaffected, and the `Topological` variant is
  unchanged.

- **Public API-surface hygiene: config / spec / report / result types are now
  `#[non_exhaustive]` with named constructors, and the soft-scorer / marker
  tuning knobs are documented-unstable.** This is a pure API-surface change —
  detection behaviour, tuning defaults, and every serialized JSON shape are
  unchanged (the public detection benchmark is byte-identical) — but it changes
  how a few public types are constructed from *other crates*.

  - **Newly `#[non_exhaustive]` (each gains a named constructor; reading code is
    unaffected, cross-crate literal construction must route through the
    constructor):**
    - `calib_targets_aruco`: `ScanDecodeConfig` (`default()` + `with_*`),
      `ArucoScanConfig` (`default()`), `Match` (`new`).
    - `calib_targets_marker`: `MarkerCircleSpec` (`new`), `MarkerBoardSpec`
      (`new` + `with_cell_size`), `MarkerBoardDetectConfig` (`new`),
      `MarkerBoardDetectReport` (`new`), `CircleMatch`
      (`unmatched` + `with_match`).
    - `calib_targets_charuco`: `CharucoBoardSpec` (`new` + `with_marker_layout`),
      `CharucoDetectConfig` (`new`), `CharucoDetectReport` (`new`),
      `CharucoAlignment` (`new`), `MarkerCornerLink` (`new`),
      `CharucoMarkerCornerLinks` (`new` + `with_mode`), `LinkViolation`
      (`new` + `with_*`).
    - `calib_targets_puzzleboard`: `PuzzleBoardSpec` (already had
      `new`/`with_origin`), `PuzzleBoardDetectConfig` (`new`),
      `PuzzleBoardDetectReport` (`new`).
    - `calib_targets_print`: `ChessboardTargetSpec` (`new`), `CharucoTargetSpec`
      (`new` + `with_marker_layout`/`with_border_bits`), `PuzzleBoardTargetSpec`
      (`new` + `with_origin`/`with_dot_diameter_rel`), `MarkerBoardTargetSpec`
      (`new` + `with_circle_diameter_rel`), `MarkerCircleSpec` (`new`), `PageSpec`
      (`default()` + `with_*`), `RenderOptions` (`default()` + `with_*`),
      `PrintableTargetDocument` (already had `new`; now `with_page`/`with_render`).
    - `projective_grid`: `TopologicalLabelTrace` (`new`), bringing it in line
      with its sibling topological-trace diagnostic types.

  - **Documented-unstable tuning knobs (no API move, doc-only):** the
    soft-log-likelihood / board-level-matcher knobs
    `bit_likelihood_slope`, `per_bit_floor`, `alignment_min_margin` (on
    `PuzzleBoardDecodeConfig` and `CharucoParams`), `cell_weight_border_threshold`
    (on `CharucoParams`), and the whole `calib_targets_marker::CircleScoreParams`
    struct are now flagged **NOT covered by semver** in rustdoc — consistent with
    the chessboard `AdvancedTuning` treatment. Leave them at their defaults
    unless tuning against a specific dataset with evidence.

  - **Language bindings (Python, WASM, FFI) are source-updated to construct the
    affected types through the new constructors.** Because no fields were added
    or renamed, the serialized JSON dict keys, the generated C header, and the
    Python typing stubs are all unchanged.

- **Chessboard diagnostics moved behind an opt-in `diagnostics` feature, and
  the hot `detect()` path no longer builds a `DebugFrame`.** The chessboard
  detector previously assembled the full per-stage `DebugFrame` introspection
  payload on every `detect()` / `detect_all()` call and then discarded it.
  That work is now skipped on the hot path, and the diagnostics surface is
  opt-in:

  - **`calib_targets_chessboard` gains a `diagnostics` cargo feature (OFF by
    default).** It gates the `diagnostics` module (`DebugFrame`,
    `IterationTrace`, `StageCounts`, the per-stage trace types,
    `DEBUG_FRAME_SCHEMA`) and the `Detector::detect_with_diagnostics` /
    `detect_all_with_diagnostics` entry points. Without the feature these
    names are absent from the public API. Enable `diagnostics` (or the
    `dataset` feature, which now implies it) to restore the full surface.

  - **`ChessboardDetection` gains a stable `cell_size: Option<f32>` field**
    (re-added as a permanent result field; populated on the normal `detect()`
    path with the seed-derived grid pitch). Construct via
    `ChessboardDetection::new(...)` + `with_cell_size(...)`. The type stays
    `#[non_exhaustive]`, so reading code is unaffected; code constructing it
    by literal across crates must route through the constructor. The field is
    mirrored across all three bindings: Python (`cell_size: float | None`),
    WASM (`cell_size: number | null`), and FFI — `ct_chessboard_result_t`
    gains a `cell_size: ct_optional_f32_t` field (`has_value == CT_TRUE`
    carries the pitch), an additive ABI change; regenerate against the
    updated C header.

  - **The `calib_targets` facade gains a matching `diagnostics` feature**
    (OFF by default) that forwards to `calib_targets_chessboard/diagnostics`
    and gates `detect_chessboard_with_diagnostics`.

  - **Behaviour on the `detect()` path is byte-identical**: the same labelled
    `ChessboardDetection` (now also carrying `cell_size`). The language
    bindings (Python, WASM, FFI) enable `diagnostics` unconditionally, so
    their diagnostic entry points are unchanged; the only generated C-header
    delta is the additive `cell_size` field noted above.

- **Chessboard tuning is now an opt-in, doc-unstable `advanced` surface.**
  The ~40 per-stage chessboard tuning knobs that previously lived flat on
  `calib_targets_chessboard::DetectorParams` (via the `ChessboardTuning`
  sub-struct, flattened into the wire format) have moved behind an opt-in,
  semver-exempt `advanced` block. This changes the public Rust API, the JSON
  wire format, and the language bindings:

  - **`ChessboardTuning` is renamed `AdvancedTuning`** and is re-exported from
    the chessboard crate root and the `calib_targets::chessboard` facade. It
    is documented but explicitly marked **unstable**: its fields are NOT
    covered by semver and may be renamed, retyped, or removed between minor
    versions. Build it from `AdvancedTuning::default()` and mutate the knobs
    you need (it is `#[non_exhaustive]`).

  - **`DetectorParams` now carries four stable fields**
    (`graph_build_algorithm`, `min_labeled_corners`, `max_components`,
    `min_corner_strength`) plus an opt-in `advanced: Option<Box<AdvancedTuning>>`.
    Attach advanced overrides with `DetectorParams::with_advanced(...)`; read
    the effective tuning (configured or default) with
    `DetectorParams::effective_tuning()`. With `advanced` unset, detection is
    byte-identical to the previous defaults.

  - **`min_corner_strength` was promoted to a stable top-level field.** Its
    serialized key stays top-level `"min_corner_strength"`, so that one key is
    wire-compatible with the previous flat layout. Setting it on a nested
    `params.chessboard` (ChArUco / PuzzleBoard / marker) keeps working.

  - **JSON / wire-format migration:** every other tuning knob now lives under
    a nested `"advanced"` object instead of at the top level. Old flat configs
    that set advanced knobs at the top level will silently fall back to the
    defaults for those knobs (serde ignores unknown top-level keys). Move the
    knobs into an `"advanced": { ... }` block to carry them forward. The
    nested block is omitted entirely when no advanced tuning is set.

  - **Removed the unused `projective_line_tol_rel` advanced knob from the
    Python `ChessboardParams`.** The field was serialized into the advanced
    block but never read by the Rust detector, so it was a no-op; removing it
    has no effect on detection. It lived in the opt-in, non-semver `advanced`
    surface. Drop the keyword from any `ChessboardParams(...)` call that set
    it; serialized configs that still carry the key continue to deserialize
    (the extra key is ignored).

  - **Bindings:** the FFI `ct_chessboard_params_t` keeps the stable fields
    directly and gates the advanced knobs behind a `has_advanced` flag plus a
    nested `ct_chessboard_advanced_t` (regenerate against the updated header).
    The Python `ChessboardParams.to_dict()` / `from_dict()` and the WASM /
    TypeScript types now use the nested `advanced` shape. No new Cargo feature
    is introduced — the opt-in is purely the API shape plus the unstable-doc
    marking.

## Older releases

The full release history is preserved under
[`docs/changelog/`](docs/changelog/), grouped by minor-version family:

- [`0.9.x`](docs/changelog/0.9.x.md) — TODO
- [`0.8.x`](docs/changelog/0.8.x.md) — TODO
- [`0.7.x`](docs/changelog/0.7.x.md) — TODO
- [`0.6.x`](docs/changelog/0.6.x.md) — PuzzleBoard crate launch
- [`0.5.x`](docs/changelog/0.5.x.md) — single-config detector API,
  multi-component ChArUco, WebAssembly bindings
- [`0.4.x`](docs/changelog/0.4.x.md) — standalone `projective-grid`
  crate, hex grids, native C API hardening
- [`0.3.x`](docs/changelog/0.3.x.md) — printable-target tooling,
  C ABI / FFI crate, ChArUco recall improvements
- [`0.2.x`](docs/changelog/0.2.x.md) — Python bindings refresh,
  ChArUco false-corner fix
- [`0.1.x`](docs/changelog/0.1.x.md) — initial public releases
