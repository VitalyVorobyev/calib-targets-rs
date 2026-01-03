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
- `detect_charuco(image, *, chess_cfg=None, params) -> dict`
- `detect_marker_board(image, *, chess_cfg=None, params=None) -> dict | None`

Inputs and config:

- `image` must be a 2D `numpy.ndarray` with `dtype=uint8` (grayscale).
- `chess_cfg` accepts `None`, a dict of overrides, or a `ChessConfig` instance.
- `params` accepts `None`, a dict of overrides, or typed params classes.
- `detect_charuco` requires `params` and the board lives in `params.board`
  (or `params["board"]` for dict overrides).

Notes:

- Typed config classes exposed by the module include `ChessConfig`,
  `ChessCornerParams`, `CoarseToFineParams`, `PyramidParams`,
  `ChessboardParams`, `OrientationClusteringParams`, `GridGraphParams`,
  `CharucoDetectorParams`, `ScanDecodeConfig`, `MarkerBoardParams`,
  `CircleScoreParams`, `CircleMatchParams`, `CharucoBoardSpec`,
  `MarkerBoardLayout`, and `MarkerCircleSpec`.
- Dict overrides can be partial; unknown keys raise `ValueError` listing valid keys.
- `target_position` is populated only when the board layout includes a valid
  `cell_size` and alignment succeeds. For marker boards, set
  `params.layout.cell_size` or `params["layout"]["cell_size"]`.
- Typed config objects expose mutable attributes (set fields directly in Python).

## Output schema (authoritative)

All outputs are JSON-compatible dicts/lists with basic Python types.

Common shapes:

- `Point2`: `[x, y]` (floats).
- `GridCoords`: `{"i": int, "j": int}`.
- `TargetDetection`:
  - `kind`: `"chessboard" | "charuco" | "checkerboard_marker"`.
  - `corners`: list of `LabeledCorner`.
- `LabeledCorner`:
  - `position`: `Point2`.
  - `grid`: `GridCoords | None`.
  - `id`: `int | None`.
  - `target_position`: `Point2 | None`.
  - `score`: `float`.

`detect_chessboard(...) -> None | dict`:

- `detection`: `TargetDetection`.
- `inliers`: `list[int]` (indices into `detection.corners`).
- `orientations`: `[float, float] | None`.
- `debug`:
  - `orientation_histogram`: `{"bin_centers": list[float], "values": list[float]} | None`.
  - `graph`: `{"nodes": list[{"position": Point2, "neighbors": list[{"index": int, "direction": str, "distance": float}]}]} | None`.

`detect_charuco(...) -> dict`:

- `detection`: `TargetDetection`.
- `markers`: list of `MarkerDetection`.
- `alignment`: `GridAlignment`.

`MarkerDetection`:

- `id`: `int`.
- `gc`: `{"gx": int, "gy": int}`.
- `rotation`: `int`.
- `hamming`: `int`.
- `score`: `float`.
- `border_score`: `float`.
- `code`: `int` (packed marker bits).
- `inverted`: `bool`.
- `corners_rect`: `[Point2, Point2, Point2, Point2]` (TL, TR, BR, BL).
- `corners_img`: `[Point2, Point2, Point2, Point2] | None` (TL, TR, BR, BL).

`GridAlignment`:

- `transform`: `{"a": int, "b": int, "c": int, "d": int}`.
- `translation`: `[int, int]`.

`detect_marker_board(...) -> None | dict`:

- `detection`: `TargetDetection`.
- `inliers`: `list[int]`.
- `circle_candidates`: list of `CircleCandidate`.
- `circle_matches`: list of `CircleMatch`.
- `alignment`: `GridAlignment | None`.
- `alignment_inliers`: `int`.

`CircleCandidate`:

- `center_img`: `Point2`.
- `cell`: `GridCoords`.
- `polarity`: `"white" | "black"`.
- `score`: `float`.
- `contrast`: `float`.

`CircleMatch`:

- `expected`: `MarkerCircleSpec`.
- `matched_index`: `int | None`.
- `distance_cells`: `float | None`.
- `offset_cells`: `{"di": int, "dj": int} | None`.

`MarkerCircleSpec`:

- `cell`: `GridCoords`.
- `polarity`: `"white" | "black"`.

## Examples

The example scripts load an image with Pillow (install it once):

```
pip install pillow
python examples/detect_chessboard.py path/to/image.png
python examples/detect_charuco.py path/to/image.png
python examples/detect_marker_board.py path/to/image.png
```
