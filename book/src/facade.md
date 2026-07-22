# calib-targets (facade)

The `calib-targets` crate is the unified entry point for the workspace. It re-exports the lower-level crates and provides optional end-to-end helpers in `calib_targets::detect` (feature `image`, enabled by default).

![Mesh-rectified grid](img/mesh_rectified_mid.png)
*Facade examples cover detection and rectification workflows.*

## Single-config detection

Each `detect_*` function takes the image plus a params struct. Where the
ChESS corner front-end is configured depends on the target shape — and the
asymmetry is deliberate:

- **Plain chessboards** are corner-cloud consumers, so `detect_chessboard`
  takes the front-end explicitly:
  `detect_chessboard(&img, &chess_cfg, &params)`. Pass
  `detect::default_chess_config()` unless you need to tune it. Because the
  chessboard detector is reusable on any corner cloud (custom upstreams,
  pre-detected corners), the facade keeps its corner config separable.
- **ChArUco, PuzzleBoard, and marker boards** own their whole-image
  pipeline, so the front-end travels on the params bundle as `params.chess`
  (defaulting to `default_chess_config()`); these entry points take no
  separate front-end argument. Their grid-step tuning lives under a
  separate `params.chessboard` field (a `ChessboardParams`).

```rust,no_run
use calib_targets::detect;
use calib_targets::chessboard::ChessboardParams;

let img = calib_targets::image::open("board.png").unwrap().to_luma8();
let params = ChessboardParams::default();
let result = detect::detect_chessboard(&img, &detect::default_chess_config(), &params);
```

## Multi-config sweep

For challenging images (uneven lighting, Scheimpflug optics), try multiple
parameter configs and keep the best result:

```rust,no_run
use calib_targets::detect;
use calib_targets::charuco::{CharucoBoardSpec, CharucoParams};
use calib_targets::aruco::builtins;

let img = calib_targets::image::open("charuco.png").unwrap().to_luma8();
let board = CharucoBoardSpec::new(22, 22, 1.0, 0.75, builtins::DICT_4X4_1000)
    .with_marker_layout(calib_targets::charuco::MarkerLayout::OpenCvCharuco);
let configs = CharucoParams::sweep_for_board(&board);
let result = detect::detect_charuco_best(&img, &configs);
```

`sweep_for_board()` returns three configs with different ChESS thresholds
(default, high, low). `detect_charuco_best` tries each and returns the result
with the most markers (then most corners).

PuzzleBoard follows the same facade shape. Its sweep also includes a
hard-weighted fallback for high-distortion fragments:

```rust,no_run
use calib_targets::detect;
use calib_targets::puzzleboard::{PuzzleBoardParams, PuzzleBoardSpec};

let img = calib_targets::image::open("puzzleboard.png").unwrap().to_luma8();
let spec = PuzzleBoardSpec::new(10, 10, 12.0).unwrap();
let configs = PuzzleBoardParams::sweep_for_board(&spec);
let result = detect::detect_puzzleboard_best(&img, &configs);
```

## Features

- `image` (default): enables `calib_targets::detect`.
- `tracing`: enables tracing output across the subcrates.
- `diagnostics` (off): forwards to the `diagnostics` feature of the
  chessboard, ChArUco, puzzleboard, and marker subcrates, gating their
  serializable trace surfaces (the chessboard `trace_topological` /
  `GeometryCheckTrace`, the ChArUco / puzzleboard per-component decode
  diagnostics, and the marker-board circle-hypothesis diagnostics). The
  detectors build no per-stage trace on the hot `detect_*` paths unless
  this is enabled (the `dataset` feature on `calib-targets-chessboard`
  implies it). The ChArUco, puzzleboard, and marker diagnostics are gated
  behind the same feature as chessboard rather than being always-on.

See the [Migration Guide](migration.md) for the full breaking-change
list when upgrading from an earlier release.
