# calib-targets — Python bindings

[![Book](https://img.shields.io/badge/book-getting--started-blue)](https://vitalyvorobyev.github.io/calib-targets-rs/getting-started.html)

Native-feeling Python API for the `calib-targets` Rust workspace.
Detects chessboards, ChArUco, PuzzleBoard, and marker boards, and
generates printable target bundles (JSON + SVG + PNG). Built with
[PyO3](https://pyo3.rs) + [maturin](https://www.maturin.rs).

Python package name: `calib_targets` (the Rust crate is
`calib-targets-py`).

## Install

```bash
# From source — this repo:
uv pip install maturin
uv run maturin develop --release -m crates/calib-targets-py/Cargo.toml

# Or from PyPI (pre-built wheels):
pip install calib-targets
```

## Hello world

```python
import numpy as np
from PIL import Image
import calib_targets as ct

image = np.asarray(Image.open("board.png").convert("L"), dtype=np.uint8)
result = ct.detect_chessboard_best(image, [ct.ChessboardParams()])
if result is not None:
    print(f"labelled {len(result.detection.corners)} corners")
```

## End-to-end round-trip per target type

Each snippet covers: **generate** a printable target → **load** the PNG
→ **detect** → **export** detection to JSON.

Runnable scripts at
[`crates/calib-targets-py/examples/`](examples/). Use any of
them as a starting point.

### Chessboard

```python
import io, json
import numpy as np
from PIL import Image
import calib_targets as ct

# 1. Generate target.
doc = ct.PrintableTargetDocument(
    target=ct.ChessboardTargetSpec(inner_rows=7, inner_cols=9, square_size_mm=20.0),
    page=ct.PageSpec(size=ct.PageSize.custom(width_mm=220.0, height_mm=180.0), margin_mm=10.0),
    render=ct.RenderOptions(png_dpi=150),
)
bundle = ct.render_target_bundle(doc)

# 2. Load as grayscale numpy array.
image = np.asarray(Image.open(io.BytesIO(bundle.png_bytes)).convert("L"), dtype=np.uint8)

# 3. Detect — prefer *_best for robustness.
result = ct.detect_chessboard_best(image, [
    ct.ChessboardParams(),
    ct.ChessboardParams(chess=ct.ChessConfig(threshold_value=0.15)),
    ct.ChessboardParams(chess=ct.ChessConfig(threshold_value=0.08)),
])

# 4. Export detection to JSON.
print(json.dumps(result.to_dict(), indent=2)[:200])
```

Runnable: [`examples/chessboard_roundtrip.py`](examples/chessboard_roundtrip.py).

### ChArUco

```python
import calib_targets as ct
# (synthesise PNG as above; build matching board spec)
board = ct.CharucoBoardSpec(
    rows=5, cols=7, cell_size=1.0, marker_size_rel=0.75,
    dictionary="DICT_4X4_50", marker_layout=ct.MarkerLayout.OPENCV_CHARUCO,
)
params = ct.CharucoParams(
    board=board, px_per_square=60.0,
    chessboard=ct.ChessboardParams(),
    max_hamming=2, min_marker_inliers=4,
)
result = ct.detect_charuco(image, params=params)   # raises on failure
print(len(result.detection.corners), "corners,", len(result.markers), "markers")
```

Runnable: [`examples/charuco_roundtrip.py`](examples/charuco_roundtrip.py).

### Marker board

```python
circles = (
    ct.MarkerCircleSpec(i=3, j=2, polarity=ct.CirclePolarity.WHITE),
    ct.MarkerCircleSpec(i=4, j=2, polarity=ct.CirclePolarity.BLACK),
    ct.MarkerCircleSpec(i=4, j=3, polarity=ct.CirclePolarity.WHITE),
)
layout = ct.MarkerBoardLayout(rows=6, cols=8, cell_size=1.0, circles=circles)
params = ct.MarkerBoardParams(layout=layout, chessboard=ct.ChessboardParams())
result = ct.detect_marker_board(image, params=params)
```

Runnable: [`examples/markerboard_roundtrip.py`](examples/markerboard_roundtrip.py).

### PuzzleBoard

```python
params = ct.default_puzzleboard_params(rows=10, cols=10)
result = ct.detect_puzzleboard(image, params=params)
# Every corner has an absolute master ID: result.detection.corners[0].id
```

Runnable: [`examples/puzzleboard_roundtrip.py`](examples/puzzleboard_roundtrip.py).

## Inputs

- `image: numpy.ndarray[uint8]` with shape `(h, w)`. Grayscale only;
  convert RGB upstream (`Image.convert("L")`).
- `chess_cfg: ChessConfig | None` — overrides the default ChESS corner
  detector.
- `params: *Params` — typed dataclass matching the detector. Dict inputs
  are rejected; use the typed classes.

## Outputs

Every detection result is a typed dataclass with full attribute access,
editor autocomplete, and type stubs. Round-trip through JSON with
`to_dict()` and `from_dict(...)` — the dict schema matches the Rust
crate's `serde_json` output byte-for-byte.

```python
payload = json.dumps(result.to_dict())
# ... later, elsewhere:
restored = ct.ChessboardDetectionResult.from_dict(json.loads(payload))
```

Every config / result type has these methods — `ChessConfig`,
`ChessboardParams`, `CharucoParams`, `PuzzleBoardParams`,
`MarkerBoardParams`, `PrintableTargetDocument`, and all result types.

## Printable targets

One-liner helpers with sensible defaults (A4 portrait, 10 mm margins, 300 DPI):

```python
doc = ct.charuco_document(rows=5, cols=7, square_size_mm=20.0,
                          marker_size_rel=0.75, dictionary="DICT_4X4_50")
written = ct.write_target_bundle(doc, "out/charuco_a4")
print(written.json_path, written.svg_path, written.png_path)
```

Other helpers: `chessboard_document`, `puzzleboard_document`,
`marker_board_document`. Each accepts optional `page=` / `render=`
overrides. For full control, construct `PrintableTargetDocument`
directly with one of the target specs (`ChessboardTargetSpec`,
`CharucoTargetSpec`, `MarkerBoardTargetSpec`, `PuzzleBoardTargetSpec`).

### CLI

`pip install calib-targets` installs a `calib-targets` console script
that mirrors the Rust CLI:

```bash
calib-targets gen puzzleboard --rows 8 --cols 10 --square-size-mm 15 \
    --out-stem puzzle
calib-targets list-dictionaries
calib-targets init chessboard --out spec.json \
    --inner-rows 6 --inner-cols 8 --square-size-mm 20
calib-targets generate --spec spec.json --out-stem my_board
```

See [`testdata/printable/*.json`](../../testdata/printable) for ready-made
spec files; every file is `PrintableTargetDocument.from_dict(
json.load(open(path)))`-compatible.

## Tuning difficult cases

1. Replace `detect_*` with `detect_*_best` and pass a 3-config sweep —
   this is the recommended default.
2. Increase rasterisation / input resolution if cells are smaller than
   ~20 px across.
3. Open the per-detector README for deeper guidance:
   [chessboard][cb], [ChArUco][charuco], [PuzzleBoard][puz],
   [marker][marker]. Python passes all parameters through to Rust, so
   tuning advice applies identically.

[cb]: https://docs.rs/calib-targets-chessboard
[charuco]: https://docs.rs/calib-targets-charuco
[puz]: https://docs.rs/calib-targets-puzzleboard
[marker]: https://docs.rs/calib-targets-marker

## Limitations

- **One target instance per image.** Multiple simultaneous boards are
  not detected; pass cropped sub-images per target.
- **Pinhole-ish optics only.** Moderate radial / perspective distortion
  is handled gracefully; fisheye is not supported.
- **Grayscale uint8 numpy arrays only.** No torch tensors, no GPU.
- **Board PNG / SVG generation for chessboard, ChArUco, marker board,
  and PuzzleBoard is supported; other target kinds are not.**

## Migration from pre-0.7 dict-based API

| Old | New |
|---|---|
| `detect_chessboard(img, params={"min_corner_strength": 0.5})` | `detect_chessboard(img, params=ChessboardParams(min_corner_strength=0.5))` |
| `detect_charuco(..., params={"board": {...}})` | `detect_charuco(..., params=CharucoParams(board=CharucoBoardSpec(...)))` |
| `result["detection"]["corners"]` | `result.detection.corners` |
| `json.dumps(result_dict)` | `json.dumps(result.to_dict())` |

Dict-based configuration is rejected in the new API; use the typed
dataclasses.

## Feature parity vs Rust facade

- `detect_chessboard` / `_all` / `_best` / `_debug` — ✔
- `detect_charuco` / `_best`, `detect_puzzleboard` / `_best`,
  `detect_marker_board` / `_best` — ✔
- Printable targets for all four target kinds — ✔
- `to_dict` / `from_dict` round-trip on every config + result type — ✔

## Implementation note

The compiled Rust module is internal (`calib_targets._core`). Public
API stability is guaranteed only for top-level `calib_targets` exports.

## Links

- Book (getting started + per-target chapters):
  <https://vitalyvorobyev.github.io/calib-targets-rs/>
- Rust facade crate: <https://docs.rs/calib-targets>
- Source + issue tracker:
  <https://github.com/VitalyVorobyev/calib-targets-rs>
