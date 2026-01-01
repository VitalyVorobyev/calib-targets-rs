# calib-targets Python bindings

This repo ships Python bindings for the high-level `calib-targets` facade crate.

## Build & develop

```
pip install maturin
maturin develop
python -c "import calib_targets; print(calib_targets)"
```

## API

The module name is `calib_targets` and it exposes three functions:

- `detect_chessboard(image, *, chess_cfg=None, params=None) -> dict | None`
- `detect_charuco(image, *, board, chess_cfg=None, params=None) -> dict`
- `detect_marker_board(image, *, chess_cfg=None, params=None) -> dict | None`

Inputs and config:

- `image` must be a 2D `numpy.ndarray` with `dtype=uint8` (grayscale).
- `chess_cfg` accepts `None`, a dict of overrides, or a `ChessConfig` instance.
- `params` accepts `None`, a dict of overrides, or typed params classes.
- `board` is a ChArUco board spec dict: `rows`, `cols`, `cell_size`,
  `marker_size_rel`, `dictionary`, `marker_layout`.

Notes:

- Typed config classes exposed by the module include `ChessConfig`,
  `ChessCornerParams`, `CoarseToFineParams`, `PyramidParams`,
  `ChessboardParams`, `OrientationClusteringParams`, `GridGraphParams`,
  `CharucoDetectorParams`, `ScanDecodeConfig`, `MarkerBoardParams`,
  `CircleScoreParams`, and `CircleMatchParams`.
- Dict overrides can be partial; unknown keys raise `ValueError` listing valid keys.
- `target_position` is populated only when the board layout includes a valid
  `cell_size` and alignment succeeds. For marker boards, set
  `params["layout"]["cell_size"]` to your square size.

## Examples

The example scripts load an image with Pillow (install it once):

```
pip install pillow
python python/examples/detect_chessboard.py path/to/image.png
python python/examples/detect_charuco.py path/to/image.png
python python/examples/detect_marker_board.py path/to/image.png
```
