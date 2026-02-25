# Calibration Target Detector — Agent Memory

## Project Overview

Rust workspace at `/Users/vitalyvorobyev/vision/calib-targets-rs`.
- `crates/calib-targets-charuco/` — ChArUco detection pipeline (primary crate)
- `crates/calib-targets-aruco/` — ArUco marker decoding
- `crates/calib-targets-chessboard/` — Chessboard detection (GridGraph BFS)
- `crates/calib-targets-core/` — Core types, homography, etc.
- `chess-corners-core` (external) — ChESS corner detector primitives

## ChArUco Detection Pipeline Stages

```
ChESS → GridGraph → ChessboardDetector → MarkerCells → scan_decode_markers_in_cells
  → select_alignment → map_charuco_corners → validate_and_fix_corners → CharucoDetectionResult
```

See `crates/calib-targets-charuco/src/detector/pipeline.rs`.

## Key Coordinate Systems

- **Grid frame (G)**: ChESS corner BFS-assigned integer coords, 1-based inner corners
- **Board frame (B)**: Board cell/corner indices, 0-based cells, 1-based inner corners
- **`alignment.map(i,j)`**: Maps grid coords → board coords (applies D4 rotation + translation)
- **`marker.gc`**: Grid-frame cell position, OFFSET from cell TL (`gc0`) by `marker.rotation`:
  - rot 0 → gc = gc0; rot 1 → gc = gc0+(1,0); rot 2 → gc = gc0+(1,1); rot 3 → gc = gc0+(0,1)
- **`corners_img[k]`** in `MarkerDetection`: always indexed by `gc0`, NOT by `marker.gc`
  - index 0→(gc0_x,gc0_y), 1→(gc0_x+1,gc0_y), 2→(gc0_x+1,gc0_y+1), 3→(gc0_x,gc0_y+1)
- **`charuco_object_xy(id)`**: Board-world position = `(col+1)*cell_size, (row+1)*cell_size`

## Corner Validation Module

`crates/calib-targets-charuco/src/detector/corner_validation.rs`

**Algorithm**: Estimates a board-to-image homography H from ALL inlier marker corners,
then for each ChArUco corner: if `|detected - H.apply(target_pos)| > threshold_rel * px_per_square`,
the corner is a false positive. Re-detect using ChESS in a small ROI around H.apply(target_pos).

**Why global H instead of per-marker predictions**: Dense self-contamination — when a false corner
is used in ALL adjacent marker cells (≤2 cells per corner in many cases), every per-marker
prediction equals the false position. The self-contamination filter removes them all, keeping the
false corner. The global H from 64-400+ inlier correspondences is robust to 2 outlier points.

**`recover_gc0(marker)`**: Inverts rotation offset to get cell TL from `marker.gc`:
```rust
match marker.rotation { 1 => (gc.gx-1, gc.gy), 2 => (gc.gx-1, gc.gy-1), 3 => (gc.gx, gc.gy-1), _ => (gc.gx, gc.gy) }
```

**`collect_board_to_image_correspondences`**: Uses `charuco_object_xy(id)` for board coords
(same as `corner.target_position`) to ensure consistent coordinate systems for H fitting.

## Key API Facts

- `chess-corners-core` (NOT `chess-corners`) must be a production dependency for `[dependencies]`
- `ChessParams` has no serde → use `#[serde(skip)]` on fields containing it
- `chess_response_u8_patch` returns ResponseMap in PATCH-LOCAL coords; shift by (x0,y0) to global
- `ImageView::with_origin([x0,y0])` allows refiner to read global pixels from local response coords
- `detect_corners_from_response_with_refiner` returns `Corner.xy` in response-map-local coords
- `estimate_homography_rect_to_img` available from `calib_targets_core` (full DLT, Hartley normalized)
- `board.charuco_corner_id_from_board_corner(i,j)` returns None for board-border corners (i≤0 etc)

## Known Failure Modes Fixed

- **Dense self-contamination**: False corner at ID X used in all adjacent marker cells → all
  per-marker predictions equal false position → self-contamination filter removes all → corner kept.
  FIXED by using global homography instead of per-marker predictions.
- **Rotation-sensitive corner indexing**: `corners_img` always indexed by `gc0`, not `marker.gc`.
  Under non-identity D4 alignment, `marker.gc != gc0`. Must recover `gc0` using `marker.rotation`.

## Test Structure

- `crates/calib-targets-charuco/tests/regression.rs`: 3 tests
  - `detects_charuco_on_large_png` (22×22, DICT_4X4_1000, ≥200 corners, IDs 369/309/109 not outliers)
  - `detects_charuco_on_small_png` (22×22 partial, DICT_4X4_250, ≥60 corners)
  - `detects_plain_chessboard_on_mid_png` (7×11 inner corners, 100% detection)
