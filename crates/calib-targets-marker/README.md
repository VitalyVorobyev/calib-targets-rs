# calib-targets-marker

![Marker-board detection overlay](https://raw.githubusercontent.com/VitalyVorobyev/calib-targets-rs/main/book/img/marker_detect_report_crop_overlay.png)

Checkerboard marker target detector (checkerboard + 3 central circles).

## Quickstart

```rust
use calib_targets_core::{Corner, GrayImageView};
use calib_targets_marker::{
    CellCoords, CirclePolarity, MarkerBoardDetector, MarkerBoardLayout, MarkerBoardParams,
    MarkerCircleSpec,
};

fn main() {
    let layout = MarkerBoardLayout {
        rows: 6,
        cols: 8,
        cell_size: Some(1.0),
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

## Notes

- `cell_size` controls `target_position` in the output; set it to your square size.

## Python bindings

Python bindings are provided via the workspace facade (`calib_targets` module).
See `python/README.md` in the repo root for setup.

## Features

- `tracing`: enables tracing output in the detector.

## Links

- Repository: https://github.com/VitalyVorobyev/calib-targets-rs
