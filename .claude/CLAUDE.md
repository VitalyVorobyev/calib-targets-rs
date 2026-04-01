# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Format
cargo fmt --all

# Lint (treat warnings as errors)
cargo clippy --workspace --all-targets -- -D warnings

# Test
cargo test --workspace
cargo test --workspace --all-features

# Docs
cargo doc --workspace --all-features

# Build book
mdbook build book
```

Run a single example:
```bash
cargo run --release --features "tracing" --example detect_chessboard -- testdata/mid.png
cargo run --release --features "tracing" --example detect_charuco -- testdata/small2.png
cargo run --release --features "tracing" --example detect_markerboard -- testdata/markerboard_crop.png
cargo run --release --features "tracing" --example detect_chessboard_best -- testdata/mid.png
cargo run --release --features "tracing" --example detect_charuco_best -- testdata/small2.png
```

Run examples with JSON config (produces detailed JSON reports):
```bash
cargo run --example chessboard -- testdata/chessboard_config.json
cargo run --example charuco_detect -- testdata/charuco_detect_config.json
```

Python bindings (built with `maturin`, managed with `uv`, crate is `crates/calib-targets-py`):
```bash
# Use the existing .venv in the project root — do not create new environments
uv run maturin develop --release -m crates/calib-targets-py/Cargo.toml
uv run python crates/calib-targets-py/examples/detect_chessboard.py path/to/image.png

# Regenerate typing stubs (must pass --check in CI)
uv run python crates/calib-targets-py/tools/generate_typing_artifacts.py

# Run Python tests
uv run pytest crates/calib-targets-py/python_tests/ -v
```

WASM bindings (built with `wasm-pack`, demo at `demo/`):
```bash
# Build WASM package into demo/pkg/
scripts/build-wasm.sh

# Run demo dev server
cd demo && npm install && npm run dev
```

## Architecture

This is a Cargo workspace. All publishable crates live under `crates/`:

| Crate | Role |
|---|---|
| `calib-targets` | Facade crate — `detect_*` and `detect_*_best` helpers, `default_chess_config()` |
| `projective-grid` | Standalone grid graph construction, traversal, homography, and grid smoothness (no image types) |
| `calib-targets-core` | Shared types: `Corner`, `GrayImageView`, `LabeledCorner`, `TargetDetection`; re-exports from `projective-grid` |
| `calib-targets-chessboard` | ChESS feature graph → chessboard grid assembly (uses `projective-grid` for graph/traversal) |
| `calib-targets-aruco` | ArUco/AprilTag dictionary, bit decoding, marker matching |
| `calib-targets-charuco` | ChArUco fusion: grid-first alignment + ArUco anchoring + corner IDs |
| `calib-targets-marker` | Checkerboard + 3-circle marker board layouts and detection |
| `calib-targets-print` | Printable target generation: JSON/SVG/PNG output bundles |
| `calib-targets-ffi` | C ABI bindings with generated header and CMake package (not published) |
| `calib-targets-py` | PyO3/maturin Python bindings (not published to crates.io) |
| `calib-targets-wasm` | wasm-bindgen WebAssembly bindings (not published to crates.io) |
| `calib-targets-cli` | CLI utilities (not published) |

**Dependency rules:** `projective-grid` is standalone (no internal deps). `core` depends on `projective-grid`. `charuco` may depend on `chessboard` and `aruco`. No cyclic deps.

**Detection pipeline** (same structure for all target types):
1. Run `chess-corners` (external crate) to detect ChESS corner features.
2. Build a proximity/orientation graph over corners and assemble a chessboard grid.
3. For ChArUco/marker boards: locally warp candidate cells and decode markers/circles.
4. Output a `TargetDetection` (or wrapping result struct) containing `LabeledCorner` entries.

**Multi-config sweep:** `detect_*_best` functions try multiple parameter configs and return the best result (most markers/corners). Built-in presets: `ChessboardParams::sweep_default()` and `CharucoParams::sweep_for_board()`.

**`TargetDetection` / `LabeledCorner`** — the common output container. Fields: `position`, `grid` (i,j), `id`, `target_position` (board units / mm), `score`. See README for per-target field usage.

**`tracing` feature** — optional, gates all performance-tracing instrumentation across the workspace.

## Key Conventions

**Coordinate system:**
- Image pixels: origin top-left, x right, y down.
- Grid: `i` right, `j` down; indices are corner intersections.
- Quad / homography corner order: **TL, TR, BR, BL** (clockwise). Never use self-crossing order.
- Pixel sampling: use `x + 0.5`, `y + 0.5` for pixel centers consistently.

**Correctness first:** prefer clear correct implementations with tests over micro-optimizations. Mark future optimizations with TODO.

**Marker decoding:** grid-aware scan in rectified space (not generic contour/quad detection). Keep bit packing order, polarity (black=1 or black=0), and `borderBits` explicit in code.

**New warnings:** fix them; do not suppress.

**`#[non_exhaustive]`:** all public enums in published crates are `#[non_exhaustive]`. New match arms in consumer code need wildcard patterns.

**MSRV:** workspace sets `rust-version = "1.88"`. Toolchain pinned to `stable` in `rust-toolchain.toml`.

## Pre-Release Quality Gates

These checks must all pass before tagging a release. Several produce generated
artifacts that go stale when the Rust API surface changes (new functions,
`#[non_exhaustive]`, renamed types, etc.).

```bash
# 1. Standard Rust checks
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --workspace --all-features

# 2. Doc warnings (broken intra-doc links, name collisions)
cargo doc --workspace --no-deps  # must produce zero warnings

# 3. Generated FFI header (stale after any change to FFI-visible types/enums)
cargo run -p calib-targets-ffi --bin generate-ffi-header -- --check

# 4. Python typing stubs (stale after any change to #[pyfunction] or #[pyclass])
uv run maturin develop --release -m crates/calib-targets-py/Cargo.toml
uv run python crates/calib-targets-py/tools/generate_typing_artifacts.py --check

# 5. Python tests
uv run pytest crates/calib-targets-py/python_tests/ -v

# 6. WASM build (if wasm-pack is installed)
scripts/build-wasm.sh
```

**Common pitfall:** changing public enums or structs (e.g. adding
`#[non_exhaustive]`) invalidates both the FFI header and Python typing stubs.
Always regenerate both after such changes.

**Binding API parity:** when adding new public functions to the Rust facade
(`crates/calib-targets/src/detect.rs`), also expose them in:
- Python bindings: `crates/calib-targets-py/src/lib.rs` + `api.py` + `__init__.py`
- WASM bindings: `crates/calib-targets-wasm/src/lib.rs`
