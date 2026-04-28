# Changelog

All notable changes to this project will be documented in this file.

This project follows [Semantic Versioning](https://semver.org/).

## 0.8.0

Coordinated workspace release that hardens the ChessboardV2 detector
with a mandatory final-geometry check, lands an opt-in topological
grid pipeline alongside the seed-and-grow default, and rewrites the
per-cell DLT and component-merge hot paths for an order-of-magnitude
speedup on high-resolution frames. Several internal modules are
re-organised into per-stage subdirectories and `GridIndex` /
`GridHomography` / `GridHomographyMesh` are renamed; see "Breaking
changes" below. Workspace minor-bumps in lockstep at `0.8.0`.

### Breaking changes

- **N-17** `calib-targets-core/src/chess.rs`: replaced the near-verbatim mirror of
  `chess-corners` config types with direct `pub use` re-exports for all non-divergent
  types (`CenterOfMassConfig`, `ForstnerConfig`, `SaddlePointConfig`, `UpscaleConfig`,
  `UpscaleMode`, `DescriptorMode`, `ThresholdMode`, `ChessCornerParams`, `PyramidParams`,
  `CoarseToFineParams`). `chess-corners` is now a direct dependency of
  `calib-targets-core`. `DetectorMode` adds `Radon` variant, `RefinementMethod` adds
  `RadonPeak`, `ChessConfig` exposes `radon_detector`. `RefinerKindConfig` and
  `ChessConfig::from_parts` are removed. Adapter shims in `detect.rs` collapsed.

- **N-1** `GridIndex` renamed to `GridCoords` across the entire workspace (the name
  `calib-targets-core` consumers already used). The old `GridCoords` alias in
  `calib-targets-core` is now the actual type definition. All `(i32, i32)` tuple
  maps in the grow/validate/topological/merge pipelines replaced with `GridCoords`.
  `GridTransform::apply` returns `GridCoords` instead of `[i32; 2]`.

- **N-2** Square-grid types gain `Square` prefix: `GridHomography` →
  `SquareGridHomography`, `GridHomographyMesh` → `SquareGridHomographyMesh`,
  `predict_grid_position` → `square_predict_grid_position`,
  `find_inconsistent_corners` → `square_find_inconsistent_corners`,
  `find_inconsistent_corners_step_aware` → `square_find_inconsistent_corners_step_aware`.

- **N-3** `SeedQuadValidator::axes` returns `[AxisHint; 2]` instead of `[f32; 2]`.
  `LocalStepPointData::{axis_u, axis_v}` merged into `axes: [AxisHint; 2]`.
  `AxisHint::from_angle` constructor added.

- **N-5** `seed_finder.rs` merged into `seed/finder.rs` submodule. `Seed` struct
  moved from `grow.rs` to `seed/mod.rs`. External import paths unchanged via
  re-exports in `square/mod.rs`.

- **N-6** `ExtensionCommonParams` extracted from `ExtensionParams` and
  `LocalExtensionParams`. Both now hold `pub common: ExtensionCommonParams`.

- **N-10** All "count of items" public diagnostic fields standardised on `usize`
  (`GlobalStepEstimate.support`, `LocalStep.supporters_*`, `ExtensionStats.attached`,
  `BfsExtensionStats.attached`, etc.).

- **N-16** Module splits: `grow_extension.rs` → `extension/{mod,common,global,local}.rs`;
  `grow.rs` → `grow.rs` + `grow_extend.rs`; `validate.rs` →
  `validate/{mod,lines,local_h,step}.rs`. Inner `bfs_grow` body extracted to
  `process_boundary_cell`. All public re-exports unchanged.

### Added

- **Mandatory final geometry check** in chessboard-v2 detector —
  every emitted `Detection` now goes through a Stage-9 precision gate
  that:
  1. drops gross mislabels via `validate()` with looser tolerances
     (`geometry_check_line_tol_rel = 0.45`,
     `geometry_check_local_h_tol_rel = 0.6` of cell_size) — full-
     cell / diagonal shifts produce ~1.4 cell residuals, well above
     these tolerances; perspective-distorted corners are spared;
  2. drops labelled corners not in the largest cardinally-connected
     component. A chessboard detection is by construction one
     `(i, j)`-labelled connected planar graph; isolated singletons
     and small-component leaks are false positives (typically a
     marker corner that passed the cluster + parity gates but sits
     well outside the main grid — e.g. `small2.png` had two such
     orphans below the labelled grid before this filter).
  `min_labeled_corners` survivors are required after the drop,
  otherwise the detection is refused entirely. Catches wrong `(i, j)`
  labels and isolated false positives that the per-attachment gates
  couldn't — bench `pos=` does not validate new `(i, j)`
  assignments, so this is the precision safety net.
- **Stage 6.75 — post-grow centre refit.** After Stage 6.5 / boosters
  converge, recompute `(θ₀, θ₁)` from the labelled set's axes alone
  using the undirected circular mean. Marker-internal corners — which
  bias the histogram-driven Stage-3 centres ~3° below the true
  chessboard axes on some ChArUco-style images — don't contribute
  here, so the refined centres are unbiased. If the shift exceeds
  `refit_min_shift_deg`, re-classify Strong / NoCluster corners under
  the new centres, then run two complementary growth passes (in
  order):
  - **BFS regrow** (`enable_post_grow_bfs_regrow`, default `true`):
    demote every `Labeled` corner back to `Clustered` and re-run
    `grow_from_seed` with the refined centres. Lifts recall on
    cases where the orphan strip is 1+ cells past the existing
    bbox edge (small3.png left strip, +21 corners). Can drop a few
    borderline corners under the centre shift.
  - **Cardinal-neighbour BFS extension**
    (`enable_post_grow_bfs_extend`, default `true`,
    `projective_grid::square::grow::extend_from_labelled`):
    non-destructive walk over the labelled bbox boundary using
    cardinal-only prediction (K=1). Recovers corners the regrow
    above dropped under the centre shift, without precision risk
    (same tolerances as the initial BFS — wider radii produce
    SHIFT-INCONSISTENT labelling).
  - Then Stage 6 / 6.5 with the refined centres for any cells the
    BFS still missed.
  Defaults: enabled, `refit_min_labelled = 8`,
  `refit_min_shift_deg = 0.5°`. Trade-off: chessboard targets gain
  recall (small3.png 91→104); puzzleboard targets that go through
  chessboard-v2 incidentally lose perimeter rows
  (`example2.png` 136→91 — these go through their own pipeline
  in production). Zero `pos` / `id` / `dup` / SHIFT-INCONSISTENT
  regressions across the entire bench.
- **`projective_grid::square::grow::extend_from_labelled`** — new
  public BFS extension over an existing labelled set. Used by the
  chessboard refit pass; reusable by any pattern-specific detector
  that wants to absorb newly-eligible corners without re-running
  BFS from scratch. Returns `BfsExtensionStats` with
  `attached / rejected_*` counters.
- **PIPELINE.md docs** for chessboard, charuco, puzzleboard, marker,
  and the topological grid sub-pipeline. Concise atomic stage tables
  (input / decision / output / failure modes / knobs) at
  `crates/<crate>/docs/PIPELINE.md` (and
  `crates/projective-grid/docs/TOPOLOGICAL_PIPELINE.md`). Working
  reference for diagnosing failures, supersedes the prose sections
  of `docs/projective_grid_overview.md` for per-pipeline depth.
- **`IterationTrace.rescue / refit / extension2 / rescue2 /
  geometry_check`** in `calib-targets-chessboard` — every post-BFS
  stage now records its `attached / rejected_*` breakdown, so a
  `bench diagnose` JSON pinpoints exactly where a corner was dropped.
  `IterationTrace` is now `#[non_exhaustive]` (matches the workspace
  convention for diagnostic structs).
- **`bench {run,check,diagnose} --chessboard-config <FILE>`** —
  optional partial-JSON override for `DetectorParams`, used to sweep
  detector knobs without rebuilding. Missing fields fall back to
  defaults via the existing `#[serde(default)]` attributes.
- **`crates/calib-targets-chessboard/docs/PIPELINE.md`** — atomic
  stage-by-stage map of the chessboard-v2 detector: per-stage input,
  decision predicate, output, dominant failure modes, and governing
  `DetectorParams` knobs. Working reference for diagnosing failures
  on real images.
- **Topological grid pipeline.** `projective_grid::build_grid_topological`
  implements the Shu / Brunton / Fiala 2009 grid finder: Delaunay
  triangulation over the corner cloud, edge classification by
  per-edge axis match, triangle-pair → quad merge, and flood-fill
  `(i, j)` labelling. Image-free (no per-cell colour sampling — the
  original paper's image-domain test is replaced by an axis-driven
  cell predicate so `projective-grid` stays standalone). Selectable
  from chessboard-v2 via
  `DetectorParams::graph_build_algorithm = GraphBuildAlgorithm::Topological`;
  default stays `ChessboardV2` because the topological pipeline
  currently regresses recall on ChArUco-style images (marker-internal
  corners poison the per-cell axis test). Runs faster + denser on
  clean PuzzleBoards. ChArUco unconditionally pins `ChessboardV2`
  inside `CharucoDetector::new` regardless of caller choice.
- **Stage 6 local-homography extension.**
  `projective_grid::square::extension::local::extend_via_local_homography`
  fits a per-candidate `H` from the `K` nearest labelled corners (by
  grid Manhattan distance) instead of one global homography. Tolerates
  heavy radial distortion and multi-region perspective where a single
  `H` breaks. Configured via `LocalExtensionParams`; complements the
  pre-existing global-H pass.
- **Shared component-merge.**
  `projective_grid::component_merge::merge_components_local` is the
  post-stage that reunites partial components from either pipeline
  (ChessboardV2 boosters or topological flood-fill) using local
  geometry only — no global homography, so it tolerates heavy radial
  distortion. The chessboard crate's historical
  `enable_component_merge` flag is now backed by this shared
  implementation via `DetectorParams::component_merge: LocalMergeParams`.

### Performance

- **`projective_grid::homography::estimate_homography` rewritten** to
  use normal equations + a 9×9 symmetric eigendecomposition of `Aᵀ A`
  instead of a full `(2N × 9)` SVD on the DLT data matrix. Hartley
  normalisation stays — preconditioning matters more under normal
  equations because cond(`Aᵀ A`) = cond(`A`)² — and the f32 condition
  number stays comfortable on real workloads. The 4-point fast path
  (`homography_from_4pt`, 8×8 LU) is unchanged. Microbench shows 1.8×
  at `N=9`, 2× at `N=25`, 2.6× at `N=100`, 3.3× at `N=225` (the
  speedup grows because the old path's bidiagonalisation is `O(N²)`
  while the new path's `Aᵀ A` accumulation is `O(N)`). Sign of the
  smallest eigenvector is unconstrained; downstream
  `normalize_homography` (divides by `h[(2,2)]`) pins it. Numerical
  contract: agrees with the SVD reference to ≤ 0.01 px in
  pixel-domain on a 1000-sample randomised homography battery; H
  entries themselves can drift up to ~1e-3 relative (the textbook f32
  cond² penalty), but downstream consumers feed `H` through
  `apply(...)` to predict cell positions and never read entries.
  `HomographyQuality::from_homography` stays on the 3×3 SVD path —
  same penalty would apply there too and the consumer is no longer
  in the per-cell hot loop.
- **`extend_via_local_homography` skips the wasted 3×3 quality SVD.**
  The local-extension call site discarded the `HomographyQuality`
  struct (`let Some((h, _)) = ...`) and gates on manually-computed
  reprojection residuals over the K supports. Switched to bare
  `estimate_homography` so each per-cell call drops one nalgebra
  `Matrix3::svd`. No behaviour change.
- **`projective_grid::component_merge::merge_components_local`
  rewritten** as a position-based Hough transform on
  `(transform, label-delta)`. The old anchor-pair enumeration was
  `O(P² Q)` per (component, component) pair and dominated the
  topological pipeline on real images with multiple fragmented
  components. The new implementation indexes one component's positions
  in a KD-tree, queries each label of the other component within
  `pos_tol`, and votes each match into a histogram bin keyed by the
  candidate alignment. Two- to three-orders-of-magnitude wins on
  microbenches; the topological pipeline measured on a representative
  high-resolution chessboard image goes from multi-second to
  tens-of-milliseconds. The original tiebreaker (preferring
  identity-transform matches by iteration order) is preserved as an
  explicit tiebreaker on transform index.
- **`extension::local::nearest_labelled_by_grid` switched from
  full-sort to bounded max-heap.** Allocates once and runs in
  `O(L log K)` per cell instead of `O(L log L)` (`L = labelled
  corners`, `K = k_nearest`). Cuts the overall ChessboardV2 extension
  stage wall-clock by roughly 4× end-to-end on a representative
  high-resolution frame, with the same deterministic
  `(distance, i, j, idx)` ordering downstream callers depend on.
- Combined effect on a 12 MP chessboard frame: full
  `detect_chessboard` runs in tens of milliseconds instead of seconds,
  with zero precision regression on the internal regression set.

### Tooling

- Added an opt-in `tracing` Cargo feature on `projective-grid` (off by
  default). When enabled, the hot-path entry points (`bfs_grow`,
  `build_grid_topological` and its substages,
  `merge_components_local`, `square::validate::validate`,
  `extend_via_local_homography`, `extend_via_global_homography`,
  `extend_from_labelled`, `estimate_global_cell_size`,
  `estimate_local_steps`) emit `tracing::instrument` spans for
  per-call p50/p95 timing.
- Added a `[profile.profiling]` Cargo profile (release with
  line-tables-only debug info) so `samply record` flamegraphs
  symbolicate without doubling binary size.
- `crates/calib-targets/examples/profile_grid.rs` is a thin driver for
  `samply record` and `RUST_LOG=info` tracing dumps.
- `docs/profiling.md` documents the full samply + tracing recipe.
- New criterion microbenches in `crates/projective-grid/benches/`:
  `topological.rs`, `merge.rs`, `validate.rs`. Existing `grow.rs` and
  `homography.rs` benches preserved as the baseline; `homography.rs`
  gains `K=8/12/20` cases mirroring the production
  `LocalExtensionParams` defaults.

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

## [0.6.0]

Coordinated workspace release that ships the new
`calib-targets-puzzleboard` crate. `calib-targets-core` adds the
`TargetKind::PuzzleBoard` variant, which is a non-additive change to a
`#[non_exhaustive]` enum but bumps the workspace minor version anyway so
all crates publish in lockstep at `0.6.0`.

### Added

- Add first-class PuzzleBoard support with a new
  `calib-targets-puzzleboard` crate. The detector samples edge-midpoint code
  dots on a chessboard grid, decodes the embedded 501 x 501 master pattern,
  and returns absolute corner IDs plus target-space positions.
- Add `TargetKind::PuzzleBoard` variant in `calib-targets-core` so the new
  detector can populate `TargetDetection.kind`.
- Add committed PuzzleBoard code-map blobs, generation/verification tools,
  synthetic and real-image regression tests, and generated PuzzleBoard
  testdata.
- Ship the PStelldinger/PuzzleBoard author-canonical `code1`/`code2` maps
  (`map_a.bin` / `map_b.bin`) and a new `import_author_maps.rs` tool so the
  shipped maps match the upstream reference implementation; add
  `tests/interop_authors.rs` to keep the maps byte-compatible.
- Add PuzzleBoard printable target generation through `calib-targets-print`,
  including JSON/SVG/PNG output bundles and Python printable dataclasses.
- Add PuzzleBoard facade helpers, Rust examples, Python bindings, WASM
  bindings, FFI C ABI structs/functions, and regenerated native headers.
- Add PuzzleBoard documentation in the crate README, workspace README,
  mdBook, and release/development command references.
- Add `PuzzleBoardSearchMode::FixedBoard`. Matches observations directly
  against the declared board's own bit pattern (derived from
  `PuzzleBoardSpec` at decode time) under `8 × (rows+1)²` candidate
  shifts, so any partial view of that specific board decodes to the same
  master IDs a full-view decode would produce. Cheaper than `Full` for
  small boards and fast enough for the large ones. Default stays `Full`;
  opt in via `params.decode.search_mode = PuzzleBoardSearchMode::FixedBoard`.
  Mirrored in the Python dataclass and WASM TypeScript types; FFI stays
  on `Full`.
- Add `cargo bench -p calib-targets --bench puzzleboard_sizes` (criterion
  comparison of `Full` vs `FixedBoard` across sizes 6, 8, 10, 12, 13, 16,
  20, 30) and `cargo run --release -p calib-targets --example
  puzzleboard_size_sweep` (per-stage success/failure/timing table used to
  pinpoint which pipeline stage a given board size fails at).
- Overlay every decoded PuzzleBoard edge-bit dot in the WASM demo: sky-blue
  ring around `bit=1` (white puzzle dot), orange ring around `bit=0` (black
  puzzle dot), opacity scaled by per-bit confidence.

### Fixed

- Filter PuzzleBoard decode candidates by bit-error rate before selecting the
  best weighted score, avoiding false negatives when a higher-score candidate
  exceeds the configured error budget.
- Re-check the PuzzleBoard minimum edge count after confidence filtering so
  weak edge samples cannot pass into the decoder as an undersized window.
- Demo dev server no longer 404s on `calib_targets_wasm_bg.wasm` — Vite's
  esbuild pre-bundler was rewriting the JS into `.vite/deps/` without
  copying the sibling `.wasm`, so the `new URL(..., import.meta.url)` fetch
  hit the SPA fallback. Fixed by adding `calib-targets-wasm` to
  `optimizeDeps.exclude`.
- Demo `ResultsPanel` grid readout now reports `max − min + 1` instead of
  `max + 1`, so a 10 × 10 PuzzleBoard no longer displays as "177 × 177"
  (master-grid indices start near 167).
- Demo PuzzleBoard edge-bit overlay now maps `observed_edges` from local to
  master coordinates via the alignment's D4 + translation before looking up
  corners, fixing the previously empty overlay.
- Fix `GridAlignment.transform` TypeScript type in the WASM demo (was
  `string`; actual serde shape is `{a, b, c, d}`).

### Changed

- Demo toolchain switched from `npm` to `bun` (`demo/bun.lock` is the
  committed lockfile; `demo/package-lock.json` removed). CI wasm job now
  uses `oven-sh/setup-bun` + `bun install --frozen-lockfile`.
- `.claude/CLAUDE.md` gains the new bench + diagnostic example commands
  and documents the `bun` switch.

## [0.5.3]

### Fixes

- **Python bindings:** fix `MarkerDetection.gc` deserialization. Rust emits
  `{"i","j"}` (from `GridCoords`), but the Python wrapper was typed as a
  separate `GridCell` dataclass requiring `{"gx","gy"}`, so every
  `detect_charuco` call with markers crashed in `from_dict`. Dropped the
  redundant `GridCell` type; `MarkerDetection.gc` now uses `GridCoords`,
  matching `LabeledCorner.grid` and `CircleCandidate.cell`.
- Added `python_tests/test_detect_roundtrip.py` that runs the real extension
  on repo test images and round-trips result dicts, so Rust/Python dict-key
  drift fails loudly instead of being masked by hand-written fixtures.

## [0.5.2]

### Changed

- **`projective-grid`:** all public types and functions are now generic over
  floating-point type (`f32` / `f64`). All types default to `f32`, so existing
  code compiles unchanged. New `Float` trait alias (`RealField + Copy`) is
  re-exported from the crate root.
- `Homography` internal matrix is now `Matrix3<F>` (previously always `f64`).
  For `f32` users this means slightly less internal precision but no
  cross-type conversions; `f64` users get full double-precision throughout.

## [0.5.1]

### Fixes

- Fix FFI C++ consumer examples: `config.graph.*` → `config.chessboard.graph.*`
  after API redesign nested `GridGraphParams` inside `ChessboardParams`.
- Fix broken intra-doc links (`detect_from_corners`, `min_marker_inliers`).
- Fix `cargo doc` binary name collision by adding `doc = false` to CLI bin.
- Regenerate FFI header and Python typing stubs after `#[non_exhaustive]` changes.
- Add `detect_*_best` sweep functions to Python and WASM bindings.
- Document pre-release quality gates in CLAUDE.md.

## [0.5.0]

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
