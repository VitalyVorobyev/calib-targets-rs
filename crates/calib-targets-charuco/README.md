# calib-targets-charuco

![ChArUco detection overlay](https://raw.githubusercontent.com/VitalyVorobyev/calib-targets-rs/main/book/img/charuco_detect_report_small2_overlay.png)

ChArUco board detector built on top of `calib-targets-core` and `calib-targets-aruco`.
ChArUco dictionaries and board layouts are fully compatible with OpenCV's aruco/charuco implementation.

## Quickstart

```rust,no_run
use calib_targets_aruco::builtins;
use calib_targets_charuco::{CharucoBoardSpec, CharucoDetector, CharucoDetectorParams, MarkerLayout};
use calib_targets_core::{Corner, GrayImageView};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let board = CharucoBoardSpec {
        rows: 5,
        cols: 7,
        cell_size: 1.0,
        marker_size_rel: 0.7,
        dictionary: builtins::DICT_4X4_50,
        marker_layout: MarkerLayout::OpenCvCharuco,
    };

    let params = CharucoDetectorParams::for_board(&board);
    let detector = CharucoDetector::new(board, params)?;

    let pixels = vec![0u8; 32 * 32];
    let view = GrayImageView {
        width: 32,
        height: 32,
        data: &pixels,
    };
    let corners: Vec<Corner> = Vec::new();

    let _ = detector.detect(&view, &corners)?;
    Ok(())
}
```

## Features

- `tracing`: enables tracing output in the detection pipeline.

## Links

- Repository: https://github.com/VitalyVorobyev/calib-targets-rs
