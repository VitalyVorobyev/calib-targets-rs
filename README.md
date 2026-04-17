# calib-targets-rs

[![CI](https://github.com/VitalyVorobyev/calib-targets-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/VitalyVorobyev/calib-targets-rs/actions/workflows/ci.yml)
[![Security audit](https://github.com/VitalyVorobyev/calib-targets-rs/actions/workflows/audit.yml/badge.svg)](https://github.com/VitalyVorobyev/calib-targets-rs/actions/workflows/audit.yml)
[![Docs](https://github.com/VitalyVorobyev/calib-targets-rs/actions/workflows/docs.yml/badge.svg)](https://vitalyvorobyev.github.io/calib-targets-rs/)
[![MSRV](https://img.shields.io/badge/MSRV-1.88-blue.svg)](https://blog.rust-lang.org/2025/06/26/Rust-1.88.0/)

Calibration target detection in Rust (chessboard, ChArUco, PuzzleBoard, ArUco/AprilTag, marker boards).

![ChArUco detection overlay](book/img/charuco_detect_report_small2_overlay.png)

> **Status:** Feature-complete, APIs may change.

[Full documentation (book)](https://vitalyvorobyev.github.io/calib-targets-rs/) |
[API reference](https://vitalyvorobyev.github.io/calib-targets-rs/api) |
[Getting Started tutorial](https://vitalyvorobyev.github.io/calib-targets-rs/getting-started.html)

## Overview

Detection is built on the [ChESS corners](https://github.com/VitalyVorobyev/chess-corners-rs) detector. All target types share the same pipeline: build a graph over ChESS features, select connected components, then optionally decode markers in locally warped cells. The local nature of the algorithm makes it robust to lens distortion. Default parameters work in most practical cases.

| Target | When to use |
|---|---|
| **Chessboard** | Simplest option; no markers needed |
| **ChArUco** | Recommended for calibration — partial views OK, unique corner IDs |
| **PuzzleBoard** | Self-identifying chessboard; partial views with dense absolute corner IDs |
| **Marker board** | Specialised layouts with 3-circle markers |

## Quickstart

### Rust

```bash
cargo add calib-targets image
```

```rust,no_run
use calib_targets::detect;
use calib_targets::ChessboardParams;
use image::ImageReader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let img = ImageReader::open("board.png")?.decode()?.to_luma8();
    let params = ChessboardParams::default();

    match detect::detect_chessboard(&img, &params) {
        Some(found) => println!("{} corners", found.detection.corners.len()),
        None => println!("no board detected"),
    }
    Ok(())
}
```

For challenging images, sweep multiple configs and keep the best result:

```rust,no_run
use calib_targets::chessboard::ChessboardParams;
use calib_targets::detect;
# use image::ImageReader;

# fn main() -> Result<(), Box<dyn std::error::Error>> {
# let img = ImageReader::open("board.png")?.decode()?.to_luma8();
let configs = ChessboardParams::sweep_default(); // three threshold presets
let result = detect::detect_chessboard_best(&img, &configs);
# Ok(())
# }
```

ChArUco and marker board follow the same pattern — see
[detect_charuco](crates/calib-targets/examples/detect_charuco.rs),
[detect_charuco_best](crates/calib-targets/examples/detect_charuco_best.rs),
[detect_markerboard](crates/calib-targets/examples/detect_markerboard.rs),
[detect_puzzleboard](crates/calib-targets/examples/detect_puzzleboard.rs),
and [detect_puzzleboard_best](crates/calib-targets/examples/detect_puzzleboard_best.rs).

### Python

```bash
pip install calib-targets numpy Pillow
```

```python
import numpy as np
from PIL import Image
import calib_targets as ct

image = np.asarray(Image.open("board.png").convert("L"), dtype=np.uint8)

# Single config
result = ct.detect_chessboard(image)
if result is not None:
    print(f"{len(result.detection.corners)} corners")

# Multi-config sweep
result = ct.detect_chessboard_best(image, [
    ct.ChessboardParams(),
    ct.ChessboardParams(chess=ct.ChessConfig(threshold_value=0.15)),
    ct.ChessboardParams(chess=ct.ChessConfig(threshold_value=0.08)),
])
```

All target types and multi-config sweep are available: `detect_chessboard`, `detect_charuco`, `detect_marker_board`, `detect_puzzleboard`, plus `detect_chessboard_best`, `detect_charuco_best`, `detect_marker_board_best`, `detect_puzzleboard_best`.

See [examples](crates/calib-targets-py/examples/) for full usage.

### Printable targets

Generate, print, detect:

```python
import calib_targets as ct
doc = ct.PrintableTargetDocument(
    target=ct.CharucoTargetSpec(rows=5, cols=7, square_size_mm=20.0,
                                marker_size_rel=0.75, dictionary="DICT_4X4_50")
)
ct.write_target_bundle(doc, "my_board/charuco_a4")  # .json + .svg + .png
```

Print the SVG at 100% scale. See the [printable-target guide](./book/src/printable.md).

### WebAssembly

Browser-ready WASM bindings (~195 KB gzipped) with a React demo app:

```bash
scripts/build-wasm.sh
cd demo && bun install && bun run dev
```

## Crates

| Crate | crates.io | Role |
|---|---|---|
| [`calib-targets`](crates/calib-targets) | [yes](https://crates.io/crates/calib-targets) | Facade: end-to-end `detect_*` and `detect_*_best` helpers |
| [`projective-grid`](crates/projective-grid) | [yes](https://crates.io/crates/projective-grid) | Standalone grid graph, traversal, homography |
| [`calib-targets-core`](crates/calib-targets-core) | [yes](https://crates.io/crates/calib-targets-core) | Shared types: `Corner`, `LabeledCorner`, `TargetDetection` |
| [`calib-targets-chessboard`](crates/calib-targets-chessboard) | [yes](https://crates.io/crates/calib-targets-chessboard) | Chessboard detector |
| [`calib-targets-aruco`](crates/calib-targets-aruco) | [yes](https://crates.io/crates/calib-targets-aruco) | ArUco/AprilTag dictionaries and decoding |
| [`calib-targets-charuco`](crates/calib-targets-charuco) | [yes](https://crates.io/crates/calib-targets-charuco) | ChArUco alignment and IDs |
| [`calib-targets-puzzleboard`](crates/calib-targets-puzzleboard) | [yes](https://crates.io/crates/calib-targets-puzzleboard) | PuzzleBoard self-identifying chessboard detection |
| [`calib-targets-marker`](crates/calib-targets-marker) | [yes](https://crates.io/crates/calib-targets-marker) | Checkerboard + 3-circle marker boards |
| [`calib-targets-print`](crates/calib-targets-print) | [yes](https://crates.io/crates/calib-targets-print) | Printable target generation (JSON/SVG/PNG) |
| `calib-targets-py` | no | Python bindings (PyO3/maturin) |
| `calib-targets-wasm` | no | WebAssembly bindings |
| `calib-targets-ffi` | no | C ABI bindings ([docs](./docs/ffi/README.md)) |
| `calib-targets-cli` | no | CLI utilities |

## Development

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-features
cargo doc --workspace --all-features
```

## Diligence Statement

This project is developed with AI coding assistants (`Codex` and `Claude Code`) as implementation tools. The project author is an expert in computer vision, validates algorithmic behavior and numerical results, and enforces quality gates before release.

## License

Dual-licensed under MIT or Apache-2.0, at your option. See `LICENSE` and `LICENSE-APACHE`.
