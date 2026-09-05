# calib-targets-marker

`calib-targets-marker` targets a checkerboard marker board: a chessboard grid with three circular markers near the center. The detector is grid-first and works with partial boards.

![Marker-board detection overlay](img/marker_detect_report_crop_overlay.png)
*Detected circle markers and aligned grid overlay.*

> For the end-to-end stage map, failure modes, and tuning, see the
> [Marker board pipeline](pipeline_marker.md). This page is the crate API
> reference.

## Detection pipeline

1. **Chessboard detection**: run `calib-targets-chessboard` to obtain grid-labeled corners (partial boards are allowed).
2. **Per-cell circle scoring**: for every valid square cell, warp the cell to a canonical patch and score a circle by comparing a disk sample to an annular ring.
3. **Candidate filtering**: keep the strongest circle candidates per polarity.
4. **Circle matching**: match candidates to the expected layout (cell coordinates + polarity).
5. **Grid alignment estimation**: derive a dihedral transform + translation from detected grid coordinates to board coordinates when enough circles agree.

## Key types

- `MarkerBoardDetector`: main entry point.
- `MarkerBoardSpec`: rows/cols plus the three expected circles (cell coordinate + polarity).
- `MarkerBoardParams`: board layout + chessboard params + circle score + match settings.
- `MarkerBoardDetection`:
  - `detection`: `TargetDetection` labeled as `CheckerboardMarker`.
  - `alignment`: optional `GridAlignment` semantic alias for the canonical affine `GridTransform`, from detected grid coordinates to board coordinates.
- `MarkerBoardDiagnostics` (opt-in, from the `diagnose` / `diagnose_with_corners` entry points):
  - `circle_candidates`: scored circles per cell.
  - `circle_matches`: matched circles (with offsets).
  - `inliers`: per-corner provenance back into the input ChESS-corner slice.
  - `alignment_inliers`: circles consistent with the best board frame.
  - `alignment_runner_up_inliers`: circles consistent with the best *competing* frame.
  - `alignment_ambiguous`: a second frame explained the circles just as well, so no alignment was returned.

## Parameters

`MarkerBoardSpec` defines the board and marker placement:

- `rows`, `cols`: inner corner counts.
- `cell_size`: optional square size in your world units (when set, `target_position` is populated).
- `circles`: three `MarkerCircleSpec` entries with `cell` (top-left corner indices) and `polarity`.
- `circle_diameter_rel`: printed disc diameter as a fraction of the square side (default `0.5`). Every radius the circle scorer probes is relative to this, so it is the one place the disc size is stated — it matches `circle_diameter_rel` on the printable spec.

`MarkerBoardParams` configures detection:

- `board`: the `MarkerBoardSpec` to detect.
- `chessboard`: `ChessboardParams` for the underlying corner-grid step. The
  chessboard detector is scale-invariant, so the v1 `expected_rows/cols`
  and `completeness_threshold` knobs no longer apply — the marker circles
  supply the geometry constraint.
- `circle_score`: per-cell circle scoring parameters.
- `match_params`: candidate filtering and matching thresholds.
- `roi_cells`: optional cell ROI `[i0, j0, i1, j1]`.

`CircleScoreParams` controls scoring:

- `patch_size`: canonical square size in pixels.
- `ring_thickness_frac`: ring thickness relative to circle radius.
- `ring_radius_mul`: ring radius relative to circle radius.
- `min_contrast`: minimum accepted disk-vs-ring contrast.
- `samples`: samples per ring for averaging.
- `center_search_px`: small pixel search around the cell center.

`CircleMatchParams` controls matching:

- `max_candidates_per_polarity`: top-N candidates to keep per polarity.
- `min_offset_inliers`: circles that must agree on one board frame before it is returned. Defaults to `3` — the whole layout.

## Notes

- Cell coordinates `(i, j)` refer to **square cells**, expressed by the top-left corner indices. The cell center is at `(i + 0.5, j + 0.5)`.
- `alignment` maps detected grid coordinates into board coordinates using a rotation and a translation, and it is the frame the returned corner labels are already expressed in.
- A circle's polarity is fixed by the square underneath it: `white` on an even `i + j`, `black` on an odd one. Any other combination draws a disc the same colour as its square, and `calib-targets-print` rejects it.
- The frame search covers only the four **rotations**, not the eight dihedral transforms. A camera imaging the printed side of an opaque board cannot produce a reflection, so including them would only let physically unreachable hypotheses compete with the true one.
- Resolving the frame is all-or-nothing by design. A single circle is consistent with all four rotations, and two leave no redundancy, so anything short of the full layout agreeing — or a second frame agreeing equally well — comes back as a typed error rather than a guess.
