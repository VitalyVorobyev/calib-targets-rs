# calib-targets

![Mesh-rectified grid](https://raw.githubusercontent.com/VitalyVorobyev/calib-targets-rs/main/book/img/mesh_rectified_mid.png)

Fast, robust calibration target detection in Rust: chessboard, ChArUco, ArUco/AprilTag dictionaries, and marker boards.

## Highlights

- Shared `TargetDetection` output across detectors for consistent downstream processing.
- End-to-end helpers (`calib_targets::detect`) that run ChESS corner detection for you (feature `image`, enabled by default).
- Low-level detector crates are re-exported when you need custom pipelines.

## Quickstart (chessboard)

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

## What you get back

All detectors produce a `TargetDetection` (returned directly for chessboards and embedded in higher-level result structs elsewhere). Each `LabeledCorner` includes pixel `position`, optional grid coordinates, optional logical `id`, optional target-space position, and a detector-specific `score`.

## Supported targets

- Chessboard: `detect::detect_chessboard` or `chessboard::ChessboardDetector`.
- ChArUco: `detect::detect_charuco` or `charuco::CharucoDetector`.
- Marker boards: `detect::detect_marker_board` or `marker::MarkerBoardDetector`.
- ArUco/AprilTag dictionaries and decoding via `aruco`.
- Printable targets: `printable::render_target_bundle` / `printable::write_target_bundle`.

## Features

- `image` (default): enables `calib_targets::detect` helpers that use `image::GrayImage` and `chess-corners`.
- `tracing`: enables tracing output across the workspace crates.

## Examples (repo)

```bash
cargo run -p calib-targets --example detect_chessboard -- path/to/image.png
cargo run -p calib-targets --example detect_charuco -- path/to/image.png
cargo run -p calib-targets --example detect_markerboard -- path/to/image.png
cargo run -p calib-targets --example generate_printable -- testdata/printable/charuco_a4.json tmpdata/printable/charuco_a4
```

## Printable targets

The facade re-exports the workspace printable backend as
`calib_targets::printable`. `PrintableTargetDocument` is the canonical
JSON-backed input, and `write_target_bundle` writes `<stem>.json`,
`<stem>.svg`, and `<stem>.png` in one call.

For the full printable-target workflow, including the canonical JSON example,
CLI/Python entry points, and print-scale validation guidance, see the
[printable-target guide](https://vitalyvorobyev.github.io/calib-targets-rs/printable.html).
The repo-local `calib-targets-cli` binary mentioned in the workspace docs is
not published on crates.io.

## Python bindings

Python bindings are provided via `crates/calib-targets-py` (module name
`calib_targets`). See `crates/calib-targets-py/README.md` for setup.

```bash
pip install maturin
maturin develop
python crates/calib-targets-py/examples/detect_charuco.py path/to/image.png
```

Notes:

- Python config accepts typed params classes only.
- `detect_charuco` requires `params` and the board lives in `params.board`.
- `target_position` is populated only when a board layout includes a valid
  cell size and alignment succeeds (for marker boards, set `params.layout.cell_size`).

## Crate map

- `calib_targets::core` – core types and homographies.
- `calib_targets::chessboard` – chessboard detection.
- `calib_targets::aruco` – ArUco/AprilTag dictionaries and decoding.
- `calib_targets::charuco` – ChArUco alignment and IDs.
- `calib_targets::marker` – checkerboard + circle marker boards.
- `calib_targets::printable` – printable target generation.

## Links

- Docs: https://docs.rs/calib-targets
- Repository: https://github.com/VitalyVorobyev/calib-targets-rs
- Workspace docs: https://github.com/VitalyVorobyev/calib-targets-rs/blob/main/README.md
