# @vitavision/calib-targets

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
npm install @vitavision/calib-targets
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
} from "@vitavision/calib-targets";

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
  console.log(`labelled ${result.corners.length} corners`);
}
```

## Per-target examples

Every detector takes `(w, h, pixels, chess_cfg, params)` and returns a
plain JS object you can `JSON.stringify`.

### Chessboard

```typescript
import { default_chess_config, default_chessboard_params, detect_chessboard_best } from "@vitavision/calib-targets";

const chessCfg = default_chess_config();
chessCfg.threshold = 15.0;

const base = default_chessboard_params();
const configs = [
  base,
  { ...base, min_labeled_corners: 12 },
  { ...base, max_components: 1 },
];
const best = detect_chessboard_best(width, height, gray, chessCfg, configs);
```

### ChArUco

```typescript
import { detect_charuco } from "@vitavision/calib-targets";

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
// result.corners[].id is the ChArUco logical corner ID.
```

### PuzzleBoard

```typescript
import {
  default_puzzleboard_params,
  detect_puzzleboard,
  render_puzzleboard_bundle,
  render_puzzleboard_png,
} from "@vitavision/calib-targets";

// Generate a PuzzleBoard PNG in the browser (PNG-only fast path).
const pngBytes = render_puzzleboard_png(10, 10, /*square_mm=*/20.0, /*dpi=*/150);

// Full JSON / SVG / PNG / DXF bundle — the DXF is the photolith-handoff
// flavor (AC1015 ASCII, $INSUNITS = 4 mm, Y-up cartesian).
const bundle = render_puzzleboard_bundle(10, 10, 20.0, 150);
// bundle.json_text  / bundle.svg_text  / bundle.dxf_text  → string
// bundle.png_bytes                                         → Uint8Array

const params = default_puzzleboard_params(10, 10);
params.decode.search_mode = { kind: "fixed_board" };
params.decode.scoring_mode = { kind: "soft_log_likelihood" };
params.decode.symmetry_mode = { kind: "rotations" }; // default
const result = detect_puzzleboard(width, height, gray, default_chess_config(), params);
// Every corner has an absolute master ID: result.corners[0].id
// Soft-mode scoring evidence is available from diagnose_puzzleboard().
```

The same `render_*_bundle` and `render_*_png` pairs exist for the other
three target families (`render_chessboard_*`, `render_charuco_*`,
`render_marker_board_*`); see the Functions table below.

### Printing on a real page

The helpers above take a fixed argument list and fit the page to the board,
which is right for a preview but leaves page size, orientation, margin and
every unnamed spec field out of reach. `render_target_bundle_json` takes the
whole `PrintableTargetDocument` instead — the same `schema_version: 1` JSON
the CLI reads and `testdata/printable/*.json` holds, so one document is
portable between the CLI, the Rust API and the browser.

```ts
import init, { render_target_bundle_json } from "@vitavision/calib-targets";
await init();

const bundle = render_target_bundle_json({
  schema_version: 1,
  target: {
    kind: "charuco",
    rows: 5, cols: 7, square_size_mm: 20.0,
    marker_size_rel: 0.75,
    dictionary: "DICT_4X4_50",
    marker_layout: "opencv_charuco",
    border_bits: 2,                       // not reachable via render_charuco_bundle
  },
  page:   { size: { kind: "letter" }, orientation: "landscape", margin_mm: 15.0 },
  render: { debug_annotations: false, png_dpi: 300 },
});
// bundle.svg_text is a 279.4 x 215.9 mm page
```

`page` and `render` are optional and default to A4 portrait, 10 mm margins,
300 DPI. Supplying the page the fixed-arity helper would have built yields a
byte-identical SVG, so this is a superset of those helpers rather than a
second rendering path. See `PrintableTargetDocument` in the TypeScript
declarations for the full shape.

### Marker board

```typescript
import { default_marker_board_params, detect_marker_board } from "@vitavision/calib-targets";

const params = default_marker_board_params();
params.board = {
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

PuzzleBoard results include a compact `decode` summary. Raw observed
edges and soft-mode runner-up scoring evidence are returned by
`diagnose_puzzleboard`.

**`LabeledCorner`** (shared across grid detectors):

```typescript
{
  position: { x: number, y: number },          // sub-pixel image location
  grid:     { u: number, v: number } | null,   // integer grid label, rebased to (0,0)
  id:       number | null,                     // ChArUco / PuzzleBoard ID
  target_position: { x: number, y: number } | null,  // mm on the printed board
  score:    number,
}
```

## Functions

| Function | Returns |
|---|---|
| `detect_corners(w, h, px, cfg)` | `Corner[]` |
| `detect_chessboard(w, h, px, cfg, params)` | `ChessboardDetection \| null` |
| `detect_chessboard_best(w, h, px, cfg, configs)` | `ChessboardDetection \| null` |
| `detect_charuco(w, h, px, cfg, params)` | `CharucoDetection` (throws on error) |
| `detect_charuco_with_corners(w, h, px, corners, params)` | `CharucoDetection` (throws on error) |
| `detect_charuco_best(w, h, px, configs)` | `CharucoDetection` (throws on all-fail) |
| `diagnose_charuco(w, h, px, cfg, params)` | `{ result: CharucoDetection \| null, diagnostics: CharucoDetectDiagnostics }` |
| `diagnose_charuco_with_corners(w, h, px, corners, params)` | same as `diagnose_charuco` |
| `detect_puzzleboard(w, h, px, cfg, params)` | `PuzzleBoardDetection` (throws on error) |
| `detect_puzzleboard_with_corners(w, h, px, corners, params)` | `PuzzleBoardDetection` (throws on error) |
| `detect_puzzleboard_best(w, h, px, configs)` | `PuzzleBoardDetection` (throws on all-fail) |
| `diagnose_puzzleboard(w, h, px, cfg, params)` | `{ result: PuzzleBoardDetection \| null, diagnostics: PuzzleBoardDiagnostics }` |
| `diagnose_puzzleboard_with_corners(w, h, px, corners, params)` | same as `diagnose_puzzleboard` |
| `detect_marker_board(w, h, px, cfg, params)` | `MarkerBoardDetection \| null` |
| `detect_marker_board_with_corners(w, h, px, corners, params)` | `MarkerBoardDetection \| null` |
| `detect_marker_board_best(w, h, px, configs)` | `MarkerBoardDetection \| null` |
| `diagnose_marker_board(w, h, px, cfg, params)` | `{ result: MarkerBoardDetection \| null, diagnostics: MarkerBoardDiagnostics }` |
| `diagnose_marker_board_with_corners(w, h, px, corners, params)` | same as `diagnose_marker_board` |
| `marker_board_sweep_for_board(spec)` | `MarkerBoardParams[]` — pass to `detect_marker_board_best` |
| `rgba_to_gray(rgba, w, h)` | `Uint8Array` (BT.601) |
| `render_chessboard_png(inner_rows, inner_cols, square_mm, dpi)` | `Uint8Array` — encoded PNG |
| `render_charuco_png(rows, cols, square_mm, marker_size_rel, dict_name, dpi)` | `Uint8Array` |
| `render_marker_board_png(inner_rows, inner_cols, square_mm, dpi)` | `Uint8Array` |
| `render_puzzleboard_png(rows, cols, square_mm, dpi)` | `Uint8Array` |
| `render_chessboard_bundle(inner_rows, inner_cols, square_mm, dpi)` | `GeneratedTargetBundle` — `{ json_text, svg_text, png_bytes, dxf_text }` |
| `render_charuco_bundle(rows, cols, square_mm, marker_size_rel, dict_name, dpi)` | `GeneratedTargetBundle` |
| `render_marker_board_bundle(inner_rows, inner_cols, square_mm, dpi)` | `GeneratedTargetBundle` |
| `render_puzzleboard_bundle(rows, cols, square_mm, dpi)` | `GeneratedTargetBundle` |
| `render_target_bundle_json(doc)` | `GeneratedTargetBundle` — full `PrintableTargetDocument`: page size, orientation, margin and every spec field |
| `default_chess_config()`, `default_chessboard_params()`, `default_puzzleboard_params(rows, cols)`, `default_marker_board_params()` | baseline configs |

## Tuning difficult cases

- Always prefer `detect_*_best` over `detect_*` — the 3-config sweep
  solves most common tuning needs without writing code.
- For blurry / low-contrast inputs, lower the chess threshold in one
  of the sweep configs — e.g. `chess.threshold = 8.0`. Since
  chess-corners 1.x the threshold is a plain number: an absolute floor
  on the raw ChESS response (there is no longer a tagged
  `{ absolute } | { relative }` form).
- For small markers (< 12 px across), resize the source canvas up before
  calling `detect_charuco*` — WASM does not upscale for you.
- Open the [per-detector READMEs][facade] / the [book tuning chapter][tune]
  for parameter-by-parameter guidance. Every knob has the same meaning as
  in the Rust facade.

[facade]: https://docs.rs/calib-targets
[tune]: https://vitalyvorobyev.github.io/calib-targets-rs/tuning.html

## Limitations

- **One target per image.** Same as the Rust facade; multiple boards in
  one frame are not disambiguated.
- **No fisheye support.** Moderate distortion is handled; severe wide-angle
  optics are not.
- **Grayscale only.** Convert from RGBA with `rgba_to_gray` before
  calling any detector.
- **No threads.** The WASM build is single-threaded; heavy detection on
  4K images may exceed 100 ms per call. Consider Web Workers.

Diagnostics are available for the three compound targets:
`diagnose_charuco`, `diagnose_marker_board`, and `diagnose_puzzleboard`
(plus their `_with_corners` counterparts) return a
`{ result, diagnostics }` object. `diagnostics` is produced even when
detection fails, so overlay tools can render the failure mode. The
diagnostics payloads carry a looser stability promise than the typed
results — see `typescript-extras.d.ts`.

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
