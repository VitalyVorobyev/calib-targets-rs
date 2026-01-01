# calib-targets Python bindings

This repo ships Python bindings for the high-level `calib-targets` facade crate.

## Build & develop

```
pip install maturin
maturin develop
python -c "import calib_targets; print(calib_targets)"
```

## Examples

The example scripts load an image with Pillow (install it once):

```
pip install pillow
python python/examples/detect_chessboard.py path/to/image.png
python python/examples/detect_charuco.py path/to/image.png
python python/examples/detect_marker_board.py path/to/image.png
```
