# Changelog

All notable changes to this project will be documented in this file.

This project follows [Semantic Versioning](https://semver.org/).

Older releases are archived under [`docs/changelog/`](docs/changelog/);
see [Older releases](#older-releases) at the bottom for the index.

## 0.8.0

Hardens the chessboard detector with a mandatory final-geometry
check, lands an opt-in topological grid pipeline alongside the
seed-and-grow default, and rewrites the per-cell DLT and
component-merge hot paths for an order-of-magnitude speedup on
high-resolution frames.

### Breaking

- `chess-corners` bumped 0.7 → 0.8; the direct `chess-corners-core`
  dep is gone (the 0.8 facade re-exports every primitive we used).
  Several upstream config types are now `#[non_exhaustive]` —
  downstream construction must use `RefinerConfig::saddle_point()` /
  `::build(...)` rather than struct literals.
- `calib-targets-core::chess` now `pub use`-re-exports
  `chess-corners` config types directly instead of mirroring them.
  `DetectorMode::Radon` and `RefinementMethod::RadonPeak` are
  exposed; `RefinerKindConfig` and `ChessConfig::from_parts` are
  removed.
- `GridIndex` renamed to `GridCoords` workspace-wide; square-grid
  types gain a `Square` prefix (`SquareGridHomography`,
  `SquareGridHomographyMesh`, `square_predict_grid_position`, …).
- `SeedQuadValidator::axes` now returns `[AxisHint; 2]`;
  `LocalStepPointData::{axis_u, axis_v}` merged into `axes`.
- `ExtensionCommonParams` extracted as `pub common` on
  `ExtensionParams` / `LocalExtensionParams`.
- Public `count` diagnostic fields standardised on `usize`.
- Internal module splits in `calib-targets-chessboard` (extension/,
  validate/, grow_extend); public re-exports preserved.

### Added

- **Mandatory final geometry check** in the chessboard detector:
  every emitted `Detection` is run through a precision gate that
  drops gross mislabels (looser line/local-H tolerances than the
  per-attachment gates) and any corner outside the largest
  cardinally-connected component. Catches wrong `(i, j)` labels
  and isolated false positives that prior stages miss.
- **Post-grow centre refit (Stage 6.75)**: after the boosters
  converge, recompute axis centres from the labelled set alone
  (undirected circular mean), reclassify, and run a BFS regrow
  plus a cardinal-only BFS extension to pick up cells the original
  centres missed. Trades 1-2 borderline corners for whole orphan
  strips on ChArUco-style images.
- **Topological grid pipeline.**
  `projective_grid::build_grid_topological` implements the
  Shu / Brunton / Fiala 2009 grid finder (Delaunay → triangle
  classification → quad merge → flood-fill labelling). Image-free
  (axis-driven cell predicate replaces the paper's colour test).
  Opt-in via
  `DetectorParams::graph_build_algorithm = Topological`; default
  stays `ChessboardV2`. ChArUco pins `ChessboardV2` regardless of
  caller choice.
- **Local-homography extension.**
  `extend_via_local_homography` fits a per-candidate `H` from the
  `K` nearest labelled corners instead of one global fit; tolerates
  heavy radial distortion and multi-region perspective.
- **Shared component-merge.**
  `projective_grid::component_merge::merge_components_local`
  reunites partial components from either pipeline using local
  geometry only. The chessboard crate's `enable_component_merge`
  flag is backed by this shared implementation.
- **`projective_grid::square::grow::extend_from_labelled`** —
  reusable cardinal-only BFS extension over an existing labelled
  set.
- **PIPELINE.md** per-detector docs (chessboard, charuco,
  puzzleboard, marker, topological): atomic stage tables of input
  / decision / output / failure modes / knobs.
- **Per-stage diagnostics.** `IterationTrace` gains
  `rescue / refit / extension2 / rescue2 / geometry_check` buckets
  with `attached / rejected_*` counters; now `#[non_exhaustive]`.
- `bench {run,check,diagnose} --chessboard-config <FILE>` for
  partial-JSON override of `DetectorParams` without rebuilds.

### Performance

End-to-end: full `detect_chessboard` on a 12 MP frame drops from
seconds to tens of milliseconds, zero precision regression on the
internal regression set.

- `projective_grid::homography::estimate_homography` rewritten via
  normal equations + 9×9 symmetric eigendecomposition (was a full
  `(2N × 9)` SVD). Hartley normalisation preserved; agreement with
  the SVD reference is ≤ 0.01 px in pixel-domain on a randomised
  battery. Microbench: 1.8× / 2× / 2.6× / 3.3× speedups at
  N = 9 / 25 / 100 / 225.
- `extend_via_local_homography` drops the wasted 3×3 quality SVD
  per cell.
- `merge_components_local` rewritten as a KD-tree-indexed Hough
  vote on `(transform, label-delta)`; was `O(P² Q)` per component
  pair, now linear in matched corners. Two-to-three orders of
  magnitude on microbenches.
- `nearest_labelled_by_grid` switched from full-sort to bounded
  max-heap (`O(L log K)`); cuts the extension stage ~4× end-to-end.

### Tooling

- Opt-in `tracing` feature on `projective-grid`; hot-path entry
  points emit `tracing::instrument` spans.
- New `[profile.profiling]` Cargo profile and
  `examples/profile_grid.rs` driver for `samply record` /
  `RUST_LOG` tracing dumps; full recipe in `docs/profiling.md`.
- New criterion microbenches: `topological.rs`, `merge.rs`,
  `validate.rs`; `homography.rs` adds `K=8/12/20` cases.

## [0.7.3]

WASM-and-tooling release. The npm WASM package gains feature parity with
the Rust facade and a typed object-shape surface, the published GitHub
Pages site picks up an interactive playground, and a new dataset-gated
PuzzleBoard regression locks the `Full` vs `FixedBoard` agreement
contract on a known printed-board dataset. No Rust API breakage.

### Changed

- **WASM npm package renamed** from `calib-targets-wasm` to the scoped
  public package `@vitavision/calib-targets`. The Rust crate name
  (`calib-targets-wasm`) is unchanged. Update consumers to
  `npm install @vitavision/calib-targets` and rewrite imports from
  `"calib-targets-wasm"` to `"@vitavision/calib-targets"`.

### Added

- **WASM API parity with the Rust facade.** New exports in
  `calib-targets-wasm`: `default_charuco_params`,
  `list_aruco_dictionaries`, `chessboard_sweep_default`,
  `charuco_sweep_for_board`, `puzzleboard_sweep_for_board`,
  `render_chessboard_png`, `render_charuco_png`,
  `render_marker_board_png`. PuzzleBoard PNG rendering ceases to be a
  special case — every supported target family renders through the
  same surface.
- **Typed WASM object shapes for TypeScript.** A hand-written
  `typescript-extras.d.ts` (43 result and parameter shapes — `Corner`
  with `axes`, the flat 30-field `DetectorParams`, PuzzleBoard
  search/scoring tagged enums, every supporting struct) is appended
  to the auto-generated `calib_targets_wasm.d.ts` during build, so
  npm consumers get strongly-typed object shapes alongside function
  signatures. Wired into both `scripts/build-wasm.sh` and the npm
  release workflow.
- **mdBook playground.** New `book/src/playground.md` chapter
  iframes the React/Vite demo at `./playground/` so the published
  GitHub Pages site has an interactive playground. The docs workflow
  builds the WASM crate, runs `vite build`, and rsyncs `demo/dist/`
  into `public/playground/` alongside the existing `api/` and `book/`
  trees.
- **Demo refresh.** The interactive demo now uses the WASM default
  helpers (`default_charuco_params`, `list_aruco_dictionaries`, the
  three sweep presets) instead of pre-0.7-schema constants that
  silently no-op'd on the current deserializer. Adds a
  "use 3-config sweep" toggle wired to the `detect_*_best` family,
  generates synthetic targets across all four target kinds, and
  exposes a dictionary dropdown for ChArUco.
- **Dataset-gated PuzzleBoard regression test.**
  `crates/calib-targets-puzzleboard/tests/dataset_full_mode_bounds.rs`
  freezes three contracts on a known printed-board dataset:
  `FixedBoard + SoftLogLikelihood` keeps every decoded
  `target_position` inside the declared board's bounds at low BER;
  `Full` and `FixedBoard` pick the same `(D4, master_origin_*)` on
  the pinned fixtures; and an on-demand `--ignored` sweep covers
  every available snap under both scoring modes. Skips silently
  when the dataset is missing.

### Fixed

- **Demo ChArUco sliders bind to the post-0.7 schema.** Sliders
  now read and write `charucoParams.board.*` (not the stale
  `.charuco.*`) and chessboard sliders use the flat-`DetectorParams`
  field names (`min_corner_strength`, `min_labeled_corners`,
  `max_components`, `cell_size_hint`).

## [0.7.2]

PuzzleBoard feature-and-fix release. This version removes the large
axis-aligned master-alias jumps seen on real multi-camera data, exposes
the new PuzzleBoard search/scoring surfaces consistently across every
binding layer, and refreshes the documentation around the supported
workflow.

### Fixed

- **PuzzleBoard fixed-board origin recovery.** `decode_fixed_board*`
  now uses the same D4-aware edge-lookup convention as the full search
  path and reports the physical board placement directly instead of a
  CRT-selected master alias. This removes the `~350 mm` horizontal /
  vertical target-position jumps that previously split different camera
  views of the same target into different board-frame quadrants.

### Added

- **PuzzleBoard scoring modes.** `PuzzleBoardScoringMode` is now part of
  the public Rust crate surface, with `SoftLogLikelihood` as the default
  and `HardWeighted` kept as a legacy diagnostic mode.
- **Richer PuzzleBoard diagnostics.** `PuzzleBoardDecodeInfo` now
  carries `scoring_mode`, `score_best`, `score_runner_up`,
  `score_margin`, and the runner-up origin / D4 transform when
  available.
- **Binding parity for PuzzleBoard.** Python, WASM, and the repo-local C
  ABI now all expose the PuzzleBoard search/scoring knobs and decode
  diagnostics, so `Full` / `FixedBoard` and `HardWeighted` /
  `SoftLogLikelihood` can be selected consistently across languages.
- **PuzzleBoard dataset tooling.** The dataset runner accepts
  `--search-mode full|fixed-board` and `--scoring-mode hard|soft`, and
  the new regression surface covers D4-invariant fixed-board decoding
  plus the previously failing `180° + upscale=2` rotation case.

## [0.7.1]

Packaging-only follow-up to `0.7.0`. No API or behavior changes.

### Fixed

- **Release workflow.** Broke a dev-dependency cycle between
  `calib-targets-chessboard` / `calib-targets-charuco` and the
  `calib-targets` facade that caused `cargo publish --verify` to fail
  when resolving the not-yet-uploaded facade against crates.io. The
  dev-deps are now path-only (matching `calib-targets-puzzleboard`'s
  existing convention). Also added `calib-targets-puzzleboard` to the
  publish order so `calib-targets-print` can resolve its regular
  dependency on it, and hardened the retry loop in
  `.github/workflows/publish-crates.yml` to treat an already-uploaded
  version as success (idempotent re-runs). Version-resolution failures
  remain retryable — the crates.io index can legitimately lag behind a
  just-uploaded dependency in the same publish chain.

## [0.7.0]

Coordinated workspace release that lands the **invariant-first
chessboard detector rewrite** with precision-by-construction on a
private regression dataset (non-negligible lens distortion and motion
blur): high detection rate, zero wrong `(i, j)` labels. This release breaks
the old chessboard API wholesale (rename + flat params shape), hoists
the pattern-agnostic pieces into `projective-grid` as a first-class
standalone library, reshapes the C ABI to match, and refreshes every
book chapter and crate README for the new surface. Workspace minor-
bumps in lockstep: every crate publishes at `0.7.0`.

### Changed — breaking

- **Chessboard detector rewrite.** The prior `calib-targets-chessboard`
  implementation (graph-based, with nested `GridGraphParams`,
  `LocalHomographyPruneParams`, `GraphCleanupParams`,
  `GapFillParams`, `OrientationClusteringParams`) is replaced by the
  invariant-first detector. Type names change from
  `ChessboardDetector` / `ChessboardParams` /
  `ChessboardDetectionResult` to `Detector` / `DetectorParams` /
  `Detection`. `DetectorParams` is flat — 30 tuning fields covering
  the 8-stage pipeline (pre-filter, clustering, cell size, seed,
  grow, validate, boosters, output gates). The detector enforces two
  hard invariants on its output: no duplicate `(i, j)` labels, and
  the bounding-box minimum rebased to `(0, 0)` with `(0, 0)` sitting
  at the **visual top-left** of the detected grid (`+i` right, `+j`
  down).
- **Facade surface update.**
  `calib_targets::detect::detect_chessboard` now takes
  `&DetectorParams`. New helpers:
  `detect_chessboard_all` (multi-component, same-board pieces),
  `detect_chessboard_best` (3-config sweep), and
  `detect_chessboard_debug` (full per-stage `DebugFrame`).
- **ChArUco chessboard field.** `CharucoParams.chessboard` is now
  `DetectorParams`. Nested `graph` / `graph_cleanup` / `gap_fill` /
  `local_homography` sub-fields are removed.
- **C ABI reshape (breaking — `publish = false`).**
  `ct_chessboard_params_t` is reshaped to the flat 30-field layout
  mirroring `DetectorParams`. Removed:
  `ct_grid_graph_params_t`, `ct_orientation_clustering_params_t`,
  `min_corners`, `expected_rows`, `expected_cols`,
  `completeness_threshold`, `use_orientation_clustering`,
  `orientation_clustering_params`, `graph`. The chessboard result
  struct replaces `has_orientations` / `orientation_0` /
  `orientation_1` with always-populated `grid_direction_0_rad` /
  `grid_direction_1_rad` / `cell_size`. New initialiser
  `ct_chessboard_params_init_default` populates a valid default-
  configured value so C callers don't hand-fill 30 fields.
- **Python binding field shape.** The Python-side
  `ChessboardParams` class keeps its name but its fields now mirror
  the new flat `DetectorParams` (no more nested `graph` /
  `graph_cleanup` / `gap_fill` / `local_homography` sub-structs).
- **Retired the `calib-targets-cli` crate.** Its binary (`calib-targets`)
  moved into the facade crate at `crates/calib-targets/src/cli/`,
  split across per-subcommand modules (`init`, `gen`, `generate`,
  `validate`, `dictionaries`, `args`, `error`). Integration tests
  moved to `crates/calib-targets/tests/cli.rs` and were extended with
  coverage for every `gen <target>` path and the new PuzzleBoard init
  flow. End-user command invocations are unchanged.

### Added

- **Standalone `projective-grid` crate.** Pattern-agnostic
  grid-detection primitives, usable without any calibration
  dependencies:
  - `projective_grid::square::validate` — line-collinearity + local-
    H-residual validator with attribution rules.
  - `projective_grid::circular_stats` — `wrap_pi`,
    `angular_dist_pi`, `smooth_circular_5`, plateau-aware
    `pick_two_peaks`, double-angle `refine_2means_double_angle`.
  - `projective_grid::square::grow` — generic BFS grid grower
    behind a `GrowValidator` trait. Chessboard's detector plugs in a
    chess-parity impl; non-calibration consumers supply their own.
  - `projective_grid::square::seed` — `Seed` / `SeedOutput` data
    types, `seed_cell_size`, `seed_homography`, and the pure-geometry
    `seed_has_midpoint_violation` helper that rejects 2× spacing
    mislabels.
- **testdata regression harness.**
  `crates/calib-targets-chessboard/tests/testdata_regression.rs` +
  `testdata/chessboard_regression_baselines.json` gate detection on
  the broader testdata set (mid, large, small0..5, and 10
  `puzzleboard_reference/example*.png` images) with per-image
  minimums + hard invariants (no duplicate labels, origin rebased,
  `(0, 0)` at visual top-left). Runs in every `cargo test`
  invocation.
- **Single-image inspection pipeline.** New
  `calib-targets-chessboard/examples/debug_single.rs` emits a per-
  image `CompactFrame` JSON; the Python overlay at
  `crates/calib-targets-py/examples/overlay_chessboard.py` grows a
  `--single-image` mode. `scripts/chessboard_regression_overlays.sh`
  drives the 19-image set end-to-end.
- **Book chapters.** New `book/src/projective_grid.md`. Rewrites of
  `book/src/chessboard.md` (folded-in algorithm spec),
  `pipeline.md`, `tuning.md`, `troubleshooting.md`,
  `example_chessboard.md`, `roadmap.md`.
- **`detect_chessboard_all` exposed in Python, WASM, and FFI bindings.**
  The multi-component chessboard detection helper (returns every same-board
  component up to `max_components`) is now available in all three bindings,
  closing the parity gap noted in the Python and WASM READMEs. FFI entry
  point: `ct_chessboard_detector_detect_all`. Python entry point:
  `calib_targets.detect_chessboard_all`. WASM entry point:
  `detect_chessboard_all`.
- **Published CLI for printable-target generation.** The `calib-targets`
  binary now ships with the facade crate behind the default `cli` feature
  (`cargo install calib-targets`) and is mirrored as a Python console
  script in `calib-targets-py` via `[project.scripts]`
  (`pip install calib-targets`). Both CLIs expose the same subcommand
  taxonomy:
  - `gen {chessboard,charuco,puzzleboard,marker-board}` — one-step flags
    → JSON + SVG + PNG bundle, backed by new ergonomic helpers in
    `calib_targets::generate` (Rust) and `calib_targets.printing`
    (Python): `chessboard_document`, `charuco_document`,
    `puzzleboard_document`, `marker_board_document`.
  - `init {chessboard,charuco,puzzleboard,marker-board}` — write a
    reviewable spec JSON first; closes the long-standing gap where
    PuzzleBoard was missing from the CLI init surface.
  - `generate`, `validate`, `list-dictionaries` — unchanged semantics,
    now accessible from a `pip`- or `cargo`-installed binary rather than
    a repo-local crate.

### Fixed

- **Grid origin.** `(0, 0)` now always lands at the visually top-
  left corner of the detected grid (`+i` right, `+j` down in image
  pixels). Previously the axis assignment was tied to the seed's
  internal slot convention, so `(0, 0)` could appear anywhere on the
  board.
- **Plateau-aware peak detection.** Clustering no longer fails on
  perfectly rectilinear boards (synthetic puzzleboards at
  `testdata/puzzleboard_reference/example8.png` /
  `example9.png`) where a physical direction's mass splits across a
  histogram bin boundary and the smoothed peak becomes flat-topped.
- **`min_peak_weight_fraction` default 0.05 → 0.02.** On noisy real-
  world ChArUco snaps (`small1`, `small3`, `small4` from the
  testdata set), the real per-peak weight on fine 2° bins is ~2-3%
  of total vote weight, below the old threshold. The new default
  stays comfortably above pure-noise bins.
- **Soft convergence for oscillating validation.** The
  validate→blacklist→regrow loop now accepts a "near-converged"
  state when the most recent iteration's new blacklist is ≤ 2
  corners and the labelled count has reached `min_labeled_corners`.
  This unblocks `testdata/puzzleboard_reference/example1.png` where
  the loop oscillated on 2–4 borderline-outlier corners and
  previously exhausted `max_validation_iters` without emitting.
- **`line_tol_rel` default 0.15 → 0.18.** Under extreme perspective
  on dense boards (`testdata/puzzleboard_reference/example2.png`),
  legitimate inner corners near the near-camera edge were blacklisted
  because their perpendicular residual against a long-column
  straight-line fit slightly exceeded the old tolerance. The
  invariant-first contract still holds — line-failure is only one of
  several independent blacklist conditions.
- **`max_validation_iters` default 3 → 6.** Absorbs wider real-
  world variance on dense boards.
- Three post-swap regressions in `calib-targets-charuco/tests/
  regression.rs` (`detects_charuco_on_small_png`,
  `detects_plain_chessboard_on_mid_png`) and
  `calib-targets-puzzleboard/tests/end_to_end.rs`
  (`fixed_board_agrees_across_disjoint_partial_views`) now pass and
  are un-ignored.
- Python binding: `CharucoDetectionResult.from_dict` now accepts the
  `raw_marker_count` / `raw_marker_wrong_id_count` fields emitted by
  the Rust serialiser, so `detect_charuco` returns instead of raising
  `ValueError: CharucoDetectionResult: unknown keys ...`.

### Infrastructure

- **Privatedata split.** The private 120-frame regression benchmark
  is copyrighted customer material and is not committed to the
  repository. Tests and benches read it from `privatedata/` when it
  is available and skip (never panic) when it is not, so CI on a
  fresh public checkout passes without any private asset.
  `.gitignore` adds `privatedata`.
- Regenerated FFI headers
  (`crates/calib-targets-ffi/include/calib_targets_ffi.h`) match the
  new struct layout.

### Documentation & onboarding

- Rewrote every crate README (repo root, facade, `projective-grid`,
  `calib-targets-core`, `calib-targets-chessboard`, `calib-targets-aruco`,
  `calib-targets-charuco`, `calib-targets-puzzleboard`,
  `calib-targets-marker`, `calib-targets-print`, `calib-targets-py`,
  `calib-targets-wasm`) for new-user friendliness,
  with explicit Inputs / Outputs, Configuration, Tuning, and Limitations
  sections, and crates.io-compatible links into the mdBook.
- Added a composed target-gallery hero image at
  `docs/img/target_gallery.png`, generated reproducibly from
  `scripts/compose_target_gallery.py`.
- Added per-target-type Python round-trip examples (generate → detect →
  export JSON) under `crates/calib-targets-py/examples/`:
  `chessboard_roundtrip.py`, `charuco_roundtrip.py`,
  `markerboard_roundtrip.py` (the `puzzleboard_roundtrip.py` example
  already existed).

## Older releases

The full release history is preserved under
[`docs/changelog/`](docs/changelog/), grouped by minor-version family:

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
