# ChArUco Alignment

`calib-targets-charuco` aligns decoded marker IDs to a board layout and assigns ChArUco corner IDs. Alignment is discrete and fast: it tries a small set of grid transforms and selects the translation with the strongest inlier support.

## Alignment pass

- Each decoded marker votes for a board translation under each candidate transform.
- The best translation wins (ties broken by inlier count).
- Inliers are the markers whose `(sx, sy)` map exactly to the expected board cell for their ID.

## Inlier filtering

After alignment is chosen, the detector keeps only inlier markers and assigns ChArUco corner IDs based on the aligned grid coordinates. The final `alignment` uses the semantic `GridAlignment` alias for the canonical affine `GridTransform`; it maps detected grid **corner** coordinates into board corner coordinates with one matrix plus translation. Markers are matched by cell (a cell is named by its top-left corner), so the matched cell placement is converted to this corner alignment with `calib_targets_core::cell_to_corner_alignment`; under a rotation the two differ in translation (see [ChArUco alignment & corner IDs](algo_charuco_alignment.md#cell-alignment-and-corner-alignment)).
