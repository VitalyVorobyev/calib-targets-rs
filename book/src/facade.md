# calib-targets (facade)

The `calib-targets` crate is the unified entry point for the workspace. It re-exports the lower-level crates and provides optional end-to-end helpers in `calib_targets::detect` (feature `image`, enabled by default).

![Mesh-rectified grid](img/mesh_rectified_mid.png)
*Facade examples cover detection and rectification workflows.*

## Single-config detection

Each `detect_*` function takes a single params struct that includes the ChESS
corner detector configuration (`params.chess` or `params.chessboard.chess`):

```rust,no_run
use calib_targets::detect;
use calib_targets::chessboard::ChessboardParams;

let img = image::open("board.png").unwrap().to_luma8();
let params = ChessboardParams::default();
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
let board = CharucoBoardSpec {
    rows: 22, cols: 22, cell_size: 1.0,
    marker_size_rel: 0.75,
    dictionary: builtins::DICT_4X4_1000,
    marker_layout: calib_targets::charuco::MarkerLayout::OpenCvCharuco,
};
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
