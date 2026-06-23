# `docs/` — internal developer reference

This tree is the **internal / developer** documentation for the
`calib-targets-rs` workspace: design notes, algorithm deep-dives, process
guides, and release history.

> **User-facing docs live elsewhere.** The polished, published documentation is
> the mdBook under [`../book/src/`](../book/src/) (getting started, tuning,
> per-crate guides, examples). Keep the two distinct: when in doubt, *how a
> consumer uses the library* → book; *how the algorithms work and how we develop
> them* → here.

## Map

### `architecture/` — start here: how the whole stack fits together
A cross-cutting map of the detection stack — the algorithm atlas, per-detector
pipeline maps, the crate-layering view, a critical review, and a ranked cleanup
backlog. The best entry point when onboarding or auditing the detector crates.

| File | What it covers |
|---|---|
| [`architecture/README.md`](architecture/README.md) | Entry map + the one-screen verdict + the keep-it-current rule. |
| [`architecture/algorithm-atlas.md`](architecture/algorithm-atlas.md) | Every atomic algorithm + the algorithm×pipeline matrix. |
| [`architecture/pipeline-maps.md`](architecture/pipeline-maps.md) | Each detector stage-by-stage (algorithm, local vs delegated). |
| [`architecture/dependency-and-layering.md`](architecture/dependency-and-layering.md) | Crate DAG, layering, the library-only surface. |
| [`architecture/critique.md`](architecture/critique.md) | Critical review: duplication/debt findings + the from-scratch verdict. |
| [`architecture/chore-backlog.md`](architecture/chore-backlog.md) | Ranked, effort/risk-tagged consolidation backlog. |

### `development/` — developer guides
The everyday gates and conventions. These are the guides linked from the
project [`CLAUDE.md`](../.claude/CLAUDE.md).

| File | What it covers |
|---|---|
| [`detection-pipeline.md`](development/detection-pipeline.md) | Topological grid builder, component merge, orientation source, axes-only contract, cell-size gotcha. |
| [`debugging.md`](development/debugging.md) | Mandatory evidence-driven protocol for any detector failure. |
| [`conventions.md`](development/conventions.md) | Public-struct conventions, binding/CLI/dict-key parity, local-only-artifact rules. |
| [`private-dataset-policy.md`](development/private-dataset-policy.md) | Disclosure policy + the two private regression datasets. |
| [`release-gates.md`](development/release-gates.md) | Full pre-release quality-gate checklist. |
| [`refactor-gates.md`](development/refactor-gates.md) | Standing per-phase validation gate for in-flight refactors. |
| [`commands.md`](development/commands.md) | Complete build / example / bench / binding / CLI command reference. |
| [`profiling.md`](development/profiling.md) | Flamegraph / per-span timing capture for the grid-build pipeline. |
| [`performance.md`](development/performance.md) | Benchmark methodology, current bottleneck ranking, and the prioritized optimization backlog. |
| [`subagent-workflow.md`](development/subagent-workflow.md) | Dispatching quick-/deep-implementer subagents during feature work. |

### `algorithms/` — algorithm deep-dives & design notes

| File | What it covers |
|---|---|
| [`topological-grid-detection.md`](algorithms/topological-grid-detection.md) | Canonical stage-by-stage map for the sole grid builder (the book's `algo_topological_grid` summarises this). |
| [`algorithmic_gaps.md`](algorithms/algorithmic_gaps.md) | Workspace-wide ledger of open/closed algorithmic gaps and known limitations. |
| [`puzzle_detection_spec.md`](algorithms/puzzle_detection_spec.md) | PuzzleBoard soft-edge-decode + global-inference design. |
| [`charuco_concept.md`](algorithms/charuco_concept.md) | ChArUco board-level hypothesis-scoring concept. |
| [`diskfit-antipodal-sector.md`](algorithms/diskfit-antipodal-sector.md) | Upstream `chess-corners` DiskFit axis-slot inversion defect note. |

### `ffi/` — C / C++ consumer docs

| File | What it covers |
|---|---|
| [`README.md`](ffi/README.md) | C API guide: headers, build, CMake staging, support boundaries, ABI shape. |
| [`cmake-consumer-quickstart.md`](ffi/cmake-consumer-quickstart.md) | Shortest-path CMake integration with a minimal C++ example. |
| [`decision-record.md`](ffi/decision-record.md) | Accepted ADR for the FFI ABI choices. |

### Release history
- [`changelog/`](changelog/) — archived per-minor-version release notes
  (`0.1.x` … `0.9.x`), indexed from the root [`CHANGELOG.md`](../CHANGELOG.md).
- [`migrations/0.10.0.md`](migrations/0.10.0.md) — the live 0.10.0 breaking-change
  migration guide (pulled into the book via `{{#include}}`; **do not move/rename**).

## Local-only (untracked) contents

These directories are **gitignored** and hold private / generated data — never
commit them and never cite their contents in any public surface (see
[`development/private-dataset-policy.md`](development/private-dataset-policy.md)):

- `datasets/` — private regression-dataset notes (counts, hashes, frame ids).
- `img/02-topo-grid/`, `img/130x130_puzzle/` — rendered pipeline overlays.

(`img/target_gallery.png` is a tracked asset used by the top-level README.)
