# Changelog

All notable changes to this project will be documented in this file.

This project follows [Semantic Versioning](https://semver.org/).

## [Unreleased]
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
