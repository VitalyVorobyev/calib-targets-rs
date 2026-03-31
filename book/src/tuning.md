# Tuning the Detector

This chapter answers the question: *"My detection fails or gives poor results — what do I change?"*

## Start here: use the built-in defaults

Before tuning anything, confirm you are starting from the library defaults:

```rust,no_run
use calib_targets::detect::{default_chess_config, detect_chessboard, ChessConfig};
use calib_targets::ChessboardParams;

let chess_cfg: ChessConfig = default_chess_config(); // ChESS corner detector config
let params    = ChessboardParams::default();  // chessboard assembly params
```

For ChArUco:

```rust,no_run
use calib_targets::charuco::CharucoDetectorParams;

let params = CharucoDetectorParams::for_board(&board);
```

`default_chess_config()` returns the workspace-owned ChESS config used by the
facade helpers. Its default tuning is single-scale detection with
`threshold_mode = Relative`, `threshold_value = 0.2`, and `nms_radius = 2`.

For ChArUco, board sampling scale is controlled separately by
`CharucoDetectorParams::for_board`, which starts with `px_per_square = 60`.
If marker decoding is the problem and the board appears at a very different
pixel scale, adjust `px_per_square` there before touching other parameters.

---

## Symptom → parameter table

| Symptom | Parameter to adjust |
|---|---|
| `ChessboardNotDetected` | `min_corners` ↓, `min_corner_strength` ↓ |
| Grid too small / partial board | `completeness_threshold` ↓ |
| Detects wrong connected component | `expected_rows` / `expected_cols` → set explicitly |
| Fast perspective / wide-angle lens | `max_spacing_pix` ↑, `orientation_tolerance_deg` ↑ |
| Dense board, corners falsely merged | `min_spacing_pix` ↑ |
| `NoMarkers` on blurry image | `min_border_score` ↓, `multi_threshold: true` |
| `AlignmentFailed` (low inlier count) | `min_marker_inliers` ↓ |

---

## Per-parameter reference: `ChessboardParams`

### `min_corner_strength`

**Default:** `0.0` (accept everything from the ChESS detector).

**When to raise:** On real-world images with textures that produce many spurious
saddle points, raise to `0.3`–`0.5` to drop weak corners before graph construction.
Raising too far discards valid but low-contrast corners near board edges.

### `min_corners`

**Default:** `16`.

**Guidance:** Set to roughly 70 % of the expected inner-corner count for your board
(e.g. 7 × 9 inner corners → `min_corners = 44`). Lowering allows partial detections;
raising avoids spurious small detections.

### `expected_rows` / `expected_cols`

**Default:** `None` (auto-detect from the largest connected component).

**When to set:** If the scene contains multiple chessboard-like objects and the wrong
one is returned, set these to the inner corner count of the board you care about. The
detector will only accept a component that matches these dimensions.

### `completeness_threshold`

**Default:** `0.7`.

**Guidance:** The fraction of expected corners that must be found for the detection to
be accepted. Lower to `0.3`–`0.5` when the board is partially occluded or at the image
border. Lower to `0.05` when exploring a very large, partially-visible board.

### `use_orientation_clustering`

**Default:** `true`.

**When to disable:** Only on synthetic or perfectly axis-aligned images where all
corners lie on a regular grid. On real images, orientation clustering is critical for
separating the two edge directions and should remain on.

---

## Per-parameter reference: `GridGraphParams`

### `min_spacing_pix`

**Default:** `5.0` pixels.

**Guidance:** Minimum distance between two corners for them to be considered separate
nodes. Raise (e.g. to `10`–`20`) when corners are dense and two nearby ChESS responses
correspond to a single physical corner, causing false links.

### `max_spacing_pix`

**Default:** `50.0` pixels.

**Guidance:** Maximum edge length in the proximity graph. For high-resolution images or
large printed boards, raise to roughly `image_width / expected_cols / 2`. If too small,
the graph is disconnected and large grids are not assembled.

### `k_neighbors`

**Default:** `8`.

**Guidance:** Number of nearest neighbors considered per corner during graph
construction. Rarely needs tuning. Lower values (e.g. `4`) can speed up graph
construction on very large corner sets at the cost of slightly lower robustness to
uneven corner spacing.

### `orientation_tolerance_deg`

**Default:** `22.5` degrees.

**Guidance:** Tolerance for the angular difference between an edge direction and the
dominant grid orientation. Tighten to `10`–`15°` in structured indoor scenes with many
false corners (e.g. tile patterns). Relax to `30°` or more for extreme perspective or a
handheld camera at a steep angle.

---

## Per-parameter reference: `ScanDecodeConfig` / ChArUco

These parameters live inside `CharucoDetectorParams`.

### `min_border_score`

**Default:** `0.75` for ChArUco.

**Guidance:** Minimum contrast score for the black border ring around a marker. Lower
cautiously to `0.65` for very blurry images. Values below `0.60` risk accepting
non-marker regions.

### `multi_threshold`

**Default:** `true`.

**Guidance:** When enabled, the decoder tries several Otsu-style binarization thresholds
until a dictionary match is found. This handles uneven lighting and motion blur at the
cost of a small speed penalty. Disable only when speed is critical and lighting is
controlled.

### `inset_frac`

**Default:** `0.06` for ChArUco.

**Guidance:** Fraction of the cell size inset from the cell boundary before sampling
the marker interior. Raise to `0.10`–`0.12` when the cell boundary visibly bleeds into
the bit area (common with thick printed borders or strong blur).

### `marker_size_rel`

**Source:** Board specification — must match the printed board exactly.

**Guidance:** Ratio of the ArUco marker side to the chessboard square side. A mismatch
here causes systematic decoding failures even when all other parameters are correct.
Verify against the printed board or the JSON spec used to generate it.

---

## Quick checklist

1. Start with defaults; run with `RUST_LOG=debug` to see corner counts and alignment scores.
2. If **no corners** are found: loosen `min_corner_strength`, check image resolution.
3. If **corners found but no grid**: check `max_spacing_pix` vs. actual square size.
4. If **grid found but no markers**: enable `multi_threshold`, lower `min_border_score`.
5. If **alignment fails**: verify board spec (rows, cols, dictionary, `marker_size_rel`).

See also: [Troubleshooting](troubleshooting.md) for per-error checklists.
