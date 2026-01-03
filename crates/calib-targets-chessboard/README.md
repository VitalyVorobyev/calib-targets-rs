# calib-targets-chessboard

![Chessboard detection overlay](https://raw.githubusercontent.com/VitalyVorobyev/calib-targets-rs/main/book/img/chessboard_detection_mid_overlay.png)

Plain chessboard detector built on top of `calib-targets-core`.

## Quickstart

```rust
use calib_targets_chessboard::{ChessboardDetector, ChessboardParams};
use calib_targets_core::Corner;

fn main() {
    let params = ChessboardParams::default();
    let detector = ChessboardDetector::new(params);

    let corners: Vec<Corner> = Vec::new();
    let result = detector.detect_from_corners(&corners);
    println!("detected: {}", result.is_some());
}
```

## Features

- `tracing`: enables tracing output in the detector and grid graph stages.

## Python bindings

Python bindings are provided via the workspace facade (`calib_targets` module).
See `crates/calib-targets-py/README.md` in the repo root for setup.

## Links

- Repository: https://github.com/VitalyVorobyev/calib-targets-rs
