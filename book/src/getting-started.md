# Getting Started: From Target to Calibration Data

This tutorial walks you through the complete workflow:

1. Choose the right calibration target for your use case.
2. Generate a printable target file.
3. Print it correctly.
4. Write detection code in Python or Rust.

No prior knowledge of the library is assumed.

---

## Step 1: Choose your target type

| Target | Best for | Requires |
|---|---|---|
| **Chessboard** | Quick start, simple intrinsic calibration | Nothing — no markers |
| **ChArUco** | Robust calibration, partial visibility OK, absolute corner IDs | ArUco dictionary |
| **Marker board** | Scenes where a full chessboard is impractical | Custom layout |

**If you are unsure, start with ChArUco.** It combines the subpixel accuracy of chessboard
corners with the robustness of ArUco markers. Each detected corner carries a unique ID and
a real-world position in millimeters, so partial views of the board are useful and board
orientation is unambiguous.

If you want the absolute simplest path and only need basic intrinsic calibration, use the
plain chessboard.

---

## Step 2: Generate a printable target

Pick the language you are most comfortable with. All paths produce the same three output
files: `<stem>.json`, `<stem>.svg`, `<stem>.png`.

### Python

```bash
pip install calib-targets
```

```python
import calib_targets as ct

# ChArUco: 5 rows × 7 cols, 20 mm squares, DICT_4X4_50 markers
doc = ct.PrintableTargetDocument(
    target=ct.CharucoTargetSpec(
        rows=5,
        cols=7,
        square_size_mm=20.0,
        marker_size_rel=0.75,
        dictionary="DICT_4X4_50",
    )
)
written = ct.write_target_bundle(doc, "my_board/charuco_a4")
print(written.png_path)   # open this to preview
```

For a plain chessboard instead:

```python
doc = ct.PrintableTargetDocument(
    target=ct.ChessboardTargetSpec(
        inner_rows=6,
        inner_cols=8,
        square_size_mm=25.0,
    )
)
ct.write_target_bundle(doc, "my_board/chessboard_a4")
```

### CLI (from source)

```bash
# list available ArUco dictionaries
cargo run -p calib-targets-cli -- list-dictionaries

# initialise a spec, validate, then render
cargo run -p calib-targets-cli -- init charuco \
  --out my_board/charuco_a4.json \
  --rows 5 --cols 7 \
  --square-size-mm 20 \
  --marker-size-rel 0.75 \
  --dictionary DICT_4X4_50

cargo run -p calib-targets-cli -- validate --spec my_board/charuco_a4.json
cargo run -p calib-targets-cli -- generate  --spec my_board/charuco_a4.json \
  --out-stem my_board/charuco_a4
```

### Rust

```rust,no_run
use calib_targets::printable::{write_target_bundle, PrintableTargetDocument};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let doc = PrintableTargetDocument::load_json("my_board/charuco_a4.json")?;
    let written = write_target_bundle(&doc, "my_board/charuco_a4")?;
    println!("{}", written.png_path.display());
    Ok(())
}
```

See [calib-targets-print](printable.md) for the full JSON schema and more options.

---

## Step 3: Print it

Open `my_board/charuco_a4.svg` (or the `.png` at the generated DPI) in your
printer dialog:

- Set scale to **100%** / "actual size". Disable "fit to page", "shrink to fit", or
  any equivalent driver option.
- After printing, **measure one square width with a ruler or caliper** and confirm it
  matches `square_size_mm` (20 mm in the example above).
- If the size is wrong, fix the print dialog and reprint — do not compensate in code.
- Mount or tape the target flat to a rigid surface. Warping or bowing degrades
  calibration accuracy significantly.
- Prefer the SVG for professional print workflows; use the PNG for quick office
  printing (check the DPI matches your printer resolution).

---

## Step 4: Detect corners in Python

The board spec used for detection must match the one used for generation exactly.

```python
import numpy as np
from PIL import Image
import calib_targets as ct

def load_gray(path: str) -> np.ndarray:
    return np.asarray(Image.open(path).convert("L"), dtype=np.uint8)

# Board spec — must match the generated target
board = ct.CharucoBoardSpec(
    rows=5,
    cols=7,
    cell_size=20.0,          # mm; gives target_position in mm
    marker_size_rel=0.75,
    dictionary="DICT_4X4_50",
    marker_layout=ct.MarkerLayout.OPENCV_CHARUCO,
)

params = ct.CharucoDetectorParams.for_board(board)

image = load_gray("frame.png")

try:
    result = ct.detect_charuco(image, params=params)
except RuntimeError as exc:
    print(f"Detection failed: {exc}")
    raise SystemExit(1)

corners = result.detection.corners
print(f"Detected {len(corners)} corners, {len(result.markers)} markers")

# Collect point pairs for solvePnP / calibrateCamera
obj_pts = []  # 3-D object points (Z = 0 for planar board)
img_pts = []  # 2-D image points
for c in corners:
    if c.target_position is not None:
        x_mm, y_mm = c.target_position
        obj_pts.append([x_mm, y_mm, 0.0])
        img_pts.append(c.position)

print(f"Point pairs ready for calibration: {len(obj_pts)}")
```

For a plain chessboard:

```python
result = ct.detect_chessboard(image)
if result is None:
    raise SystemExit("No chessboard detected")

corners = result.detection.corners
print(f"Detected {len(corners)} corners")
# target_position is None for chessboard — assign object points by grid index
for c in corners:
    i, j = c.grid          # (col, row), origin top-left
    obj_pts.append([i * square_size_mm, j * square_size_mm, 0.0])
    img_pts.append(c.position)
```

---

## Step 5: Detect corners in Rust

```toml
# Cargo.toml
[dependencies]
calib-targets = "0.4"
image = "0.25"
```

```rust,no_run
use calib_targets::charuco::{CharucoBoardSpec, CharucoDetectorParams, MarkerLayout};
use calib_targets::detect::{self, ChessConfig};
use image::ImageReader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let img = ImageReader::open("frame.png")?.decode()?.to_luma8();

    let board = CharucoBoardSpec {
        rows: 5,
        cols: 7,
        cell_size: 20.0,      // mm
        marker_size_rel: 0.75,
        dictionary: "DICT_4X4_50".parse()?,
        marker_layout: MarkerLayout::OpencvCharuco,
        ..Default::default()
    };

    let params = CharucoDetectorParams::for_board(&board);
    let chess_cfg: ChessConfig = detect::default_chess_config();

    let result = detect::detect_charuco(&img, &chess_cfg, params)?;
    println!(
        "corners: {}, markers: {}",
        result.detection.corners.len(),
        result.markers.len()
    );

    // Collect point pairs
    for c in &result.detection.corners {
        if let Some(tp) = c.target_position {
            let obj = [tp[0], tp[1], 0.0_f32];
            let img = c.position;
            // pass (obj, img) to your calibration solver
            let _ = (obj, img);
        }
    }
    Ok(())
}
```

---

## Next steps

| Topic | Where |
|---|---|
| Detection parameters explained | [Tuning the Detector](tuning.md) |
| Detection fails or gives errors | [Troubleshooting](troubleshooting.md) |
| What every output field means | [Understanding Results](output.md) |
| Full printable-target reference | [calib-targets-print](printable.md) |
| ChArUco pipeline internals | [ChArUco crate](charuco.md) |
