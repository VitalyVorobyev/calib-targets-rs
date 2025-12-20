# calib-targets-marker

`calib-targets-marker` targets a checkerboard marker board: a chessboard grid with three circular markers near the center. The intended workflow is:

1. Detect a chessboard grid using `calib-targets-chessboard`.
2. Detect three circles in image space.
3. Match circle centers to known grid coordinates.
4. Output a `TargetDetection` with `TargetKind::CheckerboardMarker`.

## Current status

The crate currently reuses the chessboard detector and relabels the result as a marker detection. Circle scoring and matching logic live in `circle_score.rs` and `detect.rs`, but the full detector pipeline is still under development.

The roadmap chapter outlines the expected work to complete this detector.
