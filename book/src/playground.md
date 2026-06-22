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
  style="width: 100%; height: 900px; border: 1px solid #ddd; border-radius: 4px;">
</iframe>

<noscript>The interactive playground requires JavaScript and WebAssembly.</noscript>

## What it does

| Surface | Description |
|---|---|
| **Image input** | Drop or browse a file; or pick a bundled public sample (chessboard, ChArUco, marker board, PuzzleBoard); or generate a synthetic target on-the-fly in WASM (Generate tab). |
| **Target family** | Switch between corner detection and the four target detectors (Chessboard, ChArUco, Marker board, PuzzleBoard). |
| **Board geometry** | For ChArUco, marker, and PuzzleBoard targets: configure rows, cols, and (ChArUco) ArUco dictionary directly in the panel. |
| **Core params** | Override `min_corner_strength`, `min_labeled_corners`, and `max_components` — the three params shared across all detector families. |
| **Multi-config sweep** | Toggle `detect_*_best` to run the built-in 3-config preset and keep the best result. |
| **Overlays** | Red corners, light-blue grid edges, yellow origin ring, and green far-corner ring drawn in image pixel coordinates. Toggled per-layer. |
| **Zoom / pan** | Scroll to zoom (up to 32×, pixel-crisp above 4×), drag to pan, double-click to fit. Hover a corner for an (i, j) / id / score tooltip. |
| **Synthetic generation** | `render_*_png` WASM functions produce a full-resolution target PNG; loading it auto-configures the matching detector. |

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
