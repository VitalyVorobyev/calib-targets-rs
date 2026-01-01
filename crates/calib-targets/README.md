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

## Features

- `image` (default): enables `calib_targets::detect` helpers that use `image::GrayImage` and `chess-corners`.
- `tracing`: enables tracing output across the workspace crates.

## Examples (repo)

```bash
cargo run -p calib-targets --example detect_chessboard -- path/to/image.png
cargo run -p calib-targets --example detect_charuco -- path/to/image.png
cargo run -p calib-targets --example detect_markerboard -- path/to/image.png
```

## Python bindings

Python bindings are provided via `crates/calib-targets-py` (module name
`calib_targets`). See the workspace `python/README.md` for setup.

```bash
pip install maturin
maturin develop
python python/examples/detect_charuco.py path/to/image.png
```

Notes:

- Python config accepts typed params classes or dict overrides (partial dicts are OK).
- `target_position` is populated only when a board layout includes a valid
  cell size and alignment succeeds (for marker boards, set
  `params["layout"]["cell_size"]`).

## Crate map

- `calib_targets::core` – core types and homographies.
- `calib_targets::chessboard` – chessboard detection.
- `calib_targets::aruco` – ArUco/AprilTag dictionaries and decoding.
- `calib_targets::charuco` – ChArUco alignment and IDs.
- `calib_targets::marker` – checkerboard + circle marker boards.

## Links

- Docs: https://docs.rs/calib-targets
- Repository: https://github.com/VitalyVorobyev/calib-targets-rs
- Workspace docs: https://github.com/VitalyVorobyev/calib-targets-rs/blob/main/README.md
