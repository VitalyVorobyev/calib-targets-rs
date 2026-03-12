# ChArUco Alignment

`calib-targets-charuco` aligns decoded marker IDs to a board layout and assigns ChArUco corner IDs. Alignment is discrete and fast: it tries a small set of grid transforms and selects the translation with the strongest inlier support.

## Marker-vote alignment

- Each decoded marker votes for a board translation under each candidate transform.
- The detector searches all D4 grid symmetries.
- The best translation wins, with patch-fit information used to break marker-count ties.
- Inliers are the markers whose `(sx, sy)` map exactly to the expected board cell for their ID.

## Patch-first placement

Sparse-marker cases are not handled by marker voting alone.

The detector also enumerates legal placements of the observed lattice patch on the target board and scores them by:

- matched expected markers
- contradictory confident marker decodes
- fraction of observed lattice corners that stay inside valid board bounds

This allows the detector to recover a unique board placement when the lattice patch is strong but the decoded marker set is small.

## Inlier filtering and policy

After alignment is chosen, the detector keeps only inlier markers and assigns ChArUco corner IDs based on the aligned grid coordinates. The final `alignment` in the result is a `GridAlignment` that maps detected grid coordinates into board coordinates.

`min_marker_inliers` remains the main acceptance threshold. A lower-inlier unique placement can be accepted only when `allow_low_inlier_unique_alignment` is explicitly enabled.
