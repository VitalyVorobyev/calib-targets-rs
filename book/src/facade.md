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

## Detector structs and free functions are one surface

Each `detect_*` free function *is* the detector one-liner: construct the
detector from `params`, then call `detect`. Reach for whichever fits — the
free function when a single call is all you need, the detector struct when
you configure once and detect many times, or when you want the
corner-reuse entry points below.

`CharucoDetector`, `PuzzleBoardDetector`, and `MarkerBoardDetector` each
expose the same five operations, mirrored by five free functions per
target (`charuco`, `puzzleboard`, `marker_board`):

- `detect(&view)` / `detect_t(img, params)` — run the ChESS corner pass
  (from `params.chess`) and the whole pipeline.
- `detect_with_corners(&view, &corners)` /
  `detect_t_with_corners(img, corners, params)` — skip the corner pass and
  use corners you already have.
- `diagnose(&view)` / `diagnose_t(img, params)` (`diagnostics` feature) —
  like `detect`, plus a diagnostics report.
- `diagnose_with_corners(&view, &corners)` /
  `diagnose_t_with_corners(img, corners, params)` (`diagnostics` feature) —
  like `detect_with_corners`, plus diagnostics.
- `detect_t_best(img, configs)` — try several configs, keep the richest
  result.

`detect_corners` on a detector runs exactly the ChESS pass that detector's
own `params.chess` configures, so corners you inject match what `detect`
would have produced on its own. The one case that genuinely wants
`detect_with_corners`: running a single corner pass across several target
detectors on the same image, rather than paying for it once per detector.

```rust,ignore
let corners = charuco_detector.detect_corners(&view);
let a = charuco_detector.detect_with_corners(&view, &corners)?;
let b = puzzle_detector.detect_with_corners(&view, &corners)?;
```

`ChessboardDetector` keeps its original, corner-cloud-only shape described
above — it has no `chess` field to run a corner pass from, by design: it
is embedded inside all three composite params types, so a nested
corner-detector config there would be dead in exactly the way a `chess`
field on `ChessboardParams` itself would be. Configure the corner pass for
a chessboard through `detect_chessboard`'s explicit `chess_cfg` argument
instead.

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
