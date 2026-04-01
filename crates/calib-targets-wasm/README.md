# calib-targets-wasm

WebAssembly bindings for [calib-targets](https://github.com/VitalyVorobyev/calib-targets-rs) calibration target detection.

Detect chessboard, ChArUco, and marker board calibration targets directly in the browser from grayscale or RGBA images.

## Installation

```bash
npm install calib-targets-wasm
```

Or use the locally built package:

```bash
wasm-pack build crates/calib-targets-wasm --target web --release
# Output in crates/calib-targets-wasm/pkg/
```

## Quick start

```typescript
import init, {
  detect_chessboard,
  detect_corners,
  rgba_to_gray,
  default_chess_config,
  default_chessboard_params,
} from "calib-targets-wasm";

// Initialize the WASM module (call once)
await init();

// Load an image onto a canvas and extract pixel data
const canvas = document.createElement("canvas");
const ctx = canvas.getContext("2d")!;
// ... draw image to canvas ...
const imageData = ctx.getImageData(0, 0, canvas.width, canvas.height);

// Convert RGBA to grayscale
const gray = rgba_to_gray(
  new Uint8Array(imageData.data.buffer),
  canvas.width,
  canvas.height,
);

// Detect chessboard corners
const chessCfg = default_chess_config();
const params = default_chessboard_params();
const result = detect_chessboard(canvas.width, canvas.height, gray, chessCfg, params);

if (result) {
  console.log(`Detected ${result.detection.corners.length} corners`);
  for (const corner of result.detection.corners) {
    console.log(`  (${corner.position.x}, ${corner.position.y}) grid=(${corner.grid?.i}, ${corner.grid?.j})`);
  }
}
```

## API

All detection functions accept grayscale `Uint8Array` pixel buffers (row-major, length = width * height). Use `rgba_to_gray` to convert from canvas RGBA format.

### Functions

| Function | Description |
|---|---|
| `detect_corners(w, h, pixels, chess_cfg)` | Detect ChESS corners. Returns `Corner[]`. |
| `detect_chessboard(w, h, pixels, chess_cfg, params)` | Detect chessboard grid. Returns `ChessboardDetectionResult \| null`. |
| `detect_charuco(w, h, pixels, chess_cfg, params)` | Detect ChArUco board. Returns `CharucoDetectionResult`. Throws on error. |
| `detect_marker_board(w, h, pixels, chess_cfg, params)` | Detect marker board. Returns `MarkerBoardDetectionResult \| null`. |
| `rgba_to_gray(rgba, w, h)` | Convert RGBA buffer to grayscale (BT.601). |
| `default_chess_config()` | Default ChESS corner detector config. |
| `default_chessboard_params()` | Default chessboard detection params. |
| `default_marker_board_params()` | Default marker board params. |

### Configuration

Config objects are plain JavaScript objects matching the Rust serde schema. Use `default_chess_config()` and `default_chessboard_params()` to get baseline configs, then override individual fields:

```typescript
const cfg = default_chess_config();
cfg.threshold_value = 0.15;
cfg.nms_radius = 3;

const params = default_chessboard_params();
params.expected_rows = 7;
params.expected_cols = 9;
params.completeness_threshold = 0.5;
```

### Output format

**Corner** (from `detect_corners`):
```typescript
{ position: { x: number, y: number }, orientation: number, strength: number }
```

**LabeledCorner** (from grid detectors):
```typescript
{
  position: { x: number, y: number },
  grid: { i: number, j: number } | null,
  id: number | null,              // ChArUco corner ID
  target_position: { x: number, y: number } | null,
  score: number
}
```

## Binary size

| Metric | Size |
|---|---|
| Raw `.wasm` | ~436 KB |
| Gzipped | ~195 KB |

The package does not include `rayon` (no threads) or the `image` codec crate. Corner detection uses `chess-corners` compiled in single-threaded mode.

## Building from source

```bash
# Prerequisites: Rust toolchain + wasm-pack
rustup target add wasm32-unknown-unknown
cargo install wasm-pack

# Build
wasm-pack build crates/calib-targets-wasm --target web --release

# Or use the helper script (outputs to demo/pkg/)
scripts/build-wasm.sh
```

## Demo

A React/TypeScript demo app is included at `demo/` in the repository:

```bash
scripts/build-wasm.sh
cd demo && npm install && npm run dev
```

## License

MIT or Apache-2.0, at your option.
