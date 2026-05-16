# Changelog

All notable changes to this project will be documented in this file.

This project follows [Semantic Versioning](https://semver.org/).

Older releases are archived under [`docs/changelog/`](docs/changelog/);
see [Older releases](#older-releases) at the bottom for the index.

## [Unreleased]

## 0.9.0

### Breaking

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

### Added

- `crates/calib-targets-py/python_tests/test_chess_config_rust_roundtrip.py`
  — end-to-end coverage that the Python `ChessConfig.to_dict()`
  payload deserializes cleanly through Rust's `serde_json`
  layer on a real test image. Guards against the silent
  dict-shape drift that fixtures alone cannot catch.

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
