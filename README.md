# calib-targets-rs

[![CI](https://github.com/VitalyVorobyev/calib-targets-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/VitalyVorobyev/calib-targets-rs/actions/workflows/ci.yml)
[![Security audit](https://github.com/VitalyVorobyev/calib-targets-rs/actions/workflows/audit.yml/badge.svg)](https://github.com/VitalyVorobyev/calib-targets-rs/actions/workflows/audit.yml)
[![Docs](https://github.com/VitalyVorobyev/calib-targets-rs/actions/workflows/docs.yml/badge.svg)](https://vitalyvorobyev.github.io/calib-targets-rs/)
[![MSRV](https://img.shields.io/badge/MSRV-1.88-blue.svg)](https://blog.rust-lang.org/2025/06/26/Rust-1.88.0/)

Calibration target detection in Rust (chessboard, ChArUco, ArUco/AprilTag, marker boards).

![ChArUco detection overlay](book/img/charuco_detect_report_small2_overlay.png)

> **Status:** Feature-complete, APIs may change.

→ **[Full documentation (book)](https://vitalyvorobyev.github.io/calib-targets-rs/)** — tuning guide, troubleshooting, output reference, and more.

## Introduction

Target detection is built on top of the [ChESS corners](https://github.com/VitalyVorobyev/chess-corners-rs) detector. All target types share the same chessboard-style pipeline: build a graph over ChESS features and select connected components. The local nature of the algorithm makes it robust to lens distortion. Detection of calibration target features (ArUco markers or circles) uses a local projective warp, which avoids heavy pattern matching while remaining robust and fast. Each algorithms has parameters, but default setup should work in most of practical cases.

## Diligence Statement

This project is developed with AI coding assistants (`Codex` and `Claude Code`) as implementation tools. Not every code path is manually line-reviewed by a human before merge. The project author is an expert in computer vision, validates algorithmic behavior and numerical results, and enforces quality gates (`fmt`/`clippy`/tests/docs/Python checks) before release. This is engineering-assisted development, not "vibe coding."

## Getting started

The workflow is: **generate a target → print it → detect it.**

| Target | When to use |
|---|---|
| **Chessboard** | Simplest option; no markers needed |
| **ChArUco** | Recommended for real calibration — partial views OK, each corner has a unique ID |
| **Marker board** | Specialised layouts with 3-circle markers |

**Generate** a ChArUco board (Python — simplest path):

```bash
pip install calib-targets
```

```python
import calib_targets as ct
doc = ct.PrintableTargetDocument(
    target=ct.CharucoTargetSpec(rows=5, cols=7, square_size_mm=20.0,
                                marker_size_rel=0.75, dictionary="DICT_4X4_50")
)
ct.write_target_bundle(doc, "my_board/charuco_a4")  # → .json + .svg + .png
```

**Print** `my_board/charuco_a4.svg` at 100% scale ("actual size" — disable "fit to page").
Measure one square with a ruler to confirm it is 20 mm.

**Detect:**

```python
import numpy as np
from PIL import Image
import calib_targets as ct

image = np.asarray(Image.open("frame.png").convert("L"), dtype=np.uint8)

board = ct.CharucoBoardSpec(rows=5, cols=7, cell_size=20.0,
                            marker_size_rel=0.75, dictionary="DICT_4X4_50",
                            marker_layout=ct.MarkerLayout.OPENCV_CHARUCO)
result = ct.detect_charuco(image, params=ct.CharucoDetectorParams.for_board(board))
print(f"{len(result.detection.corners)} corners detected")
```

→ **[Full Getting Started tutorial](https://vitalyvorobyev.github.io/calib-targets-rs/getting-started.html)** — target selection, printing guidance, Rust API, and calibration point pairs.

## Quickstart

### Chessboard

```bash
cargo add calib-targets image
```

```rust,no_run
use calib_targets::detect;
use calib_targets::ChessboardParams;
use image::ImageReader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let img = ImageReader::open("board.png")?.decode()?.to_luma8();
    let chess_cfg = detect::default_chess_config();
    let params = ChessboardParams::default();

    let result = detect::detect_chessboard(&img, &chess_cfg, params);
    println!("detected: {}", result.is_some());
    Ok(())
}
```

This code (see [example](./crates/calib-targets/examples/detect_chessboard.rs)) was used to process the 1024x576 image shown below. End-to-end detection took 3.1 ms: 2.9 ms for ChESS corner detection (single scale, `rayon` feature on) and 132 µs for chessboard recognition. (Performance numbers here and later are from a MacBook Pro M4.)

![Chessboard detection overlay](book/img/chessboard_detection_mid_overlay.png)

The exact command used was:

```zsh
cargo run --release --features "tracing" --example detect_chessboard -- testdata/mid.png
```

### Markerboard

[This example](./crates/calib-targets/examples/detect_markerboard.rs) with the command

```zsh
cargo run --release --features "tracing" --example detect_markerboard -- testdata/markerboard_crop.png
```

produces the detection below (643x358 px):

![Markerboard detection overlay](book/img/marker_detect_report_crop_overlay.png)

in 2.4 ms (including 1.5 ms for ChESS corner detection and 250 µs for chessboard detection).

### ChArUco

The 720x540 px ChArUco target in the first image took 3.2 ms (2.1 ms for ChESS corner detection and 250 µs for chessboard detection).
ChArUco dictionaries and board layouts are fully compatible with OpenCV's aruco/charuco implementation.

[This example](crates/calib-targets/examples/detect_charuco.rs) shows the code. The command is:

```zsh
cargo run --release --features "tracing" --example detect_charuco -- testdata/small2.png
```

### Python

```bash
pip install calib-targets numpy Pillow
```

```python
import numpy as np
from PIL import Image
import calib_targets as ct

image = np.asarray(Image.open("board.png").convert("L"), dtype=np.uint8)

result = ct.detect_chessboard(image)
if result is not None:
    corners = result.detection.corners
    print(f"detected {len(corners)} corners")
    print(f"first corner: {corners[0].position}, grid={corners[0].grid}")
```

All three target types follow the same pattern — swap in `detect_charuco` or
`detect_marker_board` and pass the appropriate `params`. See the [API surface](#api-surface)
below for the full signature list.

<details>
<summary>For contributors: building from source</summary>

```bash
pip install maturin
maturin develop  # run from repo root or crates/calib-targets-py
```

See `crates/calib-targets-py/README.md` for full setup details.
</details>

### The `TargetDetection` struct

`TargetDetection` is the common output container used by all detectors. For a full description of every field (`position`, `grid`, `id`, `target_position`, `score`) and when each is populated, see [Understanding Results](https://vitalyvorobyev.github.io/calib-targets-rs/output.html) in the book.

## Crates

- [`calib-targets`](https://crates.io/crates/calib-targets) -- facade crate with end-to-end helpers.
- [`calib-targets-core`](https://crates.io/crates/calib-targets-core) -- core geometry and types.
- [`calib-targets-chessboard`](https://crates.io/crates/calib-targets-chessboard) -- chessboard detector.
- [`calib-targets-aruco`](https://crates.io/crates/calib-targets-aruco) -- ArUco/AprilTag dictionaries and decoding.
- [`calib-targets-charuco`](https://crates.io/crates/calib-targets-charuco) -- ChArUco alignment and IDs.
- [`calib-targets-marker`](https://crates.io/crates/calib-targets-marker) -- checkerboard + 3-circle marker boards.
- [`calib-targets-print`](https://crates.io/crates/calib-targets-print) -- dedicated printable-target generation and JSON/SVG/PNG output.
- [`projective-grid`](https://crates.io/crates/projective-grid) -- standalone grid graph construction, traversal, and homography tools (used by `core` and `chessboard`).

Today the published Rust crates are `calib-targets`, `calib-targets-core`,
`calib-targets-chessboard`, `calib-targets-aruco`, `calib-targets-charuco`,
`calib-targets-marker`, `calib-targets-print`, and `projective-grid`. The
printable APIs are available both through the dedicated `calib-targets-print`
crate and through the published `calib-targets` facade as
`calib_targets::printable`. Repo-local companion crates such as
`calib-targets-cli`, `calib-targets-py`, and `calib-targets-ffi` are not
published on crates.io.

## Examples

The examples mentioned above are:

```bash
cargo run --example detect_chessboard -- path/to/image.png
cargo run --example detect_charuco -- path/to/image.png
cargo run --example detect_markerboard -- path/to/image.png
cargo run --example generate_printable -- testdata/printable/charuco_a4.json tmpdata/printable/charuco_a4
```

Examples with complete parameter control via JSON files:

```bash
cargo run --example chessboard -- testdata/chessboard_config.json
cargo run --example charuco_detect -- testdata/charuco_detect_config.json
```

These produce detailed json reports that can be rendered by python scripts [plot_chessboard_overlay](tools/plot_chessboard_overlay.py), [plot_charuco_overlay](tools/plot_charuco_overlay.py), and [plot_marker_overlay](tools/plot_marker_overlay.py).

Printable target generation uses canonical JSON documents stored under
`testdata/printable/`. Each flow writes `<stem>.json`, `<stem>.svg`, and
`<stem>.png` from the same source document. For the complete JSON model,
Rust/CLI/Python flows, and print-at-100%-scale guidance, see the
[printable-target guide](./book/src/printable.md).

Published Rust entry points for printable generation are the dedicated
`calib-targets-print` crate and `calib_targets::printable` from the
`calib-targets` facade crate. The CLI shown below remains a repo-local
workflow.

CLI:

```bash
cargo run -p calib-targets-cli -- generate --spec testdata/printable/charuco_a4.json --out-stem tmpdata/printable/charuco_a4
```
## C API

The repo also ships a native C ABI in `crates/calib-targets-ffi`.

Current native surface:

- generated header: `crates/calib-targets-ffi/include/calib_targets_ffi.h`
- header-only C++ helper wrapper: `crates/calib-targets-ffi/include/calib_targets_ffi.hpp`
- repo-owned C and C++ smoke examples plus an external compile/run smoke test
- a repo-local staged CMake package, release-archive helper, and `find_package(...)` consumer example

Current support boundaries:

- build from this workspace with `cargo build -p calib-targets-ffi`
- grayscale `u8` image input only
- built-in dictionary ids only
- supported tagged releases attach native archives for Linux, macOS, and Windows
- no crates.io package or package-manager metadata
- the C++ helper wrapper assumes a C++17-capable compiler, and the staged CMake flow targets CMake 3.16+

For release-archive download steps, ownership rules, the query/fill result
model, and concise C/C++ tutorials, see [the C API guide](./docs/ffi/README.md).
If you want the shortest path to a working downstream project, start with the
[CMake consumer quickstart](./docs/ffi/cmake-consumer-quickstart.md).

## Python bindings

Python bindings live in `crates/calib-targets-py`. See the [Python quickstart](#python) above for a minimal example.

### API surface {#api-surface}

- `calib_targets.detect_chessboard(image, *, chess_cfg=None, params=None) -> ChessboardDetectionResult | None`
- `calib_targets.detect_charuco(image, *, chess_cfg=None, params) -> CharucoDetectionResult`
- `calib_targets.detect_marker_board(image, *, chess_cfg=None, params=None) -> MarkerBoardDetectionResult | None`
- `calib_targets.render_target_bundle(document) -> GeneratedTargetBundle`
- `calib_targets.write_target_bundle(document, output_stem) -> WrittenTargetBundle`

Note: `target_position` is populated only when a board layout includes a valid
cell size and alignment succeeds (for marker boards, set `params.layout.cell_size`).

Config inputs:

- Dataclass-based typed inputs only (`ChessConfig`, `ChessboardParams`,
  `CharucoDetectorParams`, `MarkerBoardParams`, printable target document types, etc.).
- Mapping/dict config overrides are intentionally not supported in the new API.
- `detect_charuco` requires `params` with `params.board`.
- All config/result models provide `to_dict()` and `from_dict(...)` for
  compatibility with JSON/dict pipelines.

## Performance and accuracy

Benchmarks are coming. The goal is to be the fastest detector in this class while maintaining high sensitivity and accuracy.

## Development

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features
cargo test --workspace --all-features
cargo doc --workspace --all-features
mdbook build book
```

For contribution rules see [AGENTS.ms](./AGENTS.ms).

## License

This project is dual-licensed under MIT or Apache-2.0, at your option. See `LICENSE` and `LICENSE-APACHE`.
