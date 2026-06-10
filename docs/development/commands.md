# Command reference

The everyday gate (format, lint, test, docs) lives in `.claude/CLAUDE.md`.
This file is the full catalogue of build, example, benchmark, and binding
commands for the workspace.

## Core gate

```bash
# Format
cargo fmt --all

# Lint (treat warnings as errors)
cargo clippy --workspace --all-targets -- -D warnings

# Test
cargo test --workspace
cargo test --workspace --all-features

# Docs — MUST produce zero warnings; run before every commit
cargo doc --workspace --no-deps

# Build book
mdbook build book
```

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

Run examples with JSON config (produces detailed JSON reports):

```bash
cargo run --example chessboard -- testdata/chessboard_config.json
cargo run --example charuco_detect -- testdata/charuco_detect_config.json
cargo run -p calib-targets --example detect_puzzleboard -- testdata/puzzleboard_detect_config.json
```

## Benchmarks + diagnostics

```bash
# Criterion: PuzzleBoard detection timing across board sizes (Full vs KnownOrigin fast path)
cargo bench -p calib-targets --bench puzzleboard_sizes

# Per-size success/failure/per-stage-timing table — useful for diagnosing which stage fails
cargo run --release -p calib-targets --example puzzleboard_size_sweep
```

The grid bench harness (`calib-targets-bench`) selector is documented in
[detection-pipeline.md](detection-pipeline.md#bench-harness-selector).

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
