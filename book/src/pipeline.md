# Pipeline Overview

At a high level, the workflow looks like this:

1. **Input corners**: supply a list of `calib_targets_core::Corner` values, typically from a ChESS detector.
2. **Estimate grid axes**: cluster corner orientations to get two dominant grid directions.
3. **Build a grid graph**: connect corners that plausibly lie on the same grid lines.
4. **Assign integer coordinates**: BFS the graph to produce `(i, j)` grid indices.
5. **Select the best board**: choose the best connected component that matches expected size.
6. **Rectify (optional)**: compute a global homography or mesh warp to build a rectified view.
7. **Decode markers (optional)**: scan rectified squares for ArUco codes.
8. **Align board (optional)**: map markers to a known layout and assign corner IDs.

Output types are standardized in `calib-targets-core` as `TargetDetection` with `LabeledCorner` values. Higher-level crates enrich that output with additional metadata (inliers, marker detections, rectified views).
