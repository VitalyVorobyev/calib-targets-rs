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
```

Run examples with JSON config (produces detailed JSON reports):
```bash
cargo run --example chessboard -- testdata/chessboard_config.json
cargo run --example charuco_detect -- testdata/charuco_detect_config.json
```

Python bindings (built with `maturin`, crate is `crates/calib-targets-py`):
```bash
pip install maturin
maturin develop  # from repo root or crates/calib-targets-py
python crates/calib-targets-py/examples/detect_chessboard.py path/to/image.png
```

## Architecture

This is a Cargo workspace. All publishable crates live under `crates/`:

| Crate | Role |
|---|---|
| `calib-targets` | Facade crate — end-to-end `detect_*` helpers, `default_chess_config()` |
| `calib-targets-core` | Shared types: `Corner`, `GrayImageView`, `LabeledCorner`, `TargetDetection`, grid coords |
| `calib-targets-chessboard` | ChESS feature graph → chessboard grid assembly |
| `calib-targets-aruco` | ArUco/AprilTag dictionary, bit decoding, marker matching |
| `calib-targets-charuco` | ChArUco fusion: grid-first alignment + ArUco anchoring + corner IDs |
| `calib-targets-marker` | Checkerboard + 3-circle marker board layouts and detection |
| `calib-targets-py` | PyO3/maturin Python bindings (not published to crates.io) |

**Dependency rules:** `core` has no internal deps. `charuco` may depend on `chessboard` and `aruco`. No cyclic deps.

**Detection pipeline** (same structure for all target types):
1. Run `chess-corners` (external crate) to detect ChESS corner features.
2. Build a proximity/orientation graph over corners and assemble a chessboard grid.
3. For ChArUco/marker boards: locally warp candidate cells and decode markers/circles.
4. Output a `TargetDetection` (or wrapping result struct) containing `LabeledCorner` entries.

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

**MSRV:** workspace sets `rust-version = "1.70"`. Toolchain pinned to `stable` in `rust-toolchain.toml`.
