# Changelog

All notable changes to this project will be documented in this file.

This project follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

### API redesign

- **Breaking:** `ChessConfig` is now embedded inside each detector's params struct.
  Facade `detect_*` functions take a single `&Params` argument instead of
  separate `(&ChessConfig, Params)`. Removed `detect_charuco_default` and
  `detect_marker_board_default`.
- **Breaking:** `CharucoDetectorParams` renamed to `CharucoParams`.
- **Breaking:** `CharucoParams.charuco` field renamed to `.board`.
- **Breaking:** `MarkerBoardLayout` renamed to `MarkerBoardSpec`.
- **Breaking:** `GridCell` replaced with `GridCoords` in aruco crate.
  `BoardCell` removed.
- Add multi-config sweep API: `detect_chessboard_best`, `detect_charuco_best`,
  `detect_marker_board_best` try multiple parameter configs and return the best
  result (most markers, then most corners).
- Add `CharucoParams::sweep_for_board()` and `ChessboardParams::sweep_default()`
  presets for common multi-threshold sweep scenarios.
- Extract shared `calib_targets_core::io::{load_json, write_json, IoError}` to
  replace duplicated IO boilerplate across crates.
- Python and WASM bindings accept the new single-config API. The `chess_cfg`
  parameter is still accepted for backward compatibility (overrides
  `params.chess` or `params.chessboard.chess` when provided).
- Python: `CharucoParams` and `MarkerBoardSpec` are the canonical names;
  `CharucoDetectorParams` and `MarkerBoardLayout` remain as aliases.

### Multi-component ChArUco detection

- Merge disconnected grid components for 30-50% more corners on challenging
  images (Scheimpflug optics, narrow focus strips). Each component is aligned
  independently via marker-based D4 rotation, then merged.

### AprilTag max_hamming fix

- `CharucoParams::for_board()` now sets `max_hamming` to
  `min(2, dictionary.max_correction_bits)` instead of 0, improving recall for
  AprilTag-based ChArUco boards (e.g. `DICT_APRILTAG_36h10`).

### WebAssembly bindings and browser demo

- Add the new `calib-targets-wasm` crate (`crates/calib-targets-wasm/`) with
  `wasm-bindgen` exports for all detection pipelines: `detect_corners`,
  `detect_chessboard`, `detect_charuco`, and `detect_marker_board`. The crate
  depends directly on the detector crates and `chess-corners` (without `rayon`
  or `ml-refiner`) so it compiles cleanly for `wasm32-unknown-unknown`.
- Expose `rgba_to_gray` for browser canvas RGBA-to-grayscale conversion and
  `default_chess_config` / `default_chessboard_params` /
  `default_marker_board_params` helpers for populating UI defaults from Rust.
- Config and result objects are passed as plain JS objects via
  `serde-wasm-bindgen` (no JSON string round-trips).
- WASM binary: ~436 KB raw, ~195 KB gzipped.
- Add a React/TypeScript demo app at `demo/` (Vite 6, React 19) with:
  image upload (drag-and-drop), detection mode selector (Corners / Chessboard /
  ChArUco / Marker Board), interactive parameter sliders, canvas overlay with
  colored corners and grid edges, and a results panel with timing and JSON view.
- Add `wasm` CI job to `.github/workflows/ci.yml`: builds WASM with
  `wasm-pack`, verifies output artifacts, and builds the demo app with
  TypeScript checking.
- Add `scripts/build-wasm.sh` helper to build WASM into `demo/pkg/`.
- Add `default-members` to the root workspace manifest so `cargo test` excludes
  the WASM crate by default.

### Python bindings API refactoring

- Flatten `ChessConfig` in Python: remove nested `ChessCornerParams`,
  `CoarseToFineParams`, `PyramidParams`; all fields are now top-level with
  concrete defaults. Add `RefinerConfig`, `CenterOfMassConfig`,
  `ForstnerConfig`, `SaddlePointConfig`.
- Fold `GridGraphParams` into `ChessboardParams` as `chessboard.graph` across
  all Rust crates, Python bindings, FFI, and JSON configs.
- Add `ChessboardDetectConfig` / `ChessboardDetectReport` and
  `MarkerBoardDetectConfig` / `MarkerBoardDetectReport` for JSON-driven
  detection workflows.
- Rewrite `calib-targets-py/src/lib.rs` from ~3600 lines to ~290 lines using a
  dict-based JSON bridge (Python dataclass `to_dict()` -> `serde_json` ->
  Rust type). Remove all `*Source` enums, `*Overrides` structs, and manual
  extraction functions.

## [0.4.2]

### Release engineering

- Technical release: bump coordinated crate versions to `0.4.2` after
  publish-workflow fixes.

## [0.4.1]

### Release engineering

- Technical release: bump coordinated crate versions to `0.4.1` to fix
  publication issues.

## [0.4.0]

### Standalone `projective-grid` crate

- Add the new publishable [`projective-grid`](https://crates.io/crates/projective-grid)
  crate for pattern-agnostic 2D grid tooling: pluggable `NeighborValidator`
  traits, grid graph construction, connected-component traversal, BFS grid
  coordinate assignment, homography estimation, global rectification, per-cell
  mesh rectification, and grid smoothness prediction.
- Extract the generic square-grid geometry and homography machinery from
  `calib-targets-core` into `projective-grid`. `calib-targets-core` keeps the
  image-space pieces (`GrayImage*`, sampling, `warp_perspective_gray`) and
  re-exports `Homography`, `GridCoords` (`GridIndex` alias), `GridAlignment`,
  `GridTransform`, and homography-estimation helpers for downstream
  compatibility.
- Refactor `calib-targets-chessboard` to delegate grid-graph construction and
  traversal to `projective-grid`, while keeping chessboard-specific neighbor
  validation in-crate. Switch ChArUco grid smoothness to the shared
  `projective_grid::predict_grid_position` helper instead of maintaining a
  separate midpoint-prediction implementation.

### Hex grids and built-in validators

- Add `projective_grid::hex` with pointy-top axial-coordinate support for
  6-connected graph construction, BFS coordinate assignment, grid smoothness
  prediction, `D6` alignment transforms, `HexGridHomography`, and
  `HexGridHomographyMesh` for per-triangle affine/projective rectification.
- Add ready-to-use validator implementations in
  `projective_grid::validators`:
  `XJunctionValidator` for ChESS-like oriented square-grid corners,
  `SpatialSquareValidator` for unoriented square lattices, and
  `SpatialHexValidator` for unoriented hex lattices such as ringgrids.

### Native C API and bindings

- Expose `ScanDecodeConfig::multi_threshold` in the FFI as
  `ct_scan_decode_config_t::multi_threshold` so native callers can control the
  multi-threshold marker decode path instead of being forced to the Rust
  default.
- Add native test coverage that verifies `ct_scan_decode_config_t` preserves
  the `multi_threshold` flag when converting into the Rust
  `ScanDecodeConfig`.
- Make the Python typing-artifact generator robust to multiline
  `#[pyclass(...)]` attributes so generated `_core.pyi` stubs stay in sync
  after adding `skip_from_py_object` to config-heavy binding classes.

### Workspace and release engineering

- Centralize shared crate metadata and dependency versions in the workspace
  root via `[workspace.package]` and `[workspace.dependencies]` so the Rust
  crates inherit coordinated `0.4` versioning and one dependency set.
- Raise the documented MSRV to Rust `1.88` and surface it in the workspace
  metadata and top-level README badge.
- Update docs and packaging references from `0.3` to `0.4`, including the
  getting-started dependency snippets and the coordinated Rust/Python/native
  release metadata.
- Include `projective-grid` in the coordinated crates.io release flow and add
  CI validation that the publish order matches inter-crate dependencies before
  attempting the tagged publish job.

## [0.3.2]

### ChArUco — local grid smoothness pre-filter

- **New `grid_smoothness` module** in `calib-targets-charuco`: runs between
  `build_corner_map` and `build_marker_cells` to detect corners whose pixel
  position is inconsistent with their grid neighbors (midpoint prediction).
  This catches false corners from ArUco marker internal features picked up by
  ChESS under a loose orientation tolerance (e.g. 22.5°).  Flagged corners are
  re-detected locally via `redetect_corner_in_roi`; if re-detection fails, the
  corner is snapped to the predicted position (never removed) so that marker
  cell completeness — and thus marker detection recall — is preserved.
- **New `grid_smoothness_threshold_rel` parameter** on
  `CharucoDetectorParams` (default `0.05`, i.e. 3 px at 60 px/sq).
  Set to `f32::INFINITY` to disable.  Also exposed in the FFI
  (`ct_charuco_detector_params_t`) with the same default.
- Promote `redetect_corner_in_roi` from private to `pub(crate)` in
  `corner_validation.rs` so the grid smoothness module can reuse it.

## [0.3.1]

### Chessboard grid graph — perspective-invariant neighbor direction fix

- **Fix direction symmetry in `is_good_neighbor_with_orientation`**: the old
  code indexed diagonal directions by the source/neighbor cluster index
  (`grid_diagonals[ci]` / `grid_diagonals[cj]`), so the sign of `v_minus = oi
  - oj` depended on which corner was the "source" and which was the "neighbor".
  This broke the A→B Right ↔ B→A Left invariant the BFS relies on, causing
  spurious disconnected components and missing grid edges on rotated or
  perspective-distorted boards.
- **Canonical reference frame**: switch to `grid_diagonals[0]` and
  `grid_diagonals[1]` (independent of edge direction) so all edges in the graph
  share the same `v_plus`/`v_minus` axes. Canonicalize the sign of `v_minus`
  via the cross-product determinant so that `(v_minus, v_plus)` always form a
  right-handed frame in image coordinates, regardless of the arbitrary order
  produced by orientation clustering.
- **Perspective-invariant direction classification**: replace the image-space
  `direction_quadrant` heuristic (which broke for rotated boards) with signed
  dot products against the local grid axes. The resulting Right/Left/Down/Up
  labels are now consistent with the local grid geometry under perspective, not
  just when the board is nearly axis-aligned.
- Add a `rotated_grid_forms_single_component` unit test that constructs a 4×4
  corner grid at an arbitrary 40° rotation and verifies the graph BFS produces
  a single connected component with correct grid coordinates.

### ChArUco marker detection — improved recall on blurry images

- **Multi-threshold binarization**: the marker decode step now tries multiple
  binarization thresholds per cell (Otsu, Otsu±10, Otsu±15, two percentile
  thresholds, and a border-guided midpoint) and selects the one that yields a
  valid dictionary match with hamming=0. This recovers markers that were
  previously lost on blurry or unevenly-lit images because a single Otsu
  threshold flipped one or two border or payload bits. Controlled by the new
  `ScanDecodeConfig::multi_threshold` field (default `true`); exposed as
  `ArucoScanConfig::multi_threshold` for JSON-level overrides.
- **Lower default `min_border_score` for ChArUco**: the per-cell border-black
  ratio threshold is now `0.75` (was `0.85`) in `CharucoDetectorParams::for_board`.
  The downstream alignment and corner-validation stages already act as
  false-positive guards, so the looser scan-stage bar improves recall without
  introducing spurious detections.
- Add `percentile_threshold`, `border_guided_threshold`, and
  `compute_threshold_candidates` helper functions to `calib-targets-aruco`
  (crate-private).
- Move `log` from a dev-dependency to a regular dependency in
  `calib-targets-aruco` to support debug logging in production builds.

### Diagnostic logging across the detection pipeline

- **`calib-targets-chessboard`**: add `log::debug!` at every significant
  rejection point in the chessboard detector — early corner count check,
  post-orientation-filter count, per-component BFS/grid-fit/completeness
  rejections, and the accepted-candidate summary. Add `log_graph_summary`
  helper that logs grid-graph component sizes and node-degree distribution.
- **`calib-targets-charuco` pipeline**: add `log::debug!` / `log::warn!` at
  each stage — chessboard success/failure (with config details on failure),
  marker sampling cell counts, scan result count, alignment result (transform +
  inlier count), pre- and post-validation corner counts. Failed-cell details
  (border-score, observed code) now logged at `debug` level from
  `scan_decode_markers_in_cells`.
- **`charuco_detect` example**: switch default log level to `debug`; add
  config-echo logging on startup.

## [0.3.0]

### Printable targets

- Add the dedicated `calib-targets-print` crates.io crate to the coordinated
  Rust release flow and document it as a first-class published printable-target
  entry point alongside `calib_targets::printable`.
- Add a canonical printable-target guide with JSON, Rust, CLI, and Python
  flows, output-bundle expectations, and print-at-100%-scale guidance.
- Productize the repo-local `calib-targets-cli` workflow with
  `list-dictionaries` and `validate`, clearer help text, and integration
  coverage for discover/init/validate/generate flows.
- Add explicit millimeter-aware conversions from `CharucoBoardSpec` and
  `MarkerBoardLayout` into printable target specs and printable documents.

### Native C API

- Add the repo-local `calib-targets-ffi` crate and generated public C header
  for native consumers. The FFI crate remains `publish = false` and is built
  from the workspace rather than distributed on crates.io.
- Add fixed-struct C detector APIs for chessboard, ChArUco, and checkerboard
  marker-board detection over 8-bit grayscale images, with opaque handles,
  explicit status codes, caller-owned query/fill buffers, full ChESS
  configuration, and built-in dictionary names only.
- Add repo-owned native validation for the C API: generated-header drift
  checks, a plain C smoke example, a thin header-only C++17 RAII
  wrapper/example, and a Cargo-driven smoke test that compiles and runs
  external C and C++ consumers against the built shared library.
- Add repo-local ergonomic C++/CMake consumer packaging: stage Cargo-built
  artifacts into a deterministic CMake package prefix, export
  `calib_targets_ffi::c` and `calib_targets_ffi::cpp` targets, and validate a
  repo-owned `find_package(...)` consumer example in CI.
- Add tagged native release assets for `calib-targets-ffi`: supported GitHub
  releases now attach per-platform archives containing the staged `include/`,
  `lib/`, and `lib/cmake/` prefix so downstream C/C++ consumers can integrate
  without building Rust from source.
- Clarify current native-consumer boundaries: the release archives are the
  supported distribution format for Linux, macOS, and Windows tags, but there
  is still no crates.io/package-manager distribution, installer flow, or signed
  native package.

## [0.2.5]
- Maintenance release: bump crate versions to `0.2.5`.

## [0.2.4]
- Fix ChArUco false-corner detection: ArUco marker-interior saddle points
  could displace true chessboard-grid corners in the graph BFS and produce
  ChArUco corners with correct IDs but wrong pixel positions.
- Add marker-constrained corner validation stage in `calib-targets-charuco`:
  estimates a board-to-image homography from all inlier marker corners and
  flags corners whose reprojection error exceeds `corner_validation_threshold_rel
  * px_per_square` (default 8%). Flagged corners are re-detected via a local
  ChESS patch search seeded at the projected position.
- Add `corner_validation_threshold_rel` and `corner_redetect_params` to
  `CharucoDetectorParams`.
- Add `chess-corners-core` as a production dependency of `calib-targets-charuco`.

## [0.2.3]
- Python bindings: switch to a mixed Rust/Python package layout with private
  extension module `calib_targets._core` and typed public package sources.
- Python API: hard reset to a dataclass-first surface with typed-only config
  inputs and typed detector result objects.
- Add `to_dict()` / `from_dict(...)` compatibility helpers on public config and
  result models.
- Add generated typing artifacts (`_core.pyi`, dictionary literal definitions)
  and a generator script with `--check` mode for CI.
- Add Python type-check smoke coverage (Pyright + mypy) in CI.

## [0.2.2]
- Remove redundant ChArUco board parameter; board spec now lives in params for Rust and Python APIs.
- `CharucoDetector::new` now takes only `CharucoDetectorParams`.
- Add typed Python classes for `CharucoBoardSpec`, `MarkerBoardLayout`, and `MarkerCircleSpec`.
- Make Python config classes mutable via settable attributes.
- Document authoritative Python output schema in `crates/calib-targets-py/README.md`.

## [0.2.1]
- Add Python-friendly config/params classes with IDE signatures while keeping dict overrides.
- Allow partial dict overrides for detector params without specifying full structs.
- Validate unknown keys in Python config dicts with clearer error paths.
- Improve Python conversion errors to include parameter paths and accept NumPy scalars.

## [0.2.0]
- Document the Python bindings across the workspace README, crate readmes, and book.
- Clarify marker-board `cell_size` usage so `target_position` is populated when alignment succeeds.
- Fix macOS Python binding linking via a PyO3 build script.
- Refresh PyO3 bindings to the Bound API to remove deprecation warnings.
- Bump `chess-corners` dependency to v0.3.

## [0.1.2]
- Speed up marker circle scoring with LUT-based sampling and a center precheck.
- Add a fast in-bounds bilinear sampling helper for hot paths.

## [0.1.1]
- Initial public release of the calib-targets crates.
