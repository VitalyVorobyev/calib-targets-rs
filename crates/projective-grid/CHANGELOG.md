# Changelog

All notable changes to `projective-grid` are documented here. This crate has
its own version and release cadence, independent of the surrounding workspace.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.12.0] - 2026-08-09

This is a pre-1.0 API consolidation release. Square `Oriented2` is the mature
path; Square `Positions` / `Oriented1` remain evidence-limited, and all Hex
paths remain experimental pending real-image campaigns.

### Added

- `(Hex, Oriented1)` detection. The measured physical axis family and its
  uncertainty are preserved while the two missing local families are inferred
  from six-neighbour chord evidence.
- An opt-in `diagnostics` feature with Rust-backed stage snapshots and rejected
  feature attribution, separate from ordinary detection results.
- Deterministic Hex fixtures covering perspective, modulo-π seams, noise,
  dropout, shuffled inputs, and near/off-lattice clutter.
- Criterion coverage for native square and Hex `Oriented1` detection.

### Changed

- `DetectionRequest::new` now takes only `(lattice, evidence)`. Optional
  dimensions and non-default parameters use the named `with_dimensions` and
  `with_params` builders.
- `DetectionParams` now exposes only stable ordinary configuration. Stage-level
  controls moved to `expert::DetectionTuning` and are attached with
  `with_advanced`.
- `detect_grid` and `detect_grid_all` return the mandatory `GridDetection`
  contract. Rejections and intermediate pipeline state moved to the opt-in
  diagnostics API.
- The public surface is organized around the ordinary facade, a curated
  detector-builder `expert` seam, and feature-gated diagnostics. Topological
  orchestration remains crate-internal.
- `projective-grid` now has an independent `0.12.x` version and is released by
  `projective-grid-vX.Y.Z` tags.

### Fixed

- Non-finite positions, axes, uncertainties, and invalid tuning are rejected
  with typed errors before triangulation instead of reaching geometry code.
- Fit and validation ordering is deterministic across input and map iteration
  order.
- Hex `Oriented3` treats axis slots as an unordered local set; matching no
  longer requires the same slot index at both endpoints of an edge.
- Square synthesized-axis recovery distinguishes fully measured from
  synthesized evidence without a cryptic boolean contract.

### Removed

- The unused `Evidence::CoordinateHypotheses` detection roadmap variant.
  Caller-proposed labels remain supported by the separate
  `check_consistency` task.
- Public `DetectionReport` / `GridSolution` diagnostics from the ordinary
  result path; their diagnostic information is available through the
  `diagnostics` feature.

### Migration

See [Road to 1.0](https://github.com/VitalyVorobyev/calib-targets-rs/blob/projective-grid-v0.12.0/crates/projective-grid/ROADMAP-1.0.md)
for the compact 0.11 → 0.12 request example, release process, readiness status,
and remaining stabilization work.

[Unreleased]: https://github.com/VitalyVorobyev/calib-targets-rs/compare/projective-grid-v0.12.0...HEAD
[0.12.0]: https://github.com/VitalyVorobyev/calib-targets-rs/compare/v0.11.2...projective-grid-v0.12.0
