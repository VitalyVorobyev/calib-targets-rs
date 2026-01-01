# Changelog

All notable changes to this project will be documented in this file.

This project follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

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
