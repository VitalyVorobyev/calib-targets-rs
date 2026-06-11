# calib-targets (facade)

The `calib-targets` crate is the unified entry point for the workspace. It re-exports the lower-level crates and provides optional end-to-end helpers in `calib_targets::detect` (feature `image`, enabled by default).

![Mesh-rectified grid](img/mesh_rectified_mid.png)
*Facade examples cover detection and rectification workflows.*

## Single-config detection

Each `detect_*` function takes a single params struct. The chessboard
detector uses the workspace's `default_chess_config()` for ChESS
corner detection automatically; ChArUco / PuzzleBoard / marker board
params embed a `DetectorParams` under `params.chessboard`.

```rust,no_run
use calib_targets::detect;
use calib_targets::chessboard::DetectorParams;

let img = image::open("board.png").unwrap().to_luma8();
let params = DetectorParams::default();
let result = detect::detect_chessboard(&img, &params);
```

## Multi-config sweep

For challenging images (uneven lighting, Scheimpflug optics), try multiple
parameter configs and keep the best result:

```rust,no_run
use calib_targets::detect;
use calib_targets::charuco::{CharucoBoardSpec, CharucoParams};
use calib_targets::aruco::builtins;

let img = image::open("charuco.png").unwrap().to_luma8();
let board = CharucoBoardSpec::new(22, 22, 1.0, 0.75, builtins::DICT_4X4_1000)
    .with_marker_layout(calib_targets::charuco::MarkerLayout::OpenCvCharuco);
let configs = CharucoParams::sweep_for_board(&board);
let result = detect::detect_charuco_best(&img, &configs);
```

`sweep_for_board()` returns three configs with different ChESS thresholds
(default, high, low). `detect_charuco_best` tries each and returns the result
with the most markers (then most corners).

PuzzleBoard follows the same facade shape:

```rust,no_run
use calib_targets::detect;
use calib_targets::puzzleboard::{PuzzleBoardParams, PuzzleBoardSpec};

let img = image::open("puzzleboard.png").unwrap().to_luma8();
let spec = PuzzleBoardSpec::new(10, 10, 12.0).unwrap();
let configs = PuzzleBoardParams::sweep_for_board(&spec);
let result = detect::detect_puzzleboard_best(&img, &configs);
```

## Features

- `image` (default): enables `calib_targets::detect`.
- `tracing`: enables tracing output across the subcrates.
- `diagnostics` (off): forwards to the `diagnostics` feature of the
  chessboard, ChArUco, and puzzleboard subcrates, and gates **every**
  `detect_*_with_diagnostics` entry point (the `DebugFrame` /
  self-consistency channels). The detectors build no per-stage trace on
  the hot `detect_*` paths unless this is enabled (the `dataset` feature
  on `calib-targets-chessboard` implies it). Phase-5 unified this:
  ChArUco and puzzleboard diagnostics are now gated behind the same
  feature as chessboard rather than being always-on.

See the [Migration Guide](migration.md) for the full breaking-change
list when upgrading from an earlier release.
