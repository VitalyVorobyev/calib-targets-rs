# calib-targets-marker

Checkerboard marker target detector (checkerboard + 3 central circles).

## Example

```rust
use calib_targets_core::{Corner, GrayImageView};
use calib_targets_marker::{
    CirclePolarity, MarkerBoardDetector, MarkerBoardLayout, MarkerBoardParams, MarkerCircleSpec,
};
use calib_targets_marker::coords::CellCoords;

fn main() {
    let layout = MarkerBoardLayout {
        rows: 6,
        cols: 8,
        cell_size: None,
        circles: [
            MarkerCircleSpec {
                cell: CellCoords { i: 2, j: 2 },
                polarity: CirclePolarity::White,
            },
            MarkerCircleSpec {
                cell: CellCoords { i: 3, j: 2 },
                polarity: CirclePolarity::Black,
            },
            MarkerCircleSpec {
                cell: CellCoords { i: 2, j: 3 },
                polarity: CirclePolarity::White,
            },
        ],
    };

    let params = MarkerBoardParams::new(layout);
    let detector = MarkerBoardDetector::new(params);

    let pixels = vec![0u8; 32 * 32];
    let view = GrayImageView {
        width: 32,
        height: 32,
        data: &pixels,
    };
    let corners: Vec<Corner> = Vec::new();

    let _ = detector.detect_from_image_and_corners(&view, &corners);
}
```

## Features

- `tracing`: enables tracing output in the detector.

## Links

- Repository: https://github.com/VitalyVorobyev/calib-targets-rs
