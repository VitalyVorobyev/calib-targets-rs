# calib-targets-charuco

![ChArUco detection overlay](https://raw.githubusercontent.com/VitalyVorobyev/calib-targets-rs/main/book/img/charuco_detect_report_small2_overlay.png)

ChArUco board detector: chessboard grid assembly + ArUco marker
decoding + ID alignment, producing a fully-labelled set of inner-corner
points with global `(i, j)` grid coordinates and logical marker IDs.

Fully compatible with OpenCV's aruco / charuco dictionaries and board
layouts. Built on top of
[`calib-targets-chessboard`](../calib-targets-chessboard) (invariant-
first detector) and
[`calib-targets-aruco`](../calib-targets-aruco) (dictionary + bit
decoding).

## Quickstart

```rust,no_run
use calib_targets_aruco::builtins;
use calib_targets_charuco::{
    CharucoBoardSpec, CharucoDetector, CharucoParams, MarkerLayout,
};
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

    let params = CharucoParams::for_board(&board);
    let detector = CharucoDetector::new(params)?;

    // Replace with your decoded image + pre-detected ChESS corners.
    let pixels = vec![0u8; 32 * 32];
    let view = GrayImageView { width: 32, height: 32, data: &pixels };
    let corners: Vec<Corner> = Vec::new();

    let _ = detector.detect(&view, &corners)?;
    Ok(())
}
```

For an image-in convenience helper, use
[`calib_targets::detect::detect_charuco`] from the facade crate.

## Core concepts

- **`CharucoBoardSpec`** — rows, cols, cell size, marker size ratio,
  ArUco dictionary, marker layout. Convert to a runtime `CharucoBoard`
  via `CharucoBoard::from_spec(&spec)`.
- **`CharucoParams`** — detector tuning: chessboard detector params
  (flat `DetectorParams` from `calib-targets-chessboard`), marker
  decoding knobs, alignment tolerances. Use
  `CharucoParams::for_board(&spec)` for sensible defaults.
- **`CharucoDetector`** — one-shot: takes pre-detected ChESS corners
  + the grayscale image, returns a `CharucoDetectionResult` with
  labelled inner corners, marker decodes, and IDs.

## What you get back

`CharucoDetectionResult` wraps the shared `TargetDetection` with
ChArUco-specific extras: decoded marker IDs, marker corner pixel
positions, the alignment transform mapping chessboard `(i, j)` to
board master IDs, and reprojection diagnostics.

Every `LabeledCorner` in `result.detection.corners` carries:
- `position` — inner-corner pixel location (sub-pixel refined).
- `grid` — `(i, j)` in the board's local coordinate system (always
  rebased so the bounding-box minimum is `(0, 0)`).
- `id` — the board's logical corner ID (from the ChArUco layout).
- `target_position` — physical mm coordinates on the printed board
  (populated when cell size is known and alignment succeeds).

## Multi-component scenes

ChArUco markers can fragment the chessboard grid into disconnected
components (markers break contiguity, specular regions drop corners).
The underlying chessboard detector supports multi-component
recovery via `Detector::detect_all`; ChArUco's alignment then uses
marker decodes to reconcile components against the board's global IDs.

This is the only supported multi-component scenario — scenes with two
separate physical boards are **not** in scope.

## Features

- `tracing` — enables tracing instrumentation across the detection
  pipeline.

## Chessboard migration note

Prior to v0.6.0, `CharucoParams.chessboard` was of type
`ChessboardParams`. It is now `DetectorParams` (re-exported from
[`calib-targets-chessboard`](../calib-targets-chessboard)). Rename
imports accordingly; field shapes are flat (no nested
`grid_graph_params` / `gap_fill` / `graph_cleanup` / `local_homography`
sub-structs).

## Python bindings

Python bindings are provided via the workspace facade (`calib_targets`
module). See `crates/calib-targets-py/README.md` in the repo root for
setup.

## Links

- Docs: https://docs.rs/calib-targets-charuco
- Repository: https://github.com/VitalyVorobyev/calib-targets-rs
- Book: https://vitalyvorobyev.github.io/calib-targets-rs/
