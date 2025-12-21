# ChArUco Alignment and Refinement

`calib-targets-charuco` aligns decoded marker IDs to a board layout and assigns ChArUco corner IDs. Alignment is discrete and fast: it tries a small set of grid transforms and selects the translation with the strongest inlier support.

## Alignment pass

- Each decoded marker votes for a board translation under each candidate transform.
- The best translation wins (ties broken by inlier count).
- Inliers are the markers whose `(sx, sy)` map exactly to the expected board cell for their ID.

## Refinement pass

After an initial alignment, the detector re-decodes markers at their expected cell locations and re-solves the alignment. This filters out inconsistent detections and stabilizes the final corner IDs.

## Fallback to rectified scan

If per-cell alignment is weak, the detector can optionally scan a full rectified mesh image to recover additional markers. Enable this with `fallback_to_rectified` and request a rectified output via `build_rectified_image` when you need debug visuals.
