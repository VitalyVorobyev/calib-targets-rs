# Changelog

All notable changes to `projective-grid` are documented here. This crate has
its own version and release cadence, independent of the surrounding workspace.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.14.0] - 2026-08-15

[#77]: https://github.com/VitalyVorobyev/calib-targets-rs/issues/77
[#78]: https://github.com/VitalyVorobyev/calib-targets-rs/issues/78

### Added

- **Lattice-orientation parity as a precision invariant.** A homography
  preserves orientation over any region that does not cross its vanishing line,
  which an imaged planar target never does. The sign of the local basis
  `e_u x e_v` is therefore the same at every cell of a correctly labelled
  component — for any viewpoint, focal length, or amount of smooth lens
  distortion. `drop_set` now always drops labels that violate it. This catches
  a sub-block glued in with a reflected axis, which reverses the sign while
  leaving edge lengths, edge directions and pixel separations plausible, so no
  first-order wrong-label check can see it. The criterion compares a sign and
  has no tolerance to tune.

### Fixed

- **Detection is deterministic for a fixed input.** `detect_grid` could return
  different labellings for byte-identical input in different processes — most
  runs labelling a 24 x 24 dot grid in full, roughly one in thirty dropping a
  whole component ([#77]). Three reductions over the labelled set read
  `HashMap` iteration order, which `std` reseeds per process: the
  boundary-extension BFS seeded its queue straight from the map and then
  claimed corners first-come-first-served, and `ensure_axes` and `cell_size_of`
  accumulated `f32` sums, which are not associative. All three iterate in
  sorted cell order now.

  A 144-regime sweep (perspective x rotation x noise x dropout) found one
  regime returning four different labellings over 40 repeats of the same input;
  after the fix every regime is repeat-stable. `tests/determinism.rs` guards it
  in-process — `RandomState` draws fresh keys per `HashMap`, so repeating a
  detection inside one process already varies hash order — and
  `examples/determinism_probe.rs` sweeps for new unstable regimes.

- **Hex recall under sub-pixel noise is now covered by a test.** [#78] reported
  an axis-aligned rectangular hex patch with no perspective term collapsing
  from 30 labelled cells to 12 under 0.1 px centre noise, against 0.10.1. It
  does not reproduce on this version: the same geometry labels every cell, with
  labels consistent with ground truth, from 0 to 0.5 px noise and across a
  10x spacing range. Every previous hex fixture carried a perspective term and
  a hex-*disc* shape, so the reported configuration was untested — it now has
  its own guard in `tests/detect_hex_positions.rs`.

### Changed

- **`merge_components_local` and `merge_components_local_for` take the
  components plus one shared `positions` slice; `ComponentInput` is removed.**
  Every component indexes the same corner array, so "the same corner appears in
  two components" is a well-defined question. The previous shape allowed each
  component its own index space, where it is not.
- **Both merge entry points keep the labelling injective in both directions.**
  One lattice coordinate holds one corner, and one corner sits at one
  coordinate. A candidate label violating either is skipped, leaving a gap
  rather than a duplicate: a gap costs recall, a duplicate is a wrong label a
  consumer cannot recover from. Previously only the coordinate direction was
  guarded, so a thin overlap could re-label a single corner at a run of
  coordinates.
- **`DropSet` is `#[non_exhaustive]`** and reports the parity drops separately
  as `orientation_drop`, so a caller can attribute them in its own trace.

### Migration

```rust
// before
let views: Vec<ComponentInput<'_>> = components
    .iter()
    .map(|labelled| ComponentInput { labelled, positions })
    .collect();
let merged = merge_components_local(&views, &params);

// after — components and positions are separate arguments
let merged = merge_components_local(&components, positions, &params);
```

Callers that built each component over its own `positions` slice must first
re-index them into one shared corner array. Matches on `DropSet` need a
wildcard arm.

## [0.13.0] - 2026-08-10

### Added

- A canonical affine `expert::lattice::GridTransform` containing the lattice
  kind, row-major integer matrix, and translation, with accessors,
  `with_translation`, determinant, application, Serde support, and fallible
  integer inversion.

### Changed

- D4 and D6 symmetry transforms and detector alignments now use the same
  representation. The former public `source_kind` and `matrix` fields become
  invariant-preserving accessors, and translation is now part of the explicit
  `{lattice,matrix,translation}` contract.
- The generic neighbour-midpoint predictor accepts ordinary nalgebra real
  scalar types so workspace square compatibility APIs can delegate to it.
- Reusable prediction, transform, and homography primitives remain in the
  curated `expert` composition namespace; the ordinary crate-root facade is
  unchanged.

### Fixed

- Removed local duplicates of modulo-π wrapping/distance helpers and a second
  D4 literal table.

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

[Unreleased]: https://github.com/VitalyVorobyev/calib-targets-rs/compare/projective-grid-v0.14.0...HEAD
[0.14.0]: https://github.com/VitalyVorobyev/calib-targets-rs/compare/projective-grid-v0.13.0...projective-grid-v0.14.0
[0.13.0]: https://github.com/VitalyVorobyev/calib-targets-rs/compare/projective-grid-v0.12.0...projective-grid-v0.13.0
[0.12.0]: https://github.com/VitalyVorobyev/calib-targets-rs/compare/v0.11.2...projective-grid-v0.12.0
