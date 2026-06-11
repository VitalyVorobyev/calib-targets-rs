# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with
code in this repository. It is intentionally short — the long-form guides it
links to live under [`docs/development/`](../docs/development/).

## Everyday gate

Run before every commit (full rationale in
[`docs/development/release-gates.md`](../docs/development/release-gates.md)):

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo doc --workspace --no-deps   # MUST produce zero warnings
```

`cargo doc` zero-warnings is non-negotiable: broken intra-doc links and
ambiguous references (a name that is both a module and a function) are hard
errors here — fix them at source, do not rely on CI.

The full catalogue of build / example / benchmark / Python / WASM / CLI commands
is in [`docs/development/commands.md`](../docs/development/commands.md).

## Architecture

A Cargo workspace; all publishable crates live under `crates/`:

| Crate | Role |
|---|---|
| `calib-targets` | Facade crate — `detect_*` and `detect_*_best` helpers, `default_chess_config()` |
| `projective-grid` | Standalone grid graph construction, traversal, homography, and grid smoothness (no image types) |
| `calib-targets-core` | Shared types: `Corner`, `GrayImageView`, `LabeledCorner`, `TargetDetection`; re-exports from `projective-grid` |
| `calib-targets-chessboard` | Invariant-first chessboard detector on top of ChESS corners (precision-by-construction) |
| `calib-targets-aruco` | ArUco/AprilTag dictionary, bit decoding, marker matching |
| `calib-targets-charuco` | ChArUco fusion: grid-first alignment + ArUco anchoring + corner IDs |
| `calib-targets-puzzleboard` | PuzzleBoard self-identifying chessboard: edge-dot decode + absolute corner IDs |
| `calib-targets-marker` | Checkerboard + 3-circle marker board layouts and detection |
| `calib-targets-print` | Printable target generation: JSON/SVG/PNG output bundles |
| `calib-targets-ffi` | C ABI bindings with generated header and CMake package (not published) |
| `calib-targets-py` | PyO3/maturin Python bindings (not published to crates.io) |
| `calib-targets-wasm` | wasm-bindgen WebAssembly bindings (published to npm as `@vitavision/calib-targets`) |

**CLI**: the `calib-targets` binary lives in the facade crate behind the default
`cli` feature (`cargo install calib-targets`); the Python package exposes the
same subcommands as a console script via `[project.scripts]`.

**Dependency rules:** `projective-grid` is standalone (no internal deps). `core`
depends on `projective-grid`. `charuco` may depend on `chessboard` and `aruco`.
No cyclic deps.

**Detection pipeline** (same structure for all target types):

1. Run `chess-corners` (external crate) to detect ChESS corner features.
2. Build a proximity/orientation graph over corners and assemble a chessboard
   grid. Two algorithms sit behind `DetectorParams::graph_build_algorithm` —
   `Topological` (default) and `SeedAndGrow` (pinned for ChArUco). See the
   pipeline guide below.
3. For ChArUco/marker boards: locally warp candidate cells and decode
   markers/circles.
4. For PuzzleBoard: sample edge-midpoint dots and decode the master edge-code
   pattern.
5. Output a `TargetDetection` (or wrapping result struct) containing
   `LabeledCorner` entries.

**Multi-config sweep:** `detect_*_best` functions try multiple parameter configs
and return the best result (most markers/corners). Presets:
`ChessboardParams::sweep_default()`, `CharucoParams::sweep_for_board()`,
`PuzzleBoardParams::sweep_for_board()`.

**`TargetDetection` / `LabeledCorner`** — the common output container. Fields:
`position`, `grid` (i,j), `id`, `target_position` (board units / mm), `score`.
See README for per-target field usage.

**`tracing` feature** — optional, gates all performance-tracing instrumentation
across the workspace.

## Detailed guides

Read the relevant guide before touching that area:

- [`docs/development/detection-pipeline.md`](../docs/development/detection-pipeline.md)
  — graph-build algorithm selection (Topological vs SeedAndGrow + ChArUco
  pinning), component merge, orientation source, bench harness selector, the
  axes-only corner-orientation contract, and the cell-size-estimation gotcha.
- [`docs/development/debugging.md`](../docs/development/debugging.md)
  — the **mandatory** evidence-driven protocol for any detector failure.
- [`docs/development/conventions.md`](../docs/development/conventions.md)
  — public struct conventions (`#[non_exhaustive]` + named constructors),
  binding/CLI/dict-key parity, and local-only-artifact rules.
- [`docs/development/private-dataset-policy.md`](../docs/development/private-dataset-policy.md)
  — disclosure policy + the two regression datasets (3536119669, 130x130_puzzle).
- [`docs/development/release-gates.md`](../docs/development/release-gates.md)
  — full pre-release quality-gate checklist.
- [`docs/development/commands.md`](../docs/development/commands.md)
  — complete command reference.

## Key conventions (always on)

**Coordinate system:**
- Image pixels: origin top-left, x right, y down.
- Grid: `i` right, `j` down; indices are corner intersections.
- Quad / homography corner order: **TL, TR, BR, BL** (clockwise). Never use
  self-crossing order.
- Pixel sampling: use `x + 0.5`, `y + 0.5` for pixel centers consistently.

**Grid labels are non-negative.** Every detector that returns
`LabeledCorner { grid: Some(i, j) }` MUST rebase `(i, j)` so the labelled
bounding-box minimum is `(0, 0)`. Hard invariant for overlay / calibration
consumers; for `seed-and-grow` it is enforced inside `grow::grow_from_seed`.

**Corner orientation is axes-only.** `Corner::orientation` has been removed
workspace-wide — never reintroduce it. The only signal is
`Corner.axes: [AxisEstimate; 2]`; any circular mean of axis angles MUST
accumulate `(cos 2θ, sin 2θ)`. Full contract in the
[detection-pipeline guide](../docs/development/detection-pipeline.md#corner-orientation-contract-axes-only).

**Evidence-driven debugging.** Every detector-failure conclusion must be tied to
a measured number or a verifiable spatial fact — never a plausible narrative.
`bench check` `pos=` does NOT validate new `(i, j)` labels; overlays + an
independent geometry check are mandatory. Full protocol in
[`docs/development/debugging.md`](../docs/development/debugging.md).

**Private dataset disclosure.** Never cite a private regression dataset (counts,
filenames, hashes, frame ids) in any public surface — READMEs, `book/src/`,
CHANGELOG, rustdoc, commit messages on `main`, PRs. General performance
statements only. Concrete numbers live only in local-only surfaces. Full policy
in [`docs/development/private-dataset-policy.md`](../docs/development/private-dataset-policy.md).

**Public structs/enums:** `#[non_exhaustive]` + a named constructor on every
public type in a published crate. New match arms in consumer code need wildcard
patterns. Details in
[`docs/development/conventions.md`](../docs/development/conventions.md).

**Correctness first:** prefer clear correct implementations with tests over
micro-optimizations. Mark future optimizations with TODO.

**Marker decoding:** grid-aware scan in rectified space (not generic
contour/quad detection). Keep bit packing order, polarity (black=1 or black=0),
and `borderBits` explicit in code.

**New warnings:** fix them; do not suppress.

**No `#[allow(clippy::…)]` in production code.** If a clippy lint fires, fix the
underlying issue — extract a param/context struct, split the function, or refine
the design. The only acceptable file-scope allows are: (a) generated code
(`include!`-style outputs), and (b) `#![allow(non_camel_case_types)]` on FFI
crates for C ABI naming. Any new `#[allow(...)]` attribute requires explicit
review approval. Workspace-level clippy lints are enforced via
`[workspace.lints]` in the root `Cargo.toml` and `[lints] workspace = true` in
every crate — `too_many_arguments` is `deny` and must not be re-introduced via
an inline allow.

**Local-only artifacts — never commit.** `bench_results/`, rendered overlays,
per-frame JSONLs, aggregate JSONs, profiling dumps, sweep CSVs, and similar
generated data stay out of Git. Stage files individually; never `git add -A` in
a directory that may contain them. Details in
[`docs/development/conventions.md`](../docs/development/conventions.md#local-only-artifacts--never-commit).

**MSRV:** workspace sets `rust-version = "1.88"`. Toolchain pinned to `stable`
in `rust-toolchain.toml`.
