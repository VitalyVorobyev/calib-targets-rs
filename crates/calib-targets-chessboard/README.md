# calib-targets-chessboard

Plain chessboard detector built on top of `calib-targets-core`.

## Example

```rust
use calib_targets_chessboard::{ChessboardDetector, ChessboardParams, GridGraphParams};
use calib_targets_core::Corner;

fn main() {
    let params = ChessboardParams::default();
    let detector = ChessboardDetector::new(params).with_grid_search(GridGraphParams::default());

    let corners: Vec<Corner> = Vec::new();
    let result = detector.detect_from_corners(&corners);
    println!("detected: {}", result.is_some());
}
```

## Features

- `tracing`: enables tracing output in the detector and grid graph stages.

## Links

- Repository: https://github.com/VitalyVorobyev/calib-targets-rs
