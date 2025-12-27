# calib-targets-charuco

`calib-targets-charuco` combines chessboard detection with ArUco decoding to detect ChArUco boards. The flow is grid-first:

1. Detect a chessboard grid from ChESS corners.
2. Build per-cell quads from the detected grid.
3. Decode markers per cell (no full-image warp).
4. Align marker detections to a board specification and assign corner IDs.

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
- `CharucoDetector::detect` returns a `CharucoDetectionResult` with:
  - `detection`: labeled corners with ChArUco IDs, filtered to marker-supported corners.
  - `markers`: decoded marker detections in rectified grid coordinates (with optional `corners_img`).
  - `alignment`: grid alignment from detected grid coordinates into board coordinates.

## Per-cell decoding

The detector decodes markers **per grid cell**. This avoids building a full rectified image and keeps the work proportional to the number of valid squares. If you need a full rectified image for visualization, use the rectification helpers in `calib-targets-chessboard` on a detected grid.

## Alignment and refinement

Alignment maps decoded marker IDs to board positions using a small set of grid transforms and a translation vote. Once an alignment is found, the detector re-decodes markers at their **expected** cell locations and re-solves the alignment to filter out inconsistencies.

This two-stage approach helps reject spurious markers while keeping the final corner IDs consistent.

## Tuning notes

- `scan.inset_frac` trades off robustness vs. sensitivity. The defaults in `for_board` use a slightly smaller inset (`0.06`) to improve real-image decoding.
- `min_marker_inliers` controls how many aligned markers are required to accept a detection.

## Status

The current implementation focuses on the OpenCV-style layout and is intentionally conservative about alignment. Extensions for more layouts and improved robustness are planned (see the roadmap).

For alignment details, see [ChArUco Alignment and Refinement](charuco_alignment.md).
