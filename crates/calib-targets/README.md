# calib-targets

High-level facade crate for the `calib-targets-*` workspace.

## What this crate provides

- Stable re-exports of the underlying crates: `aruco`, `charuco`, `chessboard`, `core`, `marker`.
- Optional end-to-end helpers in `calib_targets::detect` (enabled by default).

## Quickstart (chessboard)

```rust,no_run
use calib_targets::detect;
use calib_targets::chessboard::{ChessboardParams, GridGraphParams};
use image::ImageReader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let img = ImageReader::open("board.png")?.decode()?.to_luma8();
    let chess_cfg = detect::default_chess_config();
    let params = ChessboardParams::default();
    let graph = GridGraphParams::default();

    let result = detect::detect_chessboard(&img, &chess_cfg, params, graph);
    println!("detected: {}", result.is_some());
    Ok(())
}
```

## Features

- `image` (default): enables the `calib_targets::detect` helpers.
- `tracing`: enables tracing output across the subcrates.

## Links

- Repository: https://github.com/VitalyVorobyev/calib-targets-rs
- Workspace docs: https://github.com/VitalyVorobyev/calib-targets-rs/blob/main/README.md
