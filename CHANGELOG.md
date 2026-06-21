# Changelog

All notable changes to this project will be documented in this file.

This project follows [Semantic Versioning](https://semver.org/).

Older releases are archived under [`docs/changelog/`](docs/changelog/);
see [Older releases](#older-releases) at the bottom for the index.

## Unreleased

This cycle continues the projective-grid generalization and a batched
public-surface cleanup. The workspace is still `0.x`, so breaking changes are
expected. Detection behaviour on the public benchmark is byte-identical.

### Added

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

- **`calib_targets_chessboard::Detector::new` is now fallible**
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

- **ChArUco and PuzzleBoard diagnostics moved behind an opt-in `diagnostics`
  cargo feature** (default off), matching `calib-targets-chessboard`. The
  `diagnostics` module, the diagnostics type re-exports, and
  `detect_with_diagnostics` reach the public surface only with the feature
  enabled; the facade `calib-targets/diagnostics` feature now forwards to all
  three detector crates.

- **`DetectorParams.min_labeled_corners` / `max_components` are now defaulted on
  deserialization** (`8` / `3`), so partial and legacy configs that omit them
  deserialize again. Values and serialization are unchanged.

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

- The ChArUco legacy-vote alignment's dominant-rotation-only D4 selection is
  recorded as a tracked gap (the default board-level matcher already enumerates
  all rotations); the stale alignment TODO is removed. ArUco / ChArUco cell
  corner enumeration is de-duplicated through `cell_rect_corners_at`.

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
