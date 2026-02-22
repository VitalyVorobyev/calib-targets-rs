# calib-targets Python bindings

These bindings expose a native-feeling Python API backed by Rust (`PyO3` + `maturin`).

## Build & develop

```bash
pip install maturin
maturin develop
python -c "import calib_targets as ct; print(ct.detect_chessboard)"
```

## Public API

Top-level detectors return typed dataclasses:

- `detect_chessboard(image, *, chess_cfg=None, params=None) -> ChessboardDetectionResult | None`
- `detect_charuco(image, *, chess_cfg=None, params) -> CharucoDetectionResult`
- `detect_marker_board(image, *, chess_cfg=None, params=None) -> MarkerBoardDetectionResult | None`

Configuration is typed-only (dataclasses):

- `ChessConfig`, `ChessCornerParams`, `CoarseToFineParams`, `PyramidParams`
- `ChessboardParams`, `OrientationClusteringParams`, `GridGraphParams`
- `CharucoBoardSpec`, `CharucoDetectorParams`, `ScanDecodeConfig`
- `MarkerCircleSpec`, `MarkerBoardLayout`, `CircleScoreParams`, `CircleMatchParams`, `MarkerBoardParams`

Enums and literals:

- `TargetKind`, `CirclePolarity`, `MarkerLayout`
- `DictionaryName` (Literal) and `DICTIONARY_NAMES`

## Inputs

- `image` must be a 2D `numpy.ndarray` with `dtype=uint8`.
- `chess_cfg` must be `ChessConfig | None`.
- `params` must be typed params dataclasses (or `None` where allowed).
- Dict/mapping inputs are intentionally rejected in the new API.

## Results and compatibility

Result models are dataclasses with attribute access and editor navigation.
Every config/result model provides:

- `to_dict()`
- `from_dict(...)`

This is the compatibility path for JSON pipelines and legacy dict-based code.

## Migration guide

| Old usage | New usage |
| --- | --- |
| `detect_chessboard(img, params={"min_corners": 16})` | `detect_chessboard(img, params=ChessboardParams(min_corners=16))` |
| `detect_charuco(..., params={"board": {...}})` | `detect_charuco(..., params=CharucoDetectorParams(board=CharucoBoardSpec(...)))` |
| `result["detection"]["corners"]` | `result.detection.corners` |
| N/A | `result.to_dict()` / `ResultType.from_dict(...)` |

## Examples

```bash
pip install pillow
python examples/detect_chessboard.py path/to/image.png
python examples/detect_charuco.py path/to/image.png
python examples/detect_marker_board.py path/to/image.png
```

## Implementation note

The compiled module is internal (`calib_targets._core`).
Public API stability is guaranteed only for top-level `calib_targets` exports.
