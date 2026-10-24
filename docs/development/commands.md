# Command reference

The everyday gate (format, lint, test, docs) lives in `.claude/CLAUDE.md`.
This file is the full catalogue of build, example, benchmark, and binding
commands for the workspace.

## Core gate

The everyday gate (`cargo fmt --all --check`, clippy with `-D warnings`,
`cargo test --workspace`, zero-warning `cargo doc --workspace --no-deps`) is
defined once, in the "Everyday gate" section of `.claude/CLAUDE.md`; the full
pre-release checklist is [release-gates.md](release-gates.md). Extras:

```bash
cargo test --workspace --all-features
mdbook build book
```

## Cargo aliases

Defined in `.cargo/config.toml`:

| Alias | Expands to |
|---|---|
| `cargo bench-run` | `cargo run --release -p calib-targets-bench --bin bench --` (any bench subcommand) |
| `cargo bench-check` | `... bench -- check` — regression check against the pinned baselines |
| `cargo bench-bless` | `... bench -- bless` — re-bless baselines |
| `cargo bench-preview` | `... bench -- preview` — render overlays |
| `cargo studio` | `cargo run --release -p calib-targets-studio --` — launch the detection studio |
| `cargo xtask` | `cargo run --package xtask --` |

## Run a single example

```bash
cargo run --release --features "tracing" --example detect_chessboard -- testdata/mid.png
cargo run --release --features "tracing" --example detect_charuco -- testdata/small2.png
cargo run --release --features "tracing" --example detect_markerboard -- testdata/markerboard_crop.png
cargo run --release --features "tracing" --example detect_puzzleboard -- testdata/puzzleboard_mid.png
cargo run --release --features "tracing" --example detect_chessboard_best -- testdata/mid.png
cargo run --release --features "tracing" --example detect_charuco_best -- testdata/small2.png
cargo run --release --features "tracing" --example detect_puzzleboard_best -- testdata/puzzleboard_small.png
```

The ChArUco example additionally accepts a JSON config and writes a detailed
JSON report:

```bash
cargo run -p calib-targets-charuco --example charuco_detect -- testdata/charuco_detect_config.json
```

## Benchmarks + diagnostics

```bash
# Full performance campaign (end-to-end + per-stage + criterion + flamegraphs)
bash scripts/run-perf-campaign.sh

# Whole-workspace criterion micro-benches
cargo bench --workspace

# Criterion: PuzzleBoard detection timing across board sizes (Full vs KnownOrigin fast path)
cargo bench -p calib-targets --bench puzzleboard_sizes
```

The grid bench harness (`calib-targets-bench`) selector is documented in
[detection-pipeline.md](detection-pipeline.md#bench-harness-selector); profiling
how-to + the bottleneck ranking live in [profiling.md](profiling.md) and
[performance.md](performance.md).

## Python bindings

Built with `maturin`, managed with `uv`, crate is `crates/calib-targets-py`.

```bash
# Use the existing .venv in the project root — do not create new environments
uv run maturin develop --release -m crates/calib-targets-py/Cargo.toml
uv run python crates/calib-targets-py/examples/detect_chessboard.py path/to/image.png

# Regenerate typing stubs (must pass --check in CI)
uv run python crates/calib-targets-py/tools/generate_typing_artifacts.py

# Run Python tests
uv run pytest crates/calib-targets-py/python_tests/ -v
```

## WASM bindings

Built with `wasm-pack`, demo at `demo/`.

```bash
# Build WASM package into demo/pkg/
scripts/build-wasm.sh

# Run demo dev server (use bun, not npm — the demo's lockfile is bun.lock)
cd demo && bun install && bun run dev
```

## Frontends (`demo/`, `studio/`)

Both follow the vitavision toolchain baseline: compiler options from
`@vitavision/config-ts`, bun pinned in `packageManager`, and UI built on
`@vitavision/ui` with Tailwind 4. The demo is on the current baseline
(TypeScript 7, `oxlint` with `@vitavision/config-oxlint`, `oxfmt`) and also uses
`@vitavision/stage2d`, `overlays`, `workbench` and `forms`; see `demo/README.md`.
The studio still lints with ESLint (see `studio/README.md`, "Design system").

```bash
# demo/
bun run check          # tsc -b
bun run lint           # oxlint --max-warnings 0
bun run fmt:check      # oxfmt --check
bun run build && bun run test:e2e   # functional Playwright smoke suite (what CI runs)
bun run test:screens   # local Playwright screenshots against an uncommitted baseline

# studio/
bun run check          # tsc -b
bun run lint           # eslint, 0 errors (React Compiler rules are warnings for now)
bun run test:screens   # Playwright screenshots against a local, uncommitted baseline
```

`test:screens` in `studio/` needs `cargo build --release -p calib-targets-studio`
first and starts the server itself. Capture the baseline with
`--update-snapshots` before a change and compare after it; the baselines stay
on the machine (the studio's dataset browser lists private entries).

## Printable-target CLI

The `calib-targets` binary lives in the facade crate behind the default `cli`
feature, and is mirrored in the Python package via `[project.scripts]`.

```bash
# One-step generation (flags → JSON + SVG + PNG bundle)
cargo run -p calib-targets --features cli --bin calib-targets -- \
    gen puzzleboard --rows 8 --cols 10 --square-size-mm 15 --out-stem /tmp/puzzle

# Python console script (after `maturin develop`)
uv run calib-targets gen chessboard --inner-rows 6 --inner-cols 8 \
    --square-size-mm 20 --out-stem /tmp/board
```
