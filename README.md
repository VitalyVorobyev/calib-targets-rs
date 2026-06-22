<h1>
  <a href="https://vitavision.dev/">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="book/src/img/vv-favicon-dark.svg">
      <img src="book/src/img/vv-favicon-light.svg" alt="vitavision.dev" height="48" align="left">
    </picture>
  </a>
  &nbsp;calib-targets-rs
</h1>

[![CI](https://github.com/VitalyVorobyev/calib-targets-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/VitalyVorobyev/calib-targets-rs/actions/workflows/ci.yml)
[![Security audit](https://github.com/VitalyVorobyev/calib-targets-rs/actions/workflows/audit.yml/badge.svg)](https://github.com/VitalyVorobyev/calib-targets-rs/actions/workflows/audit.yml)
[![Docs](https://github.com/VitalyVorobyev/calib-targets-rs/actions/workflows/docs.yml/badge.svg)](https://vitalyvorobyev.github.io/calib-targets-rs/)
[![MSRV](https://img.shields.io/badge/MSRV-1.88-blue.svg)](https://blog.rust-lang.org/2025/06/26/Rust-1.88.0/)

**Calibration-target detection in Rust.** Detects chessboards, ChArUco,
PuzzleBoard, and checkerboard marker boards from grayscale images.
Ships as Rust crates, Python bindings, WebAssembly bindings, and a
stable C ABI. One grid-first algorithmic core; typed result objects
per target, with a shared corner vocabulary.

![Target gallery — chessboard, ChArUco, PuzzleBoard, marker board](docs/img/target_gallery.png)

> **Status:** feature-complete. The public API is being finalised ahead of
> a stable release and may still change at minor versions.

| Target | What it is |
|---|---|
| **Chessboard** | Plain checkerboard. Detector returns labelled corner positions with `(0, 0)` rebased to the visual top-left. No markers; corners are not individually identified. |
| **ChArUco** | Chessboard with ArUco markers in white squares. Each labelled corner gets a globally-unique ID derived from the surrounding markers; partial views decode. |
| **PuzzleBoard** | Self-identifying chessboard with edge-midpoint dots encoding a 501 × 501 master pattern. Any visible fragment yields the same absolute corner IDs a full-view decode would. `Full` and `FixedBoard` search modes; soft-log-likelihood evidence is available through diagnostics for downstream consistency checks. |
| **Marker board** | Plain checkerboard with three large circle markers establishing a unique origin without a dictionary. |

Full documentation: [book][book] · [API reference][api] · [getting-started tutorial][getting-started].
Upgrading from an earlier release? See the [Migration Guide](docs/migrations/0.10.0.md)
([book chapter][migration]).

[book]: https://vitalyvorobyev.github.io/calib-targets-rs/book/
[api]: https://vitalyvorobyev.github.io/calib-targets-rs/api/
[getting-started]: https://vitalyvorobyev.github.io/calib-targets-rs/book/getting-started.html
[migration]: https://vitalyvorobyev.github.io/calib-targets-rs/book/migration.html

## Main ideas

- **Grid-first.** Every detector reduces to "find a chessboard grid,
  then decode anchors / dots / circles in rectified cells". The heavy
  lifting lives in [`calib-targets-chessboard`] and
  [`projective-grid`][projective-grid-readme].
- **Single grid pipeline, typed outputs.** The topological pipeline
  (Shu / Brunton / Fiala 2009) is image-free Delaunay +
  edge-classification + flood-fill labelling, used for all four target
  families. `GraphBuildAlgorithm` still exists as a type but the
  `SeedAndGrow` variant was removed; `Topological` is the sole variant.
- **Local invariants, not global warps.** Graph construction and
  validation work on local neighbourhoods, so moderate perspective and
  radial distortion degrade gracefully without an explicit distortion
  model. Boundary extension uses per-candidate local-H over the K
  nearest labels, tolerating stronger distortion.
- **Partial boards supported.** PuzzleBoard gives absolute IDs from a
  single visible fragment; ChArUco / marker boards label whatever is
  visible and the facade `detect_*_all` helpers return every connected
  component.
- **Consistency diagnostics built in.** PuzzleBoard surfaces the
  chosen search / scoring mode, observed edge evidence, and
  soft-decoder margins through its diagnostics channel. The chessboard
  detector ends in a Stage 9 final-geometry check that drops gross
  mislabels and isolated false positives before emitting any
  `Detection`.
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

    // The simplest call: default ChESS corner config + default detector params.
    let detection = detect::detect_chessboard(
        &img,
        &detect::default_chess_config(),
        &DetectorParams::default(),
    );

    match detection {
        Some(det) => {
            println!("detected {} corners", det.corners.len());
            for c in &det.corners {
                // position: sub-pixel (x, y) in pixels; grid: (i, j) lattice index
                println!("  ({:.1}, {:.1}) -> grid ({}, {})",
                    c.position.x, c.position.y, c.grid.i, c.grid.j);
            }
        }
        None => println!("no board detected"),
    }
    Ok(())
}
```

If detection is unreliable on a hard image (steep angle, blur, glare), reach
for the sweep helper instead of hand-tuning. `detect_chessboard_best` runs
several parameter presets and keeps the richest result;
`DetectorParams::sweep_default()` supplies three that bracket the
robustness/precision trade-off (balanced, tighter for clean boards, looser
for distorted views):

```rust,no_run
let detection = detect::detect_chessboard_best(
    &img,
    &detect::default_chess_config(),
    &DetectorParams::sweep_default(),
);
```

The other three targets follow the same shape — `detect_charuco`,
`detect_puzzleboard`, `detect_marker_board`, each with a `*_best` sweep
variant. Runnable examples:
[`detect_charuco`](crates/calib-targets/examples/detect_charuco.rs),
[`detect_markerboard`](crates/calib-targets/examples/detect_markerboard.rs),
[`detect_puzzleboard`](crates/calib-targets/examples/detect_puzzleboard.rs).

### Python

```bash
pip install calib-targets numpy Pillow
```

```python
import numpy as np
from PIL import Image
import calib_targets as ct

image = np.asarray(Image.open("board.png").convert("L"), dtype=np.uint8)

# Simplest call — all defaults:
result = ct.detect_chessboard(image)
if result is not None:
    print(f"detected {len(result.corners)} corners")
    for c in result.corners:
        print(f"  pos={c.position} grid={c.grid}")  # position in pixels, grid = (i, j)

# If detection is unreliable, try several presets and keep the best result:
configs = [
    ct.ChessboardParams(),                       # balanced default
    ct.ChessboardParams(min_labeled_corners=12), # require a denser board
    ct.ChessboardParams(max_components=1),        # single connected grid only
]
result = ct.detect_chessboard_best(image, configs)
```

End-to-end round-trip examples per target type (generate → detect →
export to JSON) live under
[`crates/calib-targets-py/examples/`](crates/calib-targets-py/examples/) —
one runnable script per target:
`chessboard_roundtrip.py`, `charuco_roundtrip.py`,
`markerboard_roundtrip.py`, `puzzleboard_roundtrip.py`.

### WebAssembly

Browser-ready WASM bindings, with a React demo:

```bash
scripts/build-wasm.sh
cd demo && bun install && bun run dev
```

See [`@vitavision/calib-targets`](crates/calib-targets-wasm) on npm
(source under `crates/calib-targets-wasm/`) for the TypeScript API
and per-target snippets.

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

## Detection results

Each detector returns a typed result; its corners share a common vocabulary.
A chessboard corner (`ChessboardCorner`) carries:

- **`position`** — sub-pixel `(x, y)` in **pixels** (image origin top-left).
- **`grid`** — integer lattice index `GridCoords { i, j }` (`i` increases
  right, `j` down), rebased so the top-left visible corner is `(0, 0)`.
- **`score`** — corner quality.

ChArUco, PuzzleBoard, and marker corners add an `id` (a globally unique
corner identifier) and a `target_position` (the corner's location on the
physical board, in millimetres) once the board is aligned. The book chapter
[Understanding Results][output] documents exactly when each optional field
is populated; the [API reference][api] lists every result type and field.

[output]: https://vitalyvorobyev.github.io/calib-targets-rs/output.html

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
| [`@vitavision/calib-targets`](crates/calib-targets-wasm) | npm / repo-local | WebAssembly bindings. |
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
