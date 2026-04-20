# calib-targets-wasm

WebAssembly bindings for the [calib-targets] Rust workspace. Run
chessboard, ChArUco, PuzzleBoard, and marker-board detection directly in
the browser from a canvas, an `ImageBitmap`, or any `Uint8Array` of
grayscale pixels.

- Tiny: ~436 KB raw, ~195 KB gzipped.
- No threads, no `image` codec. Zero runtime dependencies.
- Same detectors as the Rust facade — no algorithmic differences.
- Works in every modern browser supporting `wasm-bindgen`.

[calib-targets]: https://github.com/VitalyVorobyev/calib-targets-rs

Book & per-target chapters: <https://vitalyvorobyev.github.io/calib-targets-rs/>

## Install

```bash
npm install calib-targets-wasm
# or, for the local build output:
scripts/build-wasm.sh   # produces demo/pkg/
```

## Hello world

```typescript
import init, {
  default_chess_config,
  default_chessboard_params,
  detect_chessboard,
  rgba_to_gray,
} from "calib-targets-wasm";

await init(); // initialise the WASM module once per page

const canvas = document.createElement("canvas");
const ctx = canvas.getContext("2d")!;
// ... draw image to canvas ...
const rgba = new Uint8Array(ctx.getImageData(0, 0, canvas.width, canvas.height).data.buffer);
const gray = rgba_to_gray(rgba, canvas.width, canvas.height);

const result = detect_chessboard(
  canvas.width, canvas.height, gray,
  default_chess_config(),
  default_chessboard_params(),
);
if (result) {
  console.log(`labelled ${result.detection.corners.length} corners`);
}
```

## Per-target examples

Every detector takes `(w, h, pixels, chess_cfg, params)` and returns a
plain JS object you can `JSON.stringify`.

### Chessboard

```typescript
import { default_chess_config, default_chessboard_params, detect_chessboard_best } from "calib-targets-wasm";

const base = default_chessboard_params();
const configs = [0.20, 0.15, 0.08].map(t => ({
  ...base,
  chess: { ...base.chess, threshold_value: t },
}));
const best = detect_chessboard_best(width, height, gray, configs);
```

### ChArUco

```typescript
import { detect_charuco } from "calib-targets-wasm";

const board = {
  rows: 5, cols: 7, cell_size: 1.0,
  marker_size_rel: 0.75,
  dictionary: "DICT_4X4_50",
  marker_layout: "opencv_charuco",
};
const params = {
  board,
  px_per_square: 60.0,
  chessboard: default_chessboard_params(),
  scan: { border_bits: 1, inset_frac: 0.06, marker_size_rel: 0.75,
          min_border_score: 0.85, multi_threshold: true, dedup_by_id: true },
  max_hamming: 2,
  min_marker_inliers: 4,
};
const result = detect_charuco(width, height, gray, default_chess_config(), params);
// result.detection.corners[].id is the ChArUco logical corner ID.
```

### PuzzleBoard

```typescript
import { default_puzzleboard_params, detect_puzzleboard, render_puzzleboard_png } from "calib-targets-wasm";

// Generate a PuzzleBoard PNG in the browser (only PuzzleBoard is supported
// in-browser — see Limitations below).
const pngBytes = render_puzzleboard_png(10, 10, /*square_mm=*/20.0, /*dpi=*/150);

const params = default_puzzleboard_params(10, 10);
const result = detect_puzzleboard(width, height, gray, default_chess_config(), params);
// Every corner has an absolute master ID: result.detection.corners[0].id
```

### Marker board

```typescript
import { default_marker_board_params, detect_marker_board } from "calib-targets-wasm";

const params = default_marker_board_params();
params.layout = {
  rows: 6, cols: 8, cell_size: 1.0,
  circles: [
    { cell: { i: 2, j: 2 }, polarity: "white" },
    { cell: { i: 3, j: 2 }, polarity: "black" },
    { cell: { i: 2, j: 3 }, polarity: "white" },
  ],
};
const result = detect_marker_board(width, height, gray, default_chess_config(), params);
```

## Inputs

| Argument | Type | Notes |
|---|---|---|
| `width`, `height` | `number` | Image dimensions in pixels. |
| `pixels` | `Uint8Array` | Row-major grayscale buffer, length `w*h`. Use `rgba_to_gray` to convert from canvas RGBA. |
| `chess_cfg` | plain JS object | Start from `default_chess_config()` and override fields. |
| `params` | plain JS object | Per-detector shape; use `default_*_params(...)` and override. |
| `configs` (sweep) | `params[]` | Array of configs tried in order by `detect_*_best`. |

## Outputs

All result types deserialise to plain JS objects matching the Rust
`serde_json` schema — `JSON.stringify(result)` gives you a canonical,
cross-language payload.

**`LabeledCorner`** (shared across grid detectors):

```typescript
{
  position: { x: number, y: number },          // sub-pixel image location
  grid:     { i: number, j: number } | null,   // integer grid label, rebased to (0,0)
  id:       number | null,                     // ChArUco / PuzzleBoard ID
  target_position: { x: number, y: number } | null,  // mm on the printed board
  score:    number,
}
```

## Functions

| Function | Returns |
|---|---|
| `detect_corners(w, h, px, cfg)` | `Corner[]` |
| `detect_chessboard(w, h, px, cfg, params)` | `ChessboardDetectionResult \| null` |
| `detect_chessboard_best(w, h, px, configs)` | `ChessboardDetectionResult \| null` |
| `detect_charuco(w, h, px, cfg, params)` | `CharucoDetectionResult` (throws on error) |
| `detect_charuco_best(w, h, px, configs)` | `CharucoDetectionResult` (throws on all-fail) |
| `detect_puzzleboard(w, h, px, cfg, params)` | `PuzzleBoardDetectionResult` (throws on error) |
| `detect_puzzleboard_best(w, h, px, configs)` | `PuzzleBoardDetectionResult` (throws on all-fail) |
| `detect_marker_board(w, h, px, cfg, params)` | `MarkerBoardDetectionResult \| null` |
| `detect_marker_board_best(w, h, px, configs)` | `MarkerBoardDetectionResult \| null` |
| `rgba_to_gray(rgba, w, h)` | `Uint8Array` (BT.601) |
| `render_puzzleboard_png(rows, cols, square_mm, dpi)` | `Uint8Array` — encoded PNG |
| `default_chess_config()`, `default_chessboard_params()`, `default_puzzleboard_params(rows, cols)`, `default_marker_board_params()` | baseline configs |

## Tuning difficult cases

- Always prefer `detect_*_best` over `detect_*` — the 3-config sweep
  solves most common tuning needs without writing code.
- For blurry / low-contrast inputs, lower `chess.threshold_value` in one
  of the sweep configs.
- For small markers (< 12 px across), resize the source canvas up before
  calling `detect_charuco*` — WASM does not upscale for you.
- Open the [per-detector READMEs][facade] / the [book tuning chapter][tune]
  for parameter-by-parameter guidance. Every knob has the same meaning as
  in the Rust facade.

[facade]: https://docs.rs/calib-targets
[tune]: https://vitalyvorobyev.github.io/calib-targets-rs/tuning.html

## Limitations

- **Target PNG generation is only supported for PuzzleBoard**
  (`render_puzzleboard_png`). Chessboard, ChArUco, and marker-board PNG /
  SVG generation is not yet exposed in the WASM surface — generate them
  server-side via the Rust facade, the [Python bindings][py], or the
  [workspace CLI][cli], and serve the images to the browser.
- **One target per image.** Same as the Rust facade; multiple boards in
  one frame are not disambiguated.
- **No fisheye support.** Moderate distortion is handled; severe wide-angle
  optics are not.
- **Grayscale only.** Convert from RGBA with `rgba_to_gray` before
  calling any detector.
- **No threads.** The WASM build is single-threaded; heavy detection on
  4K images may exceed 100 ms per call. Consider Web Workers.
- **No `detect_chessboard_debug` / `detect_chessboard_all`.** The
  multi-component and debug-frame helpers are Rust-only.

[py]: https://pypi.org/project/calib-targets/
[cli]: https://crates.io/crates/calib-targets

## Build from source

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
scripts/build-wasm.sh            # outputs to demo/pkg/
```

## Demo

A React / TypeScript / Vite demo app (using `bun`, not `npm`) lives at
[`demo/`](../../demo):

```bash
scripts/build-wasm.sh
cd demo && bun install && bun run dev
```

The demo covers all four target types with live parameter tuning and
canvas overlays.

## License

MIT or Apache-2.0, at your option.
