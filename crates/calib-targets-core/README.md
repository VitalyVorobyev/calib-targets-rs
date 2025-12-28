# calib-targets-core

![Rectified grid view](https://raw.githubusercontent.com/VitalyVorobyev/calib-targets-rs/main/book/img/rectified_small.png)

Core types and geometric utilities for calibration target detection.

## Quickstart

```rust
use calib_targets_core::{Corner, TargetDetection, TargetKind};
use nalgebra::Point2;

fn main() {
    let corner = Corner {
        position: Point2::new(10.0, 20.0),
        orientation: 0.0,
        orientation_cluster: None,
        strength: 1.0,
    };

    let detection = TargetDetection {
        kind: TargetKind::Chessboard,
        corners: Vec::new(),
    };

    println!("corner: {:?}", corner.position);
    let _ = detection;
}
```

## Includes

- Homography estimation and warping helpers.
- Lightweight grayscale image views and sampling.
- Grid alignment and target detection types.

## Links

- Repository: https://github.com/VitalyVorobyev/calib-targets-rs
