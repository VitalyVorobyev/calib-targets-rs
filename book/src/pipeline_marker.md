# Marker board pipeline

> Composes: the [chessboard grid stack](pipeline_chessboard.md) +
> 3-circle anchoring.
> **Source of truth:** `crates/calib-targets-marker/docs/PIPELINE.md`.
> Crate reference: [calib-targets-marker](marker.md).

A marker board is a chessboard with three reference circles in known
cells. The pipeline runs the [chessboard grid detector](pipeline_chessboard.md)
to recover the lattice, then detects the three circles and uses them to
**anchor** the otherwise-unlabelled grid to a known board frame. It is the
lightest self-identifying target: where ChArUco anchors with per-cell
marker IDs, this anchors with a single 3-point pose.

## End-to-end stages

| # | Stage | In → Out | What it does |
|---|---|---|---|
| 0 | chessboard grid detect | ChESS corners → `ChessDetection` | `ChessDetector::detect` — **single best component** (multi-component is not supported here). |
| 1 | circle candidate detection | corner map + image → `Vec<CircleCandidate>` | For each complete 4-corner cell, warp the cell to a square patch, find the centroid + radius of a bright/dark disk, and keep the top `max_candidates_per_polarity` per polarity. |
| 2 | expected-circle matching | candidates + spec → `Vec<CircleMatch>` | For each of the 3 expected circles, find the nearest candidate within `max_distance_cells` (optional), matching by polarity. |
| 3 | grid alignment estimation | matches → `GridAlignment` + inliers | Fit a dihedral transform + translation in `(i, j)`-space from the matched 3-circle layout; require `≥ min_offset_inliers` consistent matches. |
| 4 | per-corner offset mapping | matches + alignment → offsets | Apply the alignment transform to each candidate cell coord; compute the delta from expected. |
| 5 | emit detection | grid + circles + alignment → result | Emit typed marker-board corners (optional IDs / `target_position`); circle evidence is returned through `MarkerBoardDiagnostics`. |

## What it inherits from the chessboard detector

The full chessboard topological pipeline (prefilter,
[clustering](algo_axis_clustering.md), the [grid walk](algo_topological_grid.md),
[booster recovery, and the mandatory geometry
check](algo_recovery_validation.md)). The 3-circle pattern serves only to
**anchor** the labelled grid to a known frame — a wrong `(i, j)` label at
the chessboard layer would mis-align every alignment-derived ID. This
detector uses `detect` (single best component), not `detect_all`.

## Failure modes

| Symptom | Likely stage | What it means / knob to try |
|---|---|---|
| No grid / `None` from Stage 0 | Stage 0 (chessboard) | Sparse corner cloud or clustering failure — see the [chessboard failure modes](pipeline_chessboard.md#failure-modes). |
| No / too few circle candidates | Stage 1 | Circles absent, wrong polarity (e.g. white circle on white cell), or low contrast. Adjust `circle_score` (`min_contrast`, `diameter_frac`); check `roi_cells` is not excluding them. |
| Candidates found, no matches | Stage 2 | Candidates outside `max_distance_cells`, or polarity mismatch vs the spec. Verify the three `MarkerCircleSpec` cells + polarities against the printed board. |
| Alignment `None` (too few inliers) | Stage 3 | Fewer than the required consistent matches, or circles on the board boundary giving an unreliable pose. Lower `min_offset_inliers` only if you genuinely see fewer circles. |
| Grid found but `target_position` empty | output | `layout.cell_size` is unset (or alignment failed) — `target_position` is only populated when both hold. |
| Wrong anchored IDs | **never** | A wrong chessboard `(i, j)` would cause this — file a bug at the chessboard layer. |

## Tuning

`MarkerBoardParams` is layout + chessboard params + circle scoring +
matching:

- **`layout`** — the `MarkerBoardSpec` (rows, cols, the three
  `MarkerCircleSpec` cells + polarities, optional `cell_size`). The marker
  circles supply the geometry constraint, so the v1 `expected_rows/cols`
  and `completeness_threshold` no longer apply.
- **`chessboard`** — a `DetectorParams` for the underlying grid step.
- **`circle_score`** (`CircleScoreParams`) — `patch_size`,
  `diameter_frac`, `ring_thickness_frac`, `ring_radius_mul`,
  `min_contrast`, `samples`, `center_search_px`.
- **`match_params`** (`CircleMatchParams`) — `max_candidates_per_polarity`
  (default `6`), `max_distance_cells` (optional), `min_offset_inliers`
  (default `1`).
- **`roi_cells`** — optional `[i0, j0, i1, j1]` to restrict the circle
  search.

Cell coordinates `(i, j)` in the spec refer to **square cells** by their
top-left corner index; the cell center is at `(i + 0.5, j + 0.5)`. Use the
`*_with_diagnostics` entry points to inspect scored candidates, matches,
and `alignment_inliers` when tuning.

## Cross-references

- [calib-targets-marker](marker.md) — the crate API and key types.
- [Chessboard pipeline](pipeline_chessboard.md) — the grid spine this
  detector anchors.
- `crates/calib-targets-marker/docs/PIPELINE.md` — the canonical stage map
  this page mirrors.
