# Changelog

All notable changes to this project will be documented in this file.

This project follows [Semantic Versioning](https://semver.org/).

## [Unreleased]
- Publish `calib-targets-print` as part of the Rust crates.io release flow,
  update the tagged publish workflow to release it before the `calib-targets`
  facade crate, and clarify that printable generation is a published library
  surface while `calib-targets-cli` remains repo-local.
- Add the repo-local `calib-targets-ffi` crate and generated public C header for native consumers. The FFI crate remains `publish = false` and is built from the workspace rather than distributed on crates.io.
- Add fixed-struct C detector APIs for chessboard, ChArUco, and checkerboard marker-board detection over 8-bit grayscale images, with opaque handles, explicit status codes, caller-owned query/fill buffers, full ChESS configuration, and built-in dictionary names only.
- Add repo-owned native validation for the C API: generated-header drift checks, a plain C smoke example, a thin header-only C++17 RAII wrapper/example, and a Cargo-driven smoke test that compiles and runs external C and C++ consumers against the built shared library.
- Add repo-local ergonomic C++/CMake consumer packaging: stage Cargo-built artifacts into a deterministic CMake package prefix, export `calib_targets_ffi::c` and `calib_targets_ffi::cpp` targets, and validate a repo-owned `find_package(...)` consumer example in CI.
- Add tagged native release assets for `calib-targets-ffi`: supported GitHub releases now attach per-platform archives containing the staged `include/`, `lib/`, and `lib/cmake/` prefix so downstream C/C++ consumers can integrate without building Rust from source.
- Clarify current native-consumer boundaries: the release archives are the supported distribution format for Linux, macOS, and Windows tags, but there is still no crates.io/package-manager distribution, installer flow, or signed native package.

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
