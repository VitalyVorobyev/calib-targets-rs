# Introduction

`calib-targets-rs` is a workspace of Rust crates for detecting and modeling planar calibration targets from **corner clouds** (for example, ChESS corners). The focus is geometry-first: target modeling, grid fitting, and rectification live here, while image I/O and corner detection are intentionally out of scope.

What it is:

- A small, composable set of crates for chessboard, ChArUco, and marker-style targets.
- A set of geometric primitives (homographies, rectified views, grid coords).
- Practical examples and tests based on the `chess-corners` crate.

What it is not:

- A replacement for your corner detector or image pipeline.
- A full calibration stack (no camera calibration or PnP here).

Recommended reading order:

1. [Project Overview](overview.md) and [Conventions](conventions.md)
2. [Pipeline Overview](pipeline.md)
3. Crate chapters, starting with [calib-targets-core](core.md) and [calib-targets-chessboard](chessboard.md)

This project is experimental and APIs are still evolving. The intent of this book is to document the current design and make future changes easier to reason about.

## Quickstart

Install the facade crate (the `image` feature is enabled by default):

```bash
cargo add calib-targets image
```

Minimal chessboard detection:

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

MSRV: Rust 1.70 (stable).
