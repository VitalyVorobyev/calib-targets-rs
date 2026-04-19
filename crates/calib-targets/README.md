# calib-targets

![Mesh-rectified grid](https://raw.githubusercontent.com/VitalyVorobyev/calib-targets-rs/main/book/img/mesh_rectified_mid.png)

Fast, robust calibration target detection in Rust: chessboard, ChArUco,
PuzzleBoard, ArUco/AprilTag dictionaries, and marker boards. This is the
**facade** crate — it re-exports the detector crates from the workspace
and adds high-level image-in / detection-out helpers.

## Install

```bash
cargo add calib-targets image
```

## Highlights

- Shared `TargetDetection` output across detectors for consistent
  downstream processing.
- End-to-end helpers in `calib_targets::detect` that run ChESS corner
  detection for you (feature `image`, enabled by default).
- Invariant-first **chessboard v2** detector: 119 / 120 detections and
  0 wrong `(i, j)` labels on the canonical `testdata/3536119669`
  benchmark.
- Each detector ships both a single-config `detect_*` call and a
  sweep variant (`detect_*_best`) that tries 3 pre-sets and keeps
  the best result.

## Quickstart (chessboard)

```rust,no_run
use calib_targets::detect;
use calib_targets::chessboard::DetectorParams;
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

Four entry points cover the standard chessboard workflows:

```rust,no_run
use calib_targets::detect::{
    detect_chessboard, detect_chessboard_all, detect_chessboard_best,
    detect_chessboard_debug,
};
// detect_chessboard       — Option<Detection>                 (single best component)
// detect_chessboard_all   — Vec<Detection>                    (same-board pieces)
// detect_chessboard_best  — Option<Detection>                 (best-of-3 sweep)
// detect_chessboard_debug — DebugFrame                        (full instrumentation)
```

## What you get back

Every detector produces a `TargetDetection` (returned directly for
chessboards, wrapped for ChArUco / PuzzleBoard). Each `LabeledCorner`
includes pixel `position`, optional grid coordinates, optional logical
`id`, optional target-space position, and a detector-specific `score`.

The chessboard v2 detector additionally enforces two hard invariants on
its output: no duplicate `(i, j)` labels and the bounding-box minimum
rebased to `(0, 0)` with `(0, 0)` sitting at the **visual top-left** of
the detected grid (`+i` right, `+j` down in image pixels).

## Supported targets

| Target | Detector | Helpers |
|---|---|---|
| **Chessboard** | [`calib_targets::chessboard`] (v2 invariant-first) | `detect_chessboard`, `detect_chessboard_all`, `detect_chessboard_debug`, `detect_chessboard_best` |
| **ChArUco** | [`calib_targets::charuco`] | `detect_charuco`, `detect_charuco_best` |
| **PuzzleBoard** | [`calib_targets::puzzleboard`] (self-identifying) | `detect_puzzleboard`, `detect_puzzleboard_best` |
| **Marker boards** | [`calib_targets::marker`] | `detect_marker_board` |
| **ArUco / AprilTag** | [`calib_targets::aruco`] | dictionary + decode APIs |
| **Printable targets** | [`calib_targets::printable`] | `render_target_bundle`, `write_target_bundle` |

## Features

- `image` (default): enables `calib_targets::detect` helpers that take
  `image::GrayImage` inputs and run `chess-corners` for you.
- `tracing`: enables tracing output across the workspace crates.

## Chessboard v2 API — migration note

Prior to v0.6.0 the chessboard detector's top-level types were named
`ChessboardDetector`, `ChessboardParams`, and
`ChessboardDetectionResult`. They were renamed to `Detector`,
`DetectorParams`, and `Detection` as part of the v2 rewrite. Import
paths move from `calib_targets::chessboard::ChessboardParams` to
`calib_targets::chessboard::DetectorParams`; the `detect_chessboard*`
facade signatures now take `&DetectorParams` directly.

## Examples

```bash
cargo run -p calib-targets --example detect_chessboard -- path/to/image.png
cargo run -p calib-targets --example detect_chessboard_best -- path/to/image.png
cargo run -p calib-targets --example detect_charuco -- path/to/image.png
cargo run -p calib-targets --example detect_markerboard -- path/to/image.png
cargo run -p calib-targets --example detect_puzzleboard -- path/to/image.png
cargo run -p calib-targets --example generate_printable \
    -- testdata/printable/charuco_a4.json tmpdata/printable/charuco_a4
```

## Printable targets

The facade re-exports the dedicated published `calib-targets-print`
crate as `calib_targets::printable`. `PrintableTargetDocument` is the
canonical JSON-backed input, and `write_target_bundle` writes
`<stem>.json`, `<stem>.svg`, and `<stem>.png` in one call.

For the full printable-target workflow, including the canonical JSON
example, CLI / Python entry points, and print-scale validation
guidance, see the [printable-target guide](https://vitalyvorobyev.github.io/calib-targets-rs/printable.html).
The repo-local `calib-targets-cli` binary mentioned in the workspace
docs is not published on crates.io.

## Python bindings

Python bindings are provided via `crates/calib-targets-py` (module
name `calib_targets`). See `crates/calib-targets-py/README.md` for
setup.

```bash
pip install maturin
maturin develop
python crates/calib-targets-py/examples/detect_charuco.py path/to/image.png
python crates/calib-targets-py/examples/detect_puzzleboard.py path/to/image.png
```

Notes:

- Python configs accept typed params classes only.
- `detect_charuco` requires `params` and the board lives in
  `params.board`.
- `target_position` is populated only when a board layout includes a
  valid cell size and alignment succeeds (for marker boards, set
  `params.layout.cell_size`).

## Crate map

| Re-export | Crate |
|---|---|
| `calib_targets::core` | [`calib-targets-core`](../calib-targets-core) — shared types, homographies |
| `calib_targets::chessboard` | [`calib-targets-chessboard`](../calib-targets-chessboard) — v2 invariant-first chessboard detector |
| `calib_targets::aruco` | [`calib-targets-aruco`](../calib-targets-aruco) — ArUco / AprilTag dictionaries & decoding |
| `calib_targets::charuco` | [`calib-targets-charuco`](../calib-targets-charuco) — ChArUco alignment + IDs |
| `calib_targets::puzzleboard` | [`calib-targets-puzzleboard`](../calib-targets-puzzleboard) — self-identifying PuzzleBoard |
| `calib_targets::marker` | [`calib-targets-marker`](../calib-targets-marker) — checkerboard + 3-circle marker boards |
| `calib_targets::printable` | [`calib-targets-print`](../calib-targets-print) — printable target generation |

The workspace also includes `projective-grid`, the standalone
grid-from-point-cloud library that powers the chessboard detector —
useful when you want to build your own non-calibration grid detection.

## Links

- Docs: https://docs.rs/calib-targets
- Repository: https://github.com/VitalyVorobyev/calib-targets-rs
- Book: https://vitalyvorobyev.github.io/calib-targets-rs/
