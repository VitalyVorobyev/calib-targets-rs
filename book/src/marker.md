# calib-targets-marker

`calib-targets-marker` targets a checkerboard marker board: a chessboard grid with three circular markers near the center. The detector is grid-first and works with partial boards.

![Marker-board detection overlay](../img/marker_detect_report_crop_overlay.png)
*Detected circle markers and aligned grid overlay.*

## Detection pipeline

1. **Chessboard detection**: run `calib-targets-chessboard` to obtain grid-labeled corners (partial boards are allowed).
2. **Per-cell circle scoring**: for every valid square cell, warp the cell to a canonical patch and score a circle by comparing a disk sample to an annular ring.
3. **Candidate filtering**: keep the strongest circle candidates per polarity.
4. **Circle matching**: match candidates to the expected layout (cell coordinates + polarity).
5. **Grid alignment estimation**: derive a dihedral transform + translation from detected grid coordinates to board coordinates when enough circles agree.

## Key types

- `MarkerBoardDetector`: main entry point.
- `MarkerBoardLayout`: rows/cols plus the three expected circles (cell coordinate + polarity).
- `MarkerBoardParams`: layout + chessboard/grid graph params + circle score + match settings.
- `MarkerBoardDetectionResult`:
  - `detection`: `TargetDetection` labeled as `CheckerboardMarker`.
  - `circle_candidates`: scored circles per cell.
  - `circle_matches`: matched circles (with offsets).
  - `alignment`: optional `GridAlignment` from detected grid coords to board coords.
  - `alignment_inliers`: number of circle matches used for the alignment.

## Parameters

`MarkerBoardLayout` defines the board and marker placement:

- `rows`, `cols`: inner corner counts.
- `circles`: three `MarkerCircleSpec` entries with `cell` (top-left corner indices) and `polarity`.

`MarkerBoardParams` configures detection:

- `chessboard`: `ChessboardParams` (defaults to `completeness_threshold = 0.05` to allow partial boards).
- `grid_graph`: `GridGraphParams` for neighbor search constraints.
- `circle_score`: per-cell circle scoring parameters.
- `match_params`: candidate filtering and matching thresholds.
- `roi_cells`: optional cell ROI `[i0, j0, i1, j1]`.

`CircleScoreParams` controls scoring:

- `patch_size`: canonical square size in pixels.
- `diameter_frac`: circle diameter relative to the square.
- `ring_thickness_frac`: ring thickness relative to circle radius.
- `ring_radius_mul`: ring radius relative to circle radius.
- `min_contrast`: minimum accepted disk-vs-ring contrast.
- `samples`: samples per ring for averaging.
- `center_search_px`: small pixel search around the cell center.

`CircleMatchParams` controls matching:

- `max_candidates_per_polarity`: top-N candidates to keep per polarity.
- `max_distance_cells`: optional maximum distance for a match.
- `min_offset_inliers`: minimum agreeing circles to return an alignment.

## Notes

- Cell coordinates `(i, j)` refer to **square cells**, expressed by the top-left corner indices. The cell center is at `(i + 0.5, j + 0.5)`.
- `alignment` maps detected grid coordinates into board coordinates using a dihedral transform and translation.
