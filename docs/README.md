# `docs/` — internal developer reference

This tree is the **internal / developer** documentation for the
`calib-targets-rs` workspace: design notes, algorithm deep-dives, process
guides, and release history. It is the complete map — every internal doc is
listed here.

> **User-facing docs live elsewhere.** The published documentation is the mdBook
> under [`../book/src/`](../book/src/) (getting started, tuning, per-crate
> guides, examples). Keep the two distinct: *how a consumer uses the library* →
> book; *how the algorithms work and how we develop them* → here.

## `architecture/` — how the whole stack fits together

The best entry point when onboarding or auditing the detector crates.

| File | What it covers |
|---|---|
| [`architecture/README.md`](architecture/README.md) | Entry map, the one-screen verdict, the keep-it-current rule. |
| [`architecture/algorithm-atlas.md`](architecture/algorithm-atlas.md) | Every atomic algorithm + the algorithm × pipeline matrix. |
| [`architecture/pipeline-maps.md`](architecture/pipeline-maps.md) | The shared grid spine + index of the per-crate stage maps. |
| [`architecture/dependency-and-layering.md`](architecture/dependency-and-layering.md) | Crate DAG, layering, the library-only surface. |

## Per-crate design docs

| File | What it covers |
|---|---|
| [`calib-targets-chessboard/docs/PIPELINE.md`](../crates/calib-targets-chessboard/docs/PIPELINE.md) | Chessboard stage table (the reference pipeline). |
| [`calib-targets-charuco/docs/PIPELINE.md`](../crates/calib-targets-charuco/docs/PIPELINE.md) | ChArUco stage table. |
| [`calib-targets-puzzleboard/docs/PIPELINE.md`](../crates/calib-targets-puzzleboard/docs/PIPELINE.md) | PuzzleBoard + PuzzlePole stage table. |
| [`calib-targets-marker/docs/PIPELINE.md`](../crates/calib-targets-marker/docs/PIPELINE.md) | Marker-board stage table. |
| [`calib-targets-puzzleboard/docs/SYNTHETIC_AUTHOR_LIKE.md`](../crates/calib-targets-puzzleboard/docs/SYNTHETIC_AUTHOR_LIKE.md) | The public synthetic PuzzleBoard decode fixtures. |
| [`projective-grid/docs/DESIGN.md`](../crates/projective-grid/docs/DESIGN.md) | `projective-grid` design: lattice × strategy × evidence axes, module tree, invariants. |
| [`projective-grid/docs/ORIENTATION.md`](../crates/projective-grid/docs/ORIENTATION.md) | Where orientation enters the grid builder and how it runs without it. |
| [`projective-grid/ROADMAP-1.0.md`](../crates/projective-grid/ROADMAP-1.0.md) | What stands between `projective-grid` and 1.0. |

## `development/` — developer guides

| File | What it covers |
|---|---|
| [`detection-pipeline.md`](development/detection-pipeline.md) | Topological grid builder, component merge, orientation source, axes-only contract, cell-size gotcha. |
| [`debugging.md`](development/debugging.md) | Mandatory evidence-driven protocol for any detector failure. |
| [`conventions.md`](development/conventions.md) | Public-struct conventions, binding/CLI/dict-key parity, local-only-artifact rules. |
| [`private-dataset-policy.md`](development/private-dataset-policy.md) | Disclosure policy + the two private regression datasets. |
| [`release-gates.md`](development/release-gates.md) | Every gate list: label-moving-change regression gate + bless protocol, pre-release checks, MSRV and release procedure. |
| [`commands.md`](development/commands.md) | Build / example / bench / binding / CLI command reference. |
| [`profiling.md`](development/profiling.md) | Flamegraph / per-span timing capture for the grid-build pipeline. |
| [`performance.md`](development/performance.md) | Benchmark methodology, current bottleneck ranking, optimization backlog. |
| [`backlog.md`](development/backlog.md) | Open engineering items that are not detector algorithms (bindings, plumbing, packaging). |
| [`subagent-workflow.md`](development/subagent-workflow.md) | Dispatching quick-/deep-implementer subagents. |

## `algorithms/` — algorithm deep-dives

| File | What it covers |
|---|---|
| [`topological-grid-detection.md`](algorithms/topological-grid-detection.md) | Canonical stage-by-stage map of the sole grid builder. |
| [`algorithmic_gaps.md`](algorithms/algorithmic_gaps.md) | Ledger of open algorithmic gaps and known limits. |
| [`puzzle_detection_spec.md`](algorithms/puzzle_detection_spec.md) | PuzzleBoard soft-edge-decode + global-inference design. |
| [`puzzlepole_construction.md`](algorithms/puzzlepole_construction.md) | PuzzlePole cylindrical construction, decoder cost, coordinate conventions. |

## `ffi/` — C / C++ consumer docs

| File | What it covers |
|---|---|
| [`ffi/README.md`](ffi/README.md) | C API guide: headers, build, CMake staging, ABI versioning, diagnostics JSON. |
| [`ffi/cmake-consumer-quickstart.md`](ffi/cmake-consumer-quickstart.md) | Shortest-path CMake integration with a minimal C++ example. |
| [`ffi/decision-record.md`](ffi/decision-record.md) | Accepted ADR for the FFI ABI choices. |

## Release history

- [`changelog/`](changelog/) — archived per-minor-version release notes,
  indexed from the root [`CHANGELOG.md`](../CHANGELOG.md).
- [`migrations/`](migrations/) — breaking-change migration guides.
  [`0.16.0.md`](migrations/0.16.0.md) and [`0.15.0.md`](migrations/0.15.0.md)
  are pulled into the book's Migration chapter via `{{#include}}` — **do not
  move/rename** them; older guides stay on disk but are not included.
- [`releases/`](releases/) — GitHub release bodies consumed by CI.
  [`ffi-c-api-release-draft.md`](releases/ffi-c-api-release-draft.md) is the
  `body_path` of the native-FFI archive release, so it tracks the **C ABI**
  version, which is independent of the workspace version.

## Local-only contents

Never commit these and never cite their contents in a public surface (see
[`development/private-dataset-policy.md`](development/private-dataset-policy.md)):

- `datasets/` (gitignored) — private regression-dataset notes.
- `img/` subdirectories — rendered pipeline overlays (untracked).
  `img/target_gallery.png` is the one tracked asset, used by the top-level README.
