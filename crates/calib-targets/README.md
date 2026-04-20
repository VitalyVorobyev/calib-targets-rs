# calib-targets

![Target gallery — chessboard, ChArUco, PuzzleBoard, marker board](https://raw.githubusercontent.com/VitalyVorobyev/calib-targets-rs/main/docs/img/target_gallery.png)

Fast, robust calibration-target detection in Rust: chessboard, ChArUco,
PuzzleBoard, and checkerboard marker boards. This is the **facade**
crate — the one most users install. It re-exports every detector in the
workspace and adds one-call helpers that take an `image::GrayImage`, run
ChESS corner detection, and return a labelled grid.

Install-friendly entry for the workspace; each detector has its own crate
with deeper documentation and tuning reference, linked below.

Book: <https://vitalyvorobyev.github.io/calib-targets-rs/>

## Install

```bash
cargo add calib-targets image
```

## Quickstart (chessboard)

```rust,no_run
use calib_targets::chessboard::DetectorParams;
use calib_targets::detect;
use image::ImageReader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let img = ImageReader::open("board.png")?.decode()?.to_luma8();
    let params = DetectorParams::default();

    if let Some(det) = detect::detect_chessboard(&img, &params) {
        println!(
            "labelled {} corners, cell size = {:.1} px",
            det.target.corners.len(),
            det.cell_size
        );
    }
    Ok(())
}
```

## Inputs (every helper)

- `image::GrayImage` (or a `GrayImageView` on the detector traits).
- A `*Params` config struct (`DetectorParams`, `CharucoParams`,
  `PuzzleBoardParams`, `MarkerBoardParams`). Use `::default()` for
  chessboard, `::for_board(&spec)` when a layout is required.
- For the `*_best` sweep helpers, a slice `&[Params]` — typically the
  3-config preset `Params::sweep_default(&spec)`.

## Outputs

Every detector emits a `TargetDetection` (returned directly by
chessboard; wrapped inside `CharucoDetectionResult`,
`PuzzleBoardDetectionResult`, `MarkerBoardDetectionResult` for the
others). Each `LabeledCorner` carries:

| Field | Meaning |
|---|---|
| `position: Point2<f32>` | Sub-pixel image location. |
| `grid: Option<GridCoords>` | `(i, j)` integer grid index, `i` right, `j` down, rebased so bounding-box min is `(0, 0)`. |
| `id: Option<u32>` | Logical corner ID on the target (ChArUco marker-referenced, PuzzleBoard master ID). |
| `target_position: Option<Point2<f32>>` | Physical location on the printed board (mm / board units), when cell size and alignment are known. |
| `score: f32` | Detector-specific quality score. |

Chessboard enforces two hard invariants on its output: **no duplicate
`(i, j)` labels**, and `(0, 0)` sits at the visual top-left of the
detected grid.

## Supported targets

| Target | Facade helpers | Dedicated crate |
|---|---|---|
| **Chessboard** | `detect_chessboard`, `detect_chessboard_all`, `detect_chessboard_best`, `detect_chessboard_debug` | [`calib-targets-chessboard`] |
| **ChArUco** | `detect_charuco`, `detect_charuco_best` | [`calib-targets-charuco`] |
| **PuzzleBoard** | `detect_puzzleboard`, `detect_puzzleboard_best` | [`calib-targets-puzzleboard`] |
| **Marker board** | `detect_marker_board`, `detect_marker_board_best` | [`calib-targets-marker`] |
| **Printable targets** | `printable::{render_target_bundle, write_target_bundle}` | [`calib-targets-print`] |
| **ArUco / AprilTag primitives** | `aruco::*` (dictionaries, matcher) | [`calib-targets-aruco`] |

Every detector ships a single-config helper and a 3-config
`*_best` sweep. The sweep is the recommended default for new callers:
it handles threshold tradeoffs without forcing manual tuning.

## Main ideas

- **Grid-first.** Every detector reduces to "find a chessboard grid,
  then decode anchors / dots / circles in rectified cells". The heavy
  lifting lives in [`calib-targets-chessboard`] and
  [`projective-grid`].
- **Precision-by-construction.** Wrong `(i, j)` labels would corrupt
  calibration, so the detectors reject before they guess.
- **Local invariants, not global warps.** The graph, seed, and validation
  pieces work on local neighbourhoods, so moderate perspective and radial
  distortion are handled without an explicit distortion model.
- **Partial boards supported.** PuzzleBoard gives absolute IDs from any
  visible fragment; ChArUco and marker boards label whatever is visible.

## Tuning difficult cases

Most callers never need to tune. When defaults fail:

1. Switch to `detect_*_best` with the built-in 3-config sweep.
2. Inspect which config succeeded (or none) — the sweep logs counts.
3. If all fail, open the corresponding detector README:
   [chessboard][cb-tune], [ChArUco][charuco-tune],
   [PuzzleBoard][puz-tune], [marker][marker-tune], or the
   [book tuning chapter][book-tune] for cross-detector guidance.

[cb-tune]: https://docs.rs/calib-targets-chessboard
[charuco-tune]: https://docs.rs/calib-targets-charuco
[puz-tune]: https://docs.rs/calib-targets-puzzleboard
[marker-tune]: https://docs.rs/calib-targets-marker
[book-tune]: https://vitalyvorobyev.github.io/calib-targets-rs/tuning.html

## Limitations

- **One target instance per image.** Multiple simultaneous boards are
  not disambiguated; the largest detection wins.
- **Pinhole-ish optics only.** Moderate perspective and radial
  distortion are handled gracefully; fisheye and extreme wide-angle
  lenses are not supported.
- **Grayscale input.** Colour images must be converted by the caller
  (`.to_luma8()`).
- **No temporal tracking.** Every call is independent.
- **Roughly-square cells.** Strongly anisotropic aspect ratios degrade
  detection — rescale the input first.

## Printable targets

`calib_targets::printable` re-exports [`calib-targets-print`].
`PrintableTargetDocument` is the canonical JSON input, and
`write_target_bundle` writes `<stem>.json`, `<stem>.svg`, `<stem>.png`
in one call. The `calib_targets::generate` module adds ergonomic
constructors (`chessboard_document`, `charuco_document`,
`puzzleboard_document`, `marker_board_document`) that hide the
`TargetSpec` enum wrapping. Ready-made specs live under
[`testdata/printable/`](https://github.com/VitalyVorobyev/calib-targets-rs/tree/main/testdata/printable).

### CLI

`cargo install calib-targets` ships a `calib-targets` binary with two
generation flows:

```bash
# One-step: flags directly to JSON+SVG+PNG bundle
calib-targets gen chessboard \
    --inner-rows 6 --inner-cols 8 --square-size-mm 20 \
    --out-stem my_board

calib-targets gen puzzleboard \
    --rows 8 --cols 10 --square-size-mm 15 \
    --out-stem puzzle

# Two-step: init a reviewable spec first, then render
calib-targets init charuco \
    --out spec.json \
    --rows 5 --cols 7 --square-size-mm 20 \
    --marker-size-rel 0.75 --dictionary DICT_4X4_50
calib-targets validate --spec spec.json
calib-targets generate --spec spec.json --out-stem my_charuco
```

Run `calib-targets list-dictionaries` to enumerate built-in ArUco
dictionaries. The CLI is gated on the default `cli` feature; library-only
consumers can disable it with `default-features = false`.

Canonical guide: [printable-target book chapter][printable-book].

[printable-book]: https://vitalyvorobyev.github.io/calib-targets-rs/printable.html

## Features

- `image` (default) — enables the `calib_targets::detect` helpers that
  take `image::GrayImage` inputs and run `chess-corners` internally.
- `tracing` — gates tracing spans across the workspace crates.

## Chessboard API — 0.7 migration note

In 0.7 the chessboard detector's top-level types were renamed from
`ChessboardDetector` / `ChessboardParams` / `ChessboardDetectionResult`
to `Detector` / `DetectorParams` / `Detection`. `DetectorParams` is
flat — the old nested `graph` / `gap_fill` / `local_homography`
sub-structs are gone. Import paths move from
`calib_targets::chessboard::ChessboardParams` to
`calib_targets::chessboard::DetectorParams`; `detect_chessboard*` now
takes `&DetectorParams`.

## Examples

```bash
cargo run -p calib-targets --example detect_chessboard -- path/to/image.png
cargo run -p calib-targets --example detect_chessboard_best -- path/to/image.png
cargo run -p calib-targets --example detect_charuco -- path/to/image.png
cargo run -p calib-targets --example detect_charuco_best -- path/to/image.png
cargo run -p calib-targets --example detect_markerboard -- path/to/image.png
cargo run -p calib-targets --example detect_puzzleboard -- path/to/image.png
cargo run -p calib-targets --example detect_puzzleboard_best -- path/to/image.png
cargo run -p calib-targets --example generate_printable \
    -- testdata/printable/charuco_a4.json tmpdata/printable/charuco_a4
```

## Other bindings

- **Python** — [`calib-targets-py`](../calib-targets-py) wraps the same
  facade via `maturin`. The Python package name is `calib_targets`.
- **WebAssembly** — [`calib-targets-wasm`](../calib-targets-wasm) exposes
  the detectors to the browser.
- **C FFI** — [`calib-targets-ffi`](../calib-targets-ffi) exposes a
  stable C ABI with CMake package.

## Crate map

| Re-export | Crate |
|---|---|
| `calib_targets::core` | [`calib-targets-core`] — shared types, homographies |
| `calib_targets::chessboard` | [`calib-targets-chessboard`] — invariant-first chessboard |
| `calib_targets::aruco` | [`calib-targets-aruco`] — ArUco / AprilTag dictionaries + decoding |
| `calib_targets::charuco` | [`calib-targets-charuco`] — ChArUco detection |
| `calib_targets::puzzleboard` | [`calib-targets-puzzleboard`] — self-identifying chessboard |
| `calib_targets::marker` | [`calib-targets-marker`] — checkerboard + 3 circle markers |
| `calib_targets::printable` | [`calib-targets-print`] — printable targets |

Underneath everything sits the standalone [`projective-grid`] library —
useful if you want grid construction without the calibration layer.

## Links

- Docs: <https://docs.rs/calib-targets>
- Repository: <https://github.com/VitalyVorobyev/calib-targets-rs>
- Book: <https://vitalyvorobyev.github.io/calib-targets-rs/>

[`calib-targets-core`]: https://docs.rs/calib-targets-core
[`calib-targets-chessboard`]: https://docs.rs/calib-targets-chessboard
[`calib-targets-aruco`]: https://docs.rs/calib-targets-aruco
[`calib-targets-charuco`]: https://docs.rs/calib-targets-charuco
[`calib-targets-puzzleboard`]: https://docs.rs/calib-targets-puzzleboard
[`calib-targets-marker`]: https://docs.rs/calib-targets-marker
[`calib-targets-print`]: https://docs.rs/calib-targets-print
[`projective-grid`]: https://docs.rs/projective-grid
