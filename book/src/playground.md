# Interactive Playground

The same detectors shipped in the Rust facade — chessboard, ChArUco,
PuzzleBoard, and marker board — also run **directly in the browser** via
WebAssembly. The npm package is
[`@vitavision/calib-targets`](https://www.npmjs.com/package/@vitavision/calib-targets);
the playground below is a thin React UI on top of it. No data leaves your
machine: detection happens in the WASM module loaded into this page.

<iframe
  id="calib-targets-playground"
  src="../demo/"
  title="calib-targets WebAssembly playground"
  loading="lazy"
  allow="clipboard-read; clipboard-write"
  style="width: 100%; height: 80vh; min-height: 560px; max-height: 900px; border: 1px solid color-mix(in srgb, currentColor 18%, transparent); border-radius: 6px;">
</iframe>

<noscript>The interactive playground requires JavaScript and WebAssembly.</noscript>

## What it does

| Surface | Description |
|---|---|
| **Image input** | Drop an image anywhere on the page or browse for one; pick a bundled public sample (chessboard, ChArUco, marker board, PuzzleBoard); or generate a synthetic target in WASM (Generate tab). |
| **Target** | Corners only, Chessboard, ChArUco, Marker board or PuzzleBoard. The board geometry (rows and columns, ArUco dictionary, marker size) sits right under it. |
| **Detector settings** | The ChESS corner detector (threshold, refiner, upscaling, position uncertainty) and each detector's parameters, as forms generated from the JSON Schemas shipped in the npm package. |
| **Multi-config sweep** | Run the built-in presets and keep the best result. Optional "detect on every change" switch; otherwise press Detect. Detection runs in a Web Worker and can be cancelled. |
| **Result** | Corner count, timing, ChArUco marker count, the PuzzleBoard decode summary (bit error rate, edges matched, master origin), marker-board circle matches, and a sortable corner table. Click a corner, in the image or the table, for its `(i, j)`, id, score and uncertainty. Download the raw result as JSON. |
| **Overlays** | Corners, the lattice edges with `(i, j)` labels, ArUco marker quads, marker-board circles, PuzzleBoard edge bits and, when the result carries a position covariance, uncertainty ellipses drawn at a stated multiple of 1σ. Toggle each from the layers menu. |
| **Zoom / pan** | Scroll to zoom, drag to pan, double-click to fit; `+`, `-`, `0` (fit), `1` (100 %) and the arrow keys work with the image focused. |
| **Generate** | Chessboard, ChArUco, marker board, PuzzleBoard or PuzzlePole, at a chosen square size and DPI. Rows and columns are labelled by what they count (inner corners or squares). The target loads as the image with the detector set to the same board; download it as PNG, SVG, JSON or DXF. |

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
