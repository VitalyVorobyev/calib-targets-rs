# Generate and detect a target on a screen

Use a grayscale e-reader or monitor as a flat ChArUco target. This walkthrough
takes you from the display specifications to 2D image points and 2D board
points in millimeters. You can run the detection step on the generated PNG
before photographing the screen.

## 1. Get the display dimensions

Find the native width and height in the orientation you will use, and the
physical pitch of one square pixel. If the panel is specified in pixels per
inch, divide 25.4 by that number. This example uses a 1200 × 1600 portrait
display with a pitch of 0.085 mm per pixel.

## 2. Generate the target

Install the Python package and Pillow for the detection step. The generation
command also works with the Rust CLI installed via `cargo install calib-targets`.

```bash
python -m pip install "calib-targets>=0.16.2" pillow
calib-targets gen-screen charuco --rows 5 --cols 7 \
  --square-size-mm 12 --marker-size-rel 0.75 --dictionary DICT_4X4_50 \
  --width-px 1200 --height-px 1600 --pixel-pitch-mm 0.085 \
  --margin-px 20 --out-stem reader-board
```

You can also open the [playground](playground.md), select **Generate → Screen**,
enter the same board and display settings, and download PNG and JSON.

The example fits a 142 px square pitch, or 12.07 mm. The marker occupies 108 px,
with six 18 px cells across and 17 px of white padding on each side. The JSON
records the realized square size and marker ratio `108 / 142`. Both values go
into the detector configuration.

## 3. Display and capture

Open the PNG in an image viewer with a 1:1 pixel mode. Use full screen and
disable fit-to-window and zoom so that one PNG pixel maps to one physical
display pixel. If the viewer automatically rescales the image, switch to one
that offers native-pixel playback.

Photograph the displayed board, keeping the display flat and the checker
corners in focus. For an initial check, run the next step directly on
`reader-board.png`. Camera sampling and the physical display can still
introduce moiré in photographs.

## 4. Load the realized geometry and detect

Save this as `detect_screen_board.py` next to the downloaded JSON.

```python
import json
import sys
from pathlib import Path

import calib_targets as ct
import numpy as np
from PIL import Image

sidecar = ct.ScreenTargetMetadata.from_dict(
    json.loads(Path("reader-board.json").read_text())
)
target = sidecar.target
assert isinstance(target, ct.CharucoTargetSpec)

board = ct.CharucoBoardSpec(
    rows=target.rows,
    cols=target.cols,
    cell_size=target.square_size_mm,
    marker_size_rel=target.marker_size_rel,
    dictionary=target.dictionary,
    marker_layout=target.marker_layout,
    border_bits=target.border_bits,
)

with Image.open(sys.argv[1]) as image:
    gray = np.asarray(image.convert("L"), dtype=np.uint8)

result = ct.detect_charuco(gray, params=ct.CharucoParams(board=board))
if result is None:
    raise SystemExit("No ChArUco board detected")

print(f"Detected {len(result.corners)} corners")
for corner in result.corners:
    print(corner.id, corner.position, corner.target_position)
```

```bash
python detect_screen_board.py reader-board.png
```

The first point in each pair is in image pixels. The second is in millimeters
in the ChArUco board frame. Pass the same script a photograph to get
correspondences from the camera view.

Screen mode also supports Chessboard and PuzzleBoard. See the
[screen-generation reference](printable.md#targets-for-grayscale-screens) for
their geometry and fitting rules.
