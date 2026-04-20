# calib-targets-rs

[![CI](https://github.com/VitalyVorobyev/calib-targets-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/VitalyVorobyev/calib-targets-rs/actions/workflows/ci.yml)
[![Security audit](https://github.com/VitalyVorobyev/calib-targets-rs/actions/workflows/audit.yml/badge.svg)](https://github.com/VitalyVorobyev/calib-targets-rs/actions/workflows/audit.yml)
[![Docs](https://github.com/VitalyVorobyev/calib-targets-rs/actions/workflows/docs.yml/badge.svg)](https://vitalyvorobyev.github.io/calib-targets-rs/)
[![MSRV](https://img.shields.io/badge/MSRV-1.88-blue.svg)](https://blog.rust-lang.org/2025/06/26/Rust-1.88.0/)

**Calibration-target detection in Rust.** Detects chessboards, ChArUco,
PuzzleBoard, and checkerboard marker boards from grayscale images.
Ships as Rust crates, Python bindings, WebAssembly bindings, and a
stable C ABI. One grid-first algorithmic core; a single
`TargetDetection` output type across every detector.

![Target gallery — chessboard, ChArUco, PuzzleBoard, marker board](docs/img/target_gallery.png)

> **Status:** feature-complete, heading into a 0.7 release. APIs are
> stabilising but may still change at minor versions.

| Target | When to use |
|---|---|
| **Chessboard** | Simplest option; no markers needed. |
| **ChArUco** | Partial views OK, unique corner IDs. |
| **PuzzleBoard** | Self-identifying chessboard; any visible fragment yields the **same absolute corner IDs** a full-view decode would. Multi-camera rigs, heavy occlusion. |
| **Marker board** | Checkerboard + circle markers; unique origin without a dictionary. |

Full documentation: [book][book] · [API reference][api] · [getting-started tutorial][getting-started].

[book]: https://vitalyvorobyev.github.io/calib-targets-rs/
[api]: https://vitalyvorobyev.github.io/calib-targets-rs/api
[getting-started]: https://vitalyvorobyev.github.io/calib-targets-rs/getting-started.html

## Main ideas

- **Grid-first.** Every detector reduces to "find a chessboard grid,
  then decode anchors / dots / circles in rectified cells". The heavy
  lifting lives in [`calib-targets-chessboard`] and
  [`projective-grid`][projective-grid-readme].
- **Local invariants, not global warps.** Graph construction, seed
  formation, and validation all work on local neighbourhoods — so
  moderate perspective and radial distortion degrade gracefully without
  an explicit distortion model.
- **Partial boards supported.** PuzzleBoard gives absolute IDs from a
  single visible fragment; ChArUco / marker boards label whatever is
  visible and the facade `detect_*_all` helpers return every connected
  component.
- **Multi-config sweeps.** `detect_*_best` tries three built-in presets
  and keeps the best result — no hand-tuning required for the common
  failure cases.

[`calib-targets-chessboard`]: crates/calib-targets-chessboard
[projective-grid-readme]: crates/projective-grid

## Quickstart

### Rust

```bash
cargo add calib-targets image
```

```rust,no_run
use calib_targets::chessboard::DetectorParams;
use calib_targets::detect;
use image::ImageReader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let img = ImageReader::open("board.png")?.decode()?.to_luma8();
    let result = detect::detect_chessboard_best(&img, &DetectorParams::sweep_default());
    match result {
        Some(det) => println!("{} corners", det.target.corners.len()),
        None => println!("no board detected"),
    }
    Ok(())
}
```

See also
[`detect_charuco`](crates/calib-targets/examples/detect_charuco.rs),
[`detect_markerboard`](crates/calib-targets/examples/detect_markerboard.rs),
[`detect_puzzleboard`](crates/calib-targets/examples/detect_puzzleboard.rs),
and every `*_best` variant.

### Python

```bash
pip install calib-targets numpy Pillow
```

```python
import numpy as np
from PIL import Image
import calib_targets as ct

image = np.asarray(Image.open("board.png").convert("L"), dtype=np.uint8)
result = ct.detect_chessboard_best(image, [
    ct.ChessboardParams(),
    ct.ChessboardParams(chess=ct.ChessConfig(threshold_value=0.15)),
    ct.ChessboardParams(chess=ct.ChessConfig(threshold_value=0.08)),
])
if result is not None:
    print(f"{len(result.detection.corners)} corners")
```

End-to-end round-trip examples per target type (generate → detect →
export to JSON) live under
[`crates/calib-targets-py/examples/`](crates/calib-targets-py/examples/) —
one runnable script per target:
`chessboard_roundtrip.py`, `charuco_roundtrip.py`,
`markerboard_roundtrip.py`, `puzzleboard_roundtrip.py`.

### WebAssembly

Browser-ready WASM (~195 KB gzipped), with a React demo:

```bash
scripts/build-wasm.sh
cd demo && bun install && bun run dev
```

See [`crates/calib-targets-wasm`](crates/calib-targets-wasm) for the
TypeScript API and per-target snippets.

### Printable targets

Generate a target, print at 100 % scale, then detect:

```python
import calib_targets as ct
doc = ct.PrintableTargetDocument(
    target=ct.CharucoTargetSpec(
        rows=5, cols=7, square_size_mm=20.0,
        marker_size_rel=0.75, dictionary="DICT_4X4_50",
    )
)
ct.write_target_bundle(doc, "my_board/charuco_a4")  # .json + .svg + .png
```

Ready-made specs for all four target types: [`testdata/printable/`](testdata/printable).
Canonical guide: [printable-target chapter][printable-chapter].

[printable-chapter]: https://vitalyvorobyev.github.io/calib-targets-rs/printable.html

## Limitations

- **One target instance per image.** Multiple simultaneous boards are
  not disambiguated; the largest detection wins, or none.
- **Pinhole-ish optics only.** Moderate perspective and moderate radial
  distortion are handled; fisheye and extreme wide-angle lenses are not
  supported. Thanks to local invariants the detector degrades gracefully
  under moderate distortion without explicit distortion modelling.
- **Grayscale input only.** Colour images must be converted upstream.
- **Single-image detection.** No temporal tracking.
- **Roughly-square cells.** Strongly anisotropic aspect ratios degrade
  detection — rescale the input first.

## Crates

| Crate | crates.io | Role |
|---|---|---|
| [`calib-targets`](crates/calib-targets) | [published](https://crates.io/crates/calib-targets) | Facade — the crate most users install. End-to-end `detect_*` / `detect_*_best`. |
| [`projective-grid`](crates/projective-grid) | [published](https://crates.io/crates/projective-grid) | Backbone — standalone grid graph, traversal, homography. |
| [`calib-targets-core`](crates/calib-targets-core) | [published](https://crates.io/crates/calib-targets-core) | Shared types: `Corner`, `LabeledCorner`, `TargetDetection`. |
| [`calib-targets-chessboard`](crates/calib-targets-chessboard) | [published](https://crates.io/crates/calib-targets-chessboard) | Invariant-first chessboard detector. |
| [`calib-targets-aruco`](crates/calib-targets-aruco) | [published](https://crates.io/crates/calib-targets-aruco) | ArUco / AprilTag dictionaries and decoding. |
| [`calib-targets-charuco`](crates/calib-targets-charuco) | [published](https://crates.io/crates/calib-targets-charuco) | ChArUco alignment and IDs. |
| [`calib-targets-puzzleboard`](crates/calib-targets-puzzleboard) | [published](https://crates.io/crates/calib-targets-puzzleboard) | Self-identifying PuzzleBoard. |
| [`calib-targets-marker`](crates/calib-targets-marker) | [published](https://crates.io/crates/calib-targets-marker) | Checkerboard + 3-circle marker boards. |
| [`calib-targets-print`](crates/calib-targets-print) | [published](https://crates.io/crates/calib-targets-print) | Printable target generation (JSON / SVG / PNG). |
| [`calib-targets-py`](crates/calib-targets-py) | PyPI | Python bindings (PyO3 / maturin). |
| [`calib-targets-wasm`](crates/calib-targets-wasm) | npm / repo-local | WebAssembly bindings. |
| [`calib-targets-ffi`](crates/calib-targets-ffi) | repo-local | C ABI bindings ([docs](./docs/ffi/README.md)). |

The printable-target CLI now ships with both the facade crate and the Python
package: `cargo install calib-targets` provides a `calib-targets` binary, and
`pip install calib-targets` provides the same command as a Python console
script. See [`crates/calib-targets/README.md`](crates/calib-targets/README.md)
for usage.

## Development

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-features
cargo doc --workspace --no-deps         # must produce zero warnings
mdbook build book
```

Pre-release quality gates (binding parity, typing stubs, WASM build, etc.)
are documented in [`.claude/CLAUDE.md`](.claude/CLAUDE.md).

## Diligence statement

This project is developed with AI coding assistants (Codex and Claude
Code) as implementation tools. The project author is an expert in
computer vision, validates algorithmic behaviour and numerical results,
and enforces quality gates before release.

## License

Dual-licensed under MIT or Apache-2.0, at your option. See [`LICENSE`](LICENSE)
and [`LICENSE-APACHE`](LICENSE-APACHE).
