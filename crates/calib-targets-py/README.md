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
- `detect_chessboard_best(image, *, chess_cfg=None, configs=None) -> ChessboardDetectionResult | None`
- `detect_chessboard_debug(image, *, chess_cfg=None, params=None) -> ChessboardDebug`
- `detect_charuco(image, *, chess_cfg=None, params) -> CharucoDetectionResult`
- `detect_charuco_best(image, *, chess_cfg=None, configs=None, board) -> CharucoDetectionResult | None`
- `detect_puzzleboard(image, *, chess_cfg=None, params) -> PuzzleBoardDetectionResult`
- `detect_puzzleboard_best(image, *, chess_cfg=None, configs=None) -> PuzzleBoardDetectionResult | None`
- `detect_marker_board(image, *, chess_cfg=None, params=None) -> MarkerBoardDetectionResult | None`
- `render_target_bundle(document) -> GeneratedTargetBundle`
- `write_target_bundle(document, output_stem) -> WrittenTargetBundle`

Configuration is typed-only (dataclasses):

- `ChessConfig`, `ChessCornerParams`, `CoarseToFineParams`, `PyramidParams`
- `ChessboardParams` — wraps Rust's `DetectorParams` (chessboard v2 shape; 30 flat tuning fields)
- `CharucoBoardSpec`, `CharucoParams`, `ScanDecodeConfig`
- `PuzzleBoardSpec`, `PuzzleBoardParams`, `PuzzleBoardDecodeConfig`
- `MarkerCircleSpec`, `MarkerBoardLayout`, `CircleScoreParams`, `CircleMatchParams`, `MarkerBoardParams`
- `PageSize`, `PageSpec`, `RenderOptions`, `ChessboardTargetSpec`, `CharucoTargetSpec`, `MarkerBoardTargetSpec`, `PuzzleBoardTargetSpec`, `PrintableTargetDocument`

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
| `detect_chessboard(img, params={"min_corner_strength": 0.5})` | `detect_chessboard(img, params=ChessboardParams(min_corner_strength=0.5))` |
| `detect_charuco(..., params={"board": {...}})` | `detect_charuco(..., params=CharucoParams(board=CharucoBoardSpec(...)))` |
| `result["detection"]["corners"]` | `result.detection.corners` |
| N/A | `result.to_dict()` / `ResultType.from_dict(...)` |

Chessboard v2 API migration note (v0.6.0): the Rust side renamed the
chessboard types from `ChessboardDetector` / `ChessboardParams` /
`ChessboardDetectionResult` to `Detector` / `DetectorParams` /
`Detection`. The Python binding kept the historical `ChessboardParams`
/ `ChessboardDetectionResult` class names but the fields inside
`ChessboardParams` now match the v2 flat `DetectorParams` shape — the
former nested `graph` / `graph_cleanup` / `gap_fill` /
`local_homography` sub-params are gone.

## Examples

```bash
pip install pillow
python examples/detect_chessboard.py path/to/image.png
python examples/detect_charuco.py path/to/image.png
python examples/detect_puzzleboard.py path/to/image.png
python examples/detect_marker_board.py path/to/image.png
python examples/generate_printable.py tmpdata/printable/charuco_a4
```

For the canonical printable-target JSON model, the repo-local CLI flow, and
print-at-100%-scale guidance, see the workspace printable-target guide:
https://vitalyvorobyev.github.io/calib-targets-rs/printable.html

## Implementation note

The compiled module is internal (`calib_targets._core`).
Public API stability is guaranteed only for top-level `calib_targets` exports.
