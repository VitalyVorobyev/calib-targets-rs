# Calibration Target Detector — Agent Memory

## Project Overview

Rust workspace at `/Users/vitalyvorobyev/vision/calib-targets-rs`.
- `crates/calib-targets-charuco/` — ChArUco detection pipeline (primary crate)
- `crates/calib-targets-aruco/` — ArUco marker decoding
- `crates/calib-targets-chessboard/` — Chessboard detection (topological grid builder from `projective-grid`)
- `crates/calib-targets-core/` — Core types, homography, etc.
- `chess-corners` (external) — ChESS corner detector

## ChArUco Detection Pipeline Stages

See `crates/calib-targets-charuco/src/detector/pipeline.rs`.

## Key Coordinate Systems

- **Grid frame (G)**: integer `(i, j)` labels from the topological builder's per-component walk, rebased so the labelled bounding-box minimum is `(0, 0)` (non-negative; inner corners)
- **Board frame (B)**: Board cell/corner indices, 0-based cells, 1-based inner corners
- **`alignment.map(i,j)`**: Maps grid coords → board coords (applies D4 rotation + translation)
- **`marker.gc`**: Grid-frame cell position, OFFSET from cell TL (`gc0`) by `marker.rotation`:
  - rot 0 → gc = gc0; rot 1 → gc = gc0+(1,0); rot 2 → gc = gc0+(1,1); rot 3 → gc = gc0+(0,1)
- **`corners_img[k]`** in `MarkerDetection`: always indexed by `gc0`, NOT by `marker.gc`
  - index 0→(gc0_x,gc0_y), 1→(gc0_x+1,gc0_y), 2→(gc0_x+1,gc0_y+1), 3→(gc0_x,gc0_y+1)
- **`charuco_object_xy(id)`**: Board-world position = `(col+1)*cell_size, (row+1)*cell_size`

## Corner Validation Module

`crates/calib-targets-charuco/src/detector/corner_refit.rs` (`validate_and_fix_corners`)

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

- `estimate_homography_rect_to_img` available from `calib_targets_core` (full DLT, Hartley normalized)
- `board.charuco_corner_id_from_board_corner(i,j)` returns None for board-border corners (i≤0 etc)

## Known Failure Modes Fixed

- **Dense self-contamination**: False corner at ID X used in all adjacent marker cells → all
  per-marker predictions equal false position → self-contamination filter removes all → corner kept.
  FIXED by using global homography instead of per-marker predictions.
- **Rotation-sensitive corner indexing**: `corners_img` always indexed by `gc0`, not `marker.gc`.
  Under non-identity D4 alignment, `marker.gc != gc0`. Must recover `gc0` using `marker.rotation`.
- **Double-angle circular mean for undirected axes**: angles on [0, π) representing
  undirected lines can cancel when averaged naively (sin θ and sin(θ+π) both positive, cos θ
  and cos(θ+π) opposite → mean collapses to π/2). Always accumulate in DOUBLE-ANGLE space
  (cos 2θ, sin 2θ) and halve the atan2 result.

## Test Structure

- `crates/calib-targets-charuco/tests/regression.rs` — public-testdata ChArUco and plain-chessboard regressions

## Investigations

- [Gap 16 smooth-warp backstop — NOT SAFE](gap16_smooth_warp_backstop.md) — global biquadratic/affine
  warp residual cannot catch the Gap-15 false positive (LOO z=−0.96, 3rd-smallest of 53); falsified by
  measurement on GeminiChess1. Do not re-attempt as a global-residual gate.
