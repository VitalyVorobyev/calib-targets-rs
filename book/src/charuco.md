# calib-targets-charuco

`calib-targets-charuco` combines chessboard detection with ArUco decoding to detect ChArUco boards. ChArUco dictionaries and board layouts are fully compatible with OpenCV's aruco/charuco implementation. The flow is grid-first:

![ChArUco detection overlay](img/charuco_detect_report_small2_overlay.png)
*ChArUco detection overlay with assigned corner IDs.*

1. Detect a chessboard grid from ChESS corners.
2. Build per-cell quads from the detected grid.
3. Decode markers per cell (no full-image warp).
4. Align marker detections to a board specification and assign corner IDs.

The detailed stage-by-stage description lives in [ChArUco Detection Pipeline](charuco_pipeline.md).

## Board specification

- `CharucoBoardSpec` describes the board geometry:
  - `rows`, `cols` are **square counts** (not inner corners).
  - `cell_size` is the physical square size.
  - `marker_size_rel` is the marker size relative to a square.
  - `dictionary` selects the marker dictionary.
  - `marker_layout` defines the placement scheme.
- `CharucoBoard` validates and precomputes marker placement.

## Detector

- `CharucoDetectorParams::for_board` provides a reasonable default configuration.
- The default configuration is intentionally local-first:
  - sparse per-cell marker decoding on
  - adaptive chessboard graph search on
  - multi-hypothesis marker decode off
  - rectified marker recovery off
  - global homography corner validation off
- `CharucoDetector::detect` returns a `CharucoDetectionResult` with:
  - `detection`: labeled corners with ChArUco IDs, assigned to already detected corners.
  - `markers`: decoded marker detections in rectified grid coordinates (with optional `corners_img`).
  - `alignment`: grid alignment from detected grid coordinates into board coordinates.

## Per-cell decoding

The detector decodes markers **per grid cell**. This avoids building a full rectified image and keeps the work proportional to the number of valid squares. If you need a full rectified image for visualization, use the rectification helpers in `calib-targets-chessboard` on a detected grid.

## Alignment

Alignment is discrete. The detector combines:

- marker-vote alignment over D4 grid transforms
- patch-first legal placement scoring for sparse-marker cases

Markers act as anchors for board placement; they are not the primary source of corner geometry.

## Tuning notes

- `scan.inset_frac` trades off robustness vs. sensitivity. The defaults in `for_board` use a slightly smaller inset (`0.06`) to improve real-image decoding.
- `min_marker_inliers` controls how many aligned markers are required to accept a detection.
- `augmentation.multi_hypothesis_decode` is an explicit opt-in robustness mode.
- `augmentation.rectified_recovery` is an explicit opt-in global recovery stage.
- `use_global_corner_validation` is an explicit opt-in legacy cleanup stage.

## Status

The current implementation focuses on the OpenCV-style layout and on a corner-first default path. Optional global stages remain available for diagnostics and experimentation, but they are not part of the intended default detector.

For alignment details, see [ChArUco Alignment and Refinement](charuco_alignment.md).
