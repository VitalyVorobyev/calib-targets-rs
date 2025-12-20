# calib-targets-charuco

`calib-targets-charuco` combines chessboard detection with ArUco decoding to detect ChArUco boards. The flow is grid-first:

1. Detect a chessboard grid from ChESS corners.
2. Rectify the grid (mesh warp).
3. Decode markers on the rectified grid.
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
  - `detection`: labeled corners with ChArUco IDs.
  - `markers`: decoded marker detections.
  - `alignment`: grid transform and inlier indices.
  - `rectified`: mesh-warped board view.

## Status

The current implementation focuses on the OpenCV-style layout and is intentionally conservative about alignment. Extensions for more layouts and improved robustness are planned (see the roadmap).
