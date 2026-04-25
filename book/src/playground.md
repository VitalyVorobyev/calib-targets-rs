# Interactive Playground

The same detectors shipped in the Rust facade — chessboard, ChArUco,
PuzzleBoard, and marker board — also run **directly in the browser** via
WebAssembly. The npm package is
[`@vitavision/calib-targets`](https://www.npmjs.com/package/@vitavision/calib-targets);
the playground below is a thin React UI on top of it. No data leaves your
machine: detection happens in the WASM module loaded into this page.

<iframe
  id="calib-targets-playground"
  src="./playground/"
  title="calib-targets WebAssembly playground"
  loading="lazy"
  allow="clipboard-read; clipboard-write"
  style="width: 100%; height: 900px; border: 1px solid #ddd; border-radius: 4px;">
</iframe>

<noscript>The interactive playground requires JavaScript and WebAssembly.</noscript>

## What it does

| Surface | Description |
|---|---|
| **Image input** | Drop or browse a file, or generate a synthetic chessboard / ChArUco / marker / PuzzleBoard target in WASM. |
| **Detection mode** | Switch between corner detection and the four target detectors. |
| **3-config sweep** | Toggle `detect_*_best` to try the built-in 3-config preset and keep the best result. |
| **Live tuning** | ChESS threshold / NMS / pyramid plus per-detector knobs (board dims, dictionary, marker size, board size, bit confidence). |
| **Overlays** | Detected corners colour-coded by grid position; PuzzleBoard edge bits drawn at decoded edge midpoints. |
| **JSON dump** | Toggle the raw `serde_json` payload returned by the WASM call — the same shape the Rust facade emits. |

## Running locally

If the embedded iframe fails to load (older browsers without
`WebAssembly` or `ES modules` support), build and run the demo standalone:

```bash
scripts/build-wasm.sh                       # populates demo/pkg/
cd demo && bun install && bun run dev       # http://localhost:5173
```

To use the same WASM module from your own web app:

```bash
npm install @vitavision/calib-targets
```

```typescript
import init, {
  default_chess_config,
  default_chessboard_params,
  detect_chessboard,
  rgba_to_gray,
} from "@vitavision/calib-targets";

await init();
const gray = rgba_to_gray(rgba, width, height);
const result = detect_chessboard(
  width, height, gray,
  default_chess_config(),
  default_chessboard_params(),
);
```

The full TypeScript surface — `default_*_params(...)`,
`*_sweep_*(...)`, `render_*_png(...)`, and `list_aruco_dictionaries()` —
is documented in the package README and ships as `.d.ts` declarations
alongside the WASM module.
