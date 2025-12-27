# calib-targets-core

Core types and geometric utilities for calibration target detection.

## Example

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

## Links

- Repository: https://github.com/VitalyVorobyev/calib-targets-rs
