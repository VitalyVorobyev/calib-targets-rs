# Changelog

All notable changes to this project will be documented in this file.

This project follows [Semantic Versioning](https://semver.org/).

Older releases are archived under [`docs/changelog/`](docs/changelog/);
see [Older releases](#older-releases) at the bottom for the index.

## 0.9.0

Migrates the workspace onto `chess-corners` 0.10 (skipping the
intermediate 0.9 release in published artifacts), absorbs the
0.9-era in-tree algorithm work (`OrientationMethod`, `DiskFit`,
Stage 6.25 / 6.5b, post-Stage-6 axis-slot coherence fix), and
tightens binding parity around the new tagged-enum detector
configuration. See
[`docs/chess-corners-0.10-impact.md`](docs/chess-corners-0.10-impact.md)
for the full bench matrix and the strict-dominance check that
keeps the workspace default on `RingFit`.

### Breaking

- **Removed four dead `ChessboardTuning` knobs** that were serialized
  and FFI-round-tripped but never read by any detector logic:
  `enable_line_extrapolation`, `enable_gap_fill`, `enable_component_merge`,
  and `component_merge_min_boundary_pairs`. Component merge now runs
  unconditionally via `DetectorParams::component_merge: LocalMergeParams`.
  Old JSON containing these keys still deserializes without error (serde
  ignores unknown fields); C ABI callers must remove the four fields from
  `ct_chessboard_params_t` and regenerate against the updated FFI header.

- **Removed the obsolete `diagonal_angle_tol_rad` topological knob.**
  `TopologicalParams::diagonal_angle_tol_rad` and the
  `diagonal_distance_rad` / `diagonal_margin_rad` fields on
  `TopologicalEdgeMetricTrace` have been deleted (mirrored in the
  Python `TopologicalParams` config and the `--diagonal-angle-tol-deg`
  tooling flags / manifest keys). Those fields had no effect on
  classification: diagonals are inferred per triangle — exactly two
  grid edges meeting at a vertex with different local axis slots
  promote the remaining edge to `Diagonal`. Mental model going
  forward: tune `axis_align_tol_rad` for grid-edge admission; diagonal
  inference has no separate angle threshold.

- **C ABI (`calib-targets-ffi` 0.8 → 1.0): all five detect entry points
  redesigned around `args` / `buffers` struct pairs.** Each positional-
  argument signature has been replaced by
  `(const ct_*_detect_args_t *args, ct_*_detect_buffers_t *bufs)`.
  The `args` struct bundles the detector handle and image pointer; the
  `buffers` struct bundles each output array pointer with its capacity and
  required-length out-pointer. The "NULL buffer + capacity 0 queries the
  required length" behaviour is preserved per-buffer. The five redesigned
  entry points are:
  - `ct_chessboard_detector_detect_all` (8 positional args → 2)
  - `ct_charuco_detector_detect` (9 positional args → 2)
  - `ct_marker_board_detector_detect` (12 positional args → 2)
  - `ct_chessboard_detector_detect` (6 positional args → 2)
  - `ct_puzzleboard_detector_detect` (6 positional args → 2)

  The FFI crate is bumped to `1.0.0` to mark the ABI break; downstream
  C/C++ callers must regenerate against the new header in
  `crates/calib-targets-ffi/include/calib_targets_ffi.h`.

- **`chess-corners` 0.8 → 0.10.** The upstream
  `ChessConfig` has been split into a tagged-enum tree:
  - top-level `DetectorConfig { strategy, threshold, multiscale, upscale, orientation_method, merge_radius }`;
  - `Threshold::Absolute(f32) | Relative(f32)` replaces the previous
    `(threshold_mode, threshold_value)` pair;
  - `MultiscaleConfig::SingleScale | Pyramid { levels, min_size, refinement_radius }`
    replaces flat `pyramid_levels` / `pyramid_min_size` / `refinement_radius`;
  - `UpscaleConfig::Disabled | Fixed(u32)` replaces the previous
    flat upscale fields;
  - `DescriptorRing::FollowDetector | Canonical | Broad` replaces the
    boolean `descriptor_use_radius10` hint;
  - `ChessRefiner::CenterOfMass(_) | Forstner(_) | SaddlePoint(_) | Ml`
    replaces the discriminator + parallel-tuning-struct shape;
  - `DetectionStrategy::Chess(ChessConfig) | Radon(RadonConfig)` makes
    detector dispatch type-checked.
- **`find_chess_corners_image` is gone.** Replace with
  `Detector::new(cfg)?.detect(&img)?` (or `detect_u8`). The
  workspace facade still routes through
  `calib_targets::detect::detect_corners(&img, &cfg, pre_blur_sigma_px)`,
  which now constructs the `Detector` internally.
- **Python `ChessConfig` rewritten end-to-end.** The dataclass
  ships the tagged-enum tree (`Threshold`, `MultiscaleConfig`,
  `UpscaleConfig`, `ChessRefiner`, `DetectionStrategy`,
  `ChessStrategyConfig`) and its `to_dict()` emits the exact JSON
  shape that `serde_json::to_value(DetectorConfig)` produces on the
  Rust side. Callers that pre-built dicts with `threshold_value` /
  `threshold_mode` / `pyramid_levels` get a clear `ValueError`
  pointing at the migration; legacy keyword construction of the
  inner `RefinerConfig(kind="forstner")` keeps working via a thin
  shim that forwards to `ChessRefiner.forstner()`.
- Markdown documentation refreshed (workspace README, Python
  README, chessboard / puzzleboard / WASM READMEs, book
  troubleshooting chapter) for the new threshold and config
  spellings.

#### Public-API surface revision

`0.9.0` also carries a deliberate, workspace-wide revision of the
public API. Successive debugging passes had made nearly every
pipeline stage, intermediate state, trace struct, and tuning knob
`pub`; the revision sorts that surface into **three channels** —
*stable results* (the facts a calibration consumer needs), *opt-in
diagnostics* (evidence about *how* a detection was reached, behind
named `diagnostics` / `trace` modules with a looser stability
promise), and *private internals* (pipeline scaffolding, now
`pub(crate)` or private `mod`). Most of the entries below are
breaking only for code that reached into a detector's internals;
the consumer-facing changes have a before→after below.

**Modules and stage items made private.** A large block of
implementation detail left the public surface:

- `calib-targets-chessboard` — all 13 root modules and 7 `pipeline`
  submodules are now private `mod`; the ~30 stage functions / types
  that sat in the crate prelude (`cluster_axes`, `find_seed`,
  `grow_from_seed`, `apply_boosters`, `estimate_cell_size`,
  `validate`, `run_pipeline`, `Seed`, `GrowResult`, `ClusterCenters`,
  `CornerAug`, …) are no longer re-exported. The crate root now
  exposes only the curated contract.
- `calib-targets-marker` — all 6 modules (`circle_score`, `coords`,
  `detect`, `io`, `match_circles`, `types`) are now private `mod`;
  the superseded `estimate_grid_offset` free function was removed
  (use `estimate_grid_alignment`).
- `projective-grid` — the `square::grow_extension` and
  `square::seed_finder` compatibility-alias modules were removed;
  import from `square::extension` and `square::seed::finder`
  instead. `square::cleanup` is now a private `mod`.
- `calib-targets-aruco` — `Dictionary` fields and the generated
  `DICT_*_CODES: &[u64]` statics are no longer public raw storage.
  Use `Dictionary::from_static_codes(...)` for custom dictionaries and
  the `name()`, `marker_size()`, `max_correction_bits()`, and `codes()`
  accessors for reads.
- `calib-targets` — the `cli` module is now `#[doc(hidden)]`; it
  was never intended as library API.

**Diagnostics relocated into named channels.** Each detector now
has one consistent diagnostic channel — a `diagnostics` (or
`trace`) module plus a `*_with_diagnostics` method — instead of
trace types on the crate root and evidence fields inlined into
result structs:

- `projective-grid` — the topological trace types
  (`TopologicalTrace`, `TopologicalComponentTrace`,
  `TopologicalEdgeMetricTrace`, … and `build_grid_topological_trace`)
  are no longer re-exported at the crate root. Reach them under
  `projective_grid::topological::trace`.
- `calib-targets-chessboard` — new `diagnostics` module; the
  `DebugFrame`, per-stage `*Trace`, `StageCounts`, `ClusterDebug`,
  `CornerStage`, and `DEBUG_FRAME_SCHEMA` types moved into it.
  `Detector` went from 6 detect methods to 4: `detect`,
  `detect_all`, `detect_with_diagnostics`,
  `detect_all_with_diagnostics`. The `detect_debug` /
  `detect_instrumented` / `detect_all_debug` /
  `detect_all_instrumented` methods and the `InstrumentedResult`
  type were removed — `detect_with_diagnostics` returns a
  `DebugFrame`, and `StageCounts::from_frame` derives the compact
  counters that `detect_instrumented` used to bundle.
- `calib-targets-charuco` — new `diagnostics` module; the 9
  `*Diag*` types (`BoardMatchDiagnostics`, `CellDiag`,
  `CharucoDetectDiagnostics`, `ComponentDiagnostics`,
  `DiagHypothesis`, `RejectReason`, …) moved into it.
- `calib-targets-puzzleboard` — new `diagnostics` module plus a
  `PuzzleBoardDiagnostics` struct and a
  `PuzzleBoardDetector::detect_with_diagnostics` method.
- `calib-targets-marker` — new `diagnostics` module plus a
  `MarkerBoardDiagnostics` struct and `detect_*_with_diagnostics`
  methods.

**Result structs slimmed to facts; diagnostics moved out.** The
four detector result types had grown to mix facts and evidence;
the evidence now lives in the matching `*Diagnostics` struct,
reached via `detect_with_diagnostics`:

- *chessboard* — the result type was restructured. Before, a
  `Detection` wrapped the generic `core::TargetDetection` and
  carried `grid_directions`, `cell_size`, and a `strong_indices`
  vec parallel to `target.corners`:

  ```rust
  // before
  let det = detect_chessboard(&img, &params)?;        // Option<chessboard::Detection>
  for c in &det.target.corners {                      // two-level nesting
      let (i, j) = { let g = c.grid.unwrap(); (g.i, g.j) };  // grid is Option, never None here
      let src = det.strong_indices[/* index in lockstep */]; // parallel vector
  }

  // after
  let det = detect_chessboard(&img, &chess_cfg, &params)?;   // Option<ChessboardDetection>
  for c in &det.corners {                             // one level
      let (i, j) = (c.grid.i, c.grid.j);              // grid is non-optional
      let src = c.input_index;                        // provenance on the corner
  }
  ```

  `Detection` is now `ChessboardDetection { corners: Vec<ChessboardCorner> }`,
  and `ChessboardCorner { position, grid: GridCoords, input_index, score }`
  carries a **non-optional** `grid` (a chessboard corner is always
  labelled). Dropped from the result: `grid_directions` (ill-defined
  once the board is projectively warped), `cell_size` (a scale prior,
  not a measurement — derive scale from corner spacing), the
  `target` / `TargetDetection` wrapper, and the parallel
  `strong_indices` vec (now per-corner `input_index`).
  `rectify_from_chessboard_result` now takes `&ChessboardDetection`.
- *charuco* — `CharucoDetectionResult` now exposes
  `corners: Vec<CharucoCorner>` directly instead of wrapping the
  corners in a generic `TargetDetection`; `target_detection()` remains
  available when callers need the shared carrier. `raw_marker_count`
  and `raw_marker_wrong_id_count` moved into
  `CharucoDetectDiagnostics`; reach them via
  `CharucoDetector::detect_with_diagnostics`.
- *puzzleboard* — `PuzzleBoardDetectionResult` now exposes
  `corners: Vec<PuzzleBoardCorner>` directly. `observed_edges` moved
  off the result, and the decode `score_best` / `score_runner_up` /
  `score_margin` / `runner_up_*` / `scoring_mode` fields moved off
  `PuzzleBoardDecodeInfo`, into `PuzzleBoardDiagnostics`.
  `PuzzleBoardDecodeInfo` keeps a compact quality summary
  (`edges_observed` / `edges_matched`, `mean_confidence`,
  `bit_error_rate`, `master_origin_*`).
- *marker* — `MarkerBoardDetectionResult` now exposes
  `corners: Vec<MarkerBoardCorner>` directly plus `alignment`.
  `inliers`, `circle_candidates`, `circle_matches`, and
  `alignment_inliers` moved into `MarkerBoardDiagnostics`.

**`#[non_exhaustive]` + constructors on result / data carriers.**
`TargetDetection`, `LabeledCorner`, `CharucoDetectionResult`,
`CharucoCorner`, `PuzzleBoardDetectionResult`, `PuzzleBoardCorner`,
`MarkerBoardDetectionResult`, `MarkerBoardCorner`,
`ChessboardDetection`, and `ChessboardCorner` are now
`#[non_exhaustive]`. External code can no longer build them with a
struct literal; use the new `new(...)` constructors (and, for
`LabeledCorner`, the `with_grid` / `with_id` /
`with_target_position` setters). Field reads are unaffected.

**chessboard `DetectorParams` split into a stable core + advanced
tuning.** `DetectorParams` had grown to ~50 fields, most named
after internal pipeline stages. It now has a 3-field stable core —
`graph_build_algorithm`, `min_labeled_corners`, `max_components` —
plus a `tuning: ChessboardTuning` sub-struct holding the ~42
stage-tuning knobs. The dead `cell_size_hint` field was removed.

```rust
// before
let mut p = DetectorParams::default();
p.cluster_tol_deg = 9.0;

// after
let mut p = DetectorParams::default();
p.tuning.cluster_tol_deg = 9.0;
```

`ChessboardTuning` is `#[serde(flatten)]`-ed, so **the serialized
JSON / config wire format stays flat** — existing config files
deserialize unchanged; only Rust struct-field access moves to
`params.tuning.<knob>`.

**facade `detect_chessboard*` consolidated, 8 functions → 5.**
Every chessboard entry point now takes the ChESS `DetectorConfig`
as an explicit parameter, so the `*_with_config` variants are
gone; callers that do not tune ChESS pass `&default_chess_config()`.
`detect_chessboard_debug` / `detect_chessboard_debug_with_config`
are now the single `detect_chessboard_with_diagnostics`.

```rust
// before
let det = detect_chessboard(&img, &params)?;

// after
let det = detect_chessboard(&img, &default_chess_config(), &params)?;
```

The five surviving entry points are `detect_chessboard`,
`detect_chessboard_all`, `detect_chessboard_best`,
`detect_chessboard_from_gray_u8`, and
`detect_chessboard_with_diagnostics`.

**`chess-corners` re-exports trimmed.** `calib-targets-core` and
the `calib-targets` facade now re-export only `DetectorConfig` and
`OrientationMethod` from `chess-corners` — re-exporting the whole
upstream surface would freeze `chess-corners`'s API into this
workspace's semver contract. Code that named advanced ChESS tuning
types (`ChessConfig`, `RadonConfig`, `Threshold`, `RefinerKind`,
`MultiscaleConfig`, …) through `calib_targets::*` or
`calib_targets_core::*` should import them from the `chess-corners`
crate directly.

**`#![deny(missing_docs)]` on every publishable library crate.**
Every public item in the nine publishable crates now carries a doc
comment; this is enforced at compile time.

**Bindings re-mirrored the slimmed shapes.** The FFI, Python, and
WASM bindings track the revised surface:

- *FFI* — the C ABI changed: `ct_*_result_t` structs were slimmed
  to match the Rust results (`ct_chessboard_result_t` now carries
  only `corners_len`, with corners copied into a caller-provided
  `ct_chessboard_corner_t` array; the marker-board circle output
  buffers and `ct_circle_candidate_t` / `ct_circle_match_t` were
  removed — the marker diagnostics channel has no FFI binding);
  `cell_size_hint` was dropped. Regenerate against the updated
  header in
  `crates/calib-targets-ffi/include/calib_targets_ffi.h`.
- *Python* — the result dataclasses were slimmed to match the new
  Rust result dicts: target-specific results expose `corners`
  directly, and diagnostic fields were removed from typed results.
- *WASM* — the serialized result shapes were slimmed: target-specific
  results expose `corners` directly, diagnostic fields moved out, and
  `detect_chessboard_best` gained a `chess_cfg` argument so the
  caller can choose the ChESS config used for corner detection
  across the sweep.

**Binding diagnostics access restored.** The API revision moved each
detector's diagnostics into a Rust `diagnostics` / `trace` module and
`*_with_diagnostics` methods, but left the three binding crates
without programmatic access to that evidence. The bindings now expose
the diagnostics channel for every detector:

- *FFI* — additive JSON-string accessors
  `ct_chessboard_detector_detect_diagnostics_json`,
  `ct_charuco_detector_detect_diagnostics_json`,
  `ct_marker_board_detector_detect_diagnostics_json`, and
  `ct_puzzleboard_detector_detect_diagnostics_json`. Each runs
  detection and writes a NUL-terminated UTF-8 JSON string of the
  diagnostics struct into a caller-owned buffer, reusing the
  `ct_last_error_message` query/fill discipline (NULL + capacity 0
  queries the required length). The deeply-nested diagnostics trees
  carry a looser stability promise than the typed result API, so a
  JSON string is the right encoding rather than frozen C structs.
  The typed `*_detect` entry points are unchanged.
- *Python* — new `detect_charuco_with_diagnostics`,
  `detect_marker_board_with_diagnostics`, and
  `detect_puzzleboard_with_diagnostics` return a
  `{"result": ..., "diagnostics": ...}` dict (chessboard diagnostics
  remain reachable via the existing `detect_chessboard_debug`).
- *WASM* — new `detect_chessboard_with_diagnostics`,
  `detect_charuco_with_diagnostics`,
  `detect_marker_board_with_diagnostics`, and
  `detect_puzzleboard_with_diagnostics` return a
  `{ result, diagnostics }` object; the diagnostics object shapes are
  declared in `typescript-extras.d.ts`.

- **Printable-target bundle API restructured around `StemPaths` to add a
  fourth (DXF) output.** `calib_targets_print::stem_paths()` now returns
  a `StemPaths` struct (`#[non_exhaustive]` + `StemPaths::from_stem`
  constructor) carrying `json` / `svg` / `png` / `dxf` paths, replacing
  the previous `(json, svg, png)` tuple. `GeneratedTargetBundle` and
  `WrittenTargetBundle` switch to `#[non_exhaustive]` with named
  constructors and gain `dxf_text: String` / `dxf_path: PathBuf` fields
  respectively. The Python `GeneratedTargetBundle` and
  `WrittenTargetBundle` dataclasses mirror the new fields, and both the
  Rust and Python CLIs now print the fourth (DXF) path alongside the
  existing three.

### Added

- `crates/calib-targets-py/python_tests/test_chess_config_rust_roundtrip.py`
  — end-to-end coverage that the Python `ChessConfig.to_dict()`
  payload deserializes cleanly through Rust's `serde_json`
  layer on a real test image. Guards against the silent
  dict-shape drift that fixtures alone cannot catch.

- **DXF as a fourth printable-target output, scoped to chrome-on-glass
  photolithography handoff.** Producers' CAM stacks need a vector DXF
  with exact mm geometry, which neither the rasterized PNG nor the
  SVG satisfies. The writer
  (`crates/calib-targets-print/src/render_dxf.rs`) is AutoCAD R2000
  (AC1015) ASCII, hand-rolled (no external `dxf` dependency),
  `$INSUNITS = 4` (mm), 6-decimal coordinates (1 nm precision), Y-up
  cartesian with origin at the page bottom-left (matches LibreCAD /
  FreeCAD / CAM350; SVG's Y-down would mirror the board), single
  `PATTERN` layer carrying only `Fill::Black` regions (the chrome
  side; debug annotations never reach the DXF even when
  `render.debug_annotations` is on, since the DXF is rendered from
  the pre-debug scene snapshot), closed `LWPOLYLINE` for rectangles
  and native `CIRCLE` for circles (no polygonal approximation).
  Covered by a checked-in golden snapshot
  (`crates/calib-targets-print/tests/golden/charuco_3x3_dict4x4_50.dxf`)
  plus unit tests for Y-flip, polarity filter, header, and entity
  counts, and by Rust + Python CLI integration tests that assert the
  `.dxf` is written with `AC1015` + mm units. FFI and WASM are
  intentionally untouched (neither exposes printable-target
  generation today).

- `crates/calib-targets-py/examples/generate_charuco_26x26_4x4_1000.py`
  — sample script that emits a 26×26 `DICT_4X4_1000` ChArUco at
  1.5 mm cell size, the kind of fine-pitch board that photolith is
  the right process for.

### Changed

- Bumped `chess-corners` `0.10 → 0.11`. The 0.11 release is a
  re-export reorganization with no algorithm or signature change:
  low-level pipeline stages, parameter structs, and scratch buffers
  (`ChessParams`, `RefinerKind`, `Refiner`, `Roi`, `ImageView`,
  `PyramidParams`, `chess_response_u8_patch`,
  `detect_corners_from_response_with_refiner`) moved from the crate
  root into the new `chess_corners::low_level` module. Workspace
  imports were updated accordingly; no user-facing behaviour change.

### Notes

- The 0.9-era algorithm work (`OrientationMethod` plumbing,
  `DiskFit`, post-Stage-6 axis-slot coherence) was developed
  in-tree on this branch alongside the 0.10 API migration; the
  bench matrix is in `docs/chess-corners-0.10-impact.md`. The
  workspace default stays on `RingFit` (strict-dominance rule
  did not trigger).





## Older releases

The full release history is preserved under
[`docs/changelog/`](docs/changelog/), grouped by minor-version family:

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
