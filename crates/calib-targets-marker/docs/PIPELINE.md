# Marker board detection pipeline — atomic stages

Concise stage-by-stage map of `calib-targets-marker`'s detector. The
target is a chessboard with three reference circles in known cells;
the circles anchor the otherwise-unlabelled chessboard grid to a
known frame.

## Stage table

| # | Name | In | Out | Decision | Failure modes | Knobs |
|---|---|---|---|---|---|---|
| 0 | chessboard grid detect | `&[Corner]` (ChESS raw) | `ChessDetection` (single best, no multi-component) | `ChessDetector::detect` with caller-chosen `graph_build_algorithm` (typically default) | no chessboard found | every `chessboard.*` knob from `DetectorParams` (full pipeline of `crates/calib-targets-chessboard/docs/PIPELINE.md`) |
| 1 | circle candidate detection | corner map + image | `Vec<CircleCandidate>` (per cell: position, radius, contrast, polarity) | for each complete 4-corner cell, warp the cell into a square-normal image patch, sample the response at each pixel, find centroid + radius of bright/dark disk; keep the top `max_candidates_per_polarity` per polarity | marker circles absent / wrong polarity (white circle on white cell) / very low contrast | `circle_score: CircleScoreParams`, `roi_cells: Option<[i0, j0, i1, j1]>`, `match_params.max_candidates_per_polarity` (default `6`) |
| 2 | expected-circle matching | candidates + board spec | `Vec<CircleMatch>` (expected → candidate index, offset in cells) | for each of the 3 expected marker circles, find the nearest candidate within `max_distance_cells` (optional); match by polarity | candidates outside the distance threshold; wrong-polarity match | `match_params.max_distance_cells`, `match_params.max_candidates_per_polarity` |
| 3 | grid alignment estimation | matched circles + candidates | `GridAlignment` (rotation + translation in `(i, j)`-space) + inlier count | RANSAC-like: fit `estimate_grid_alignment` on the matched 3-circle layout; require `≥ min_offset_inliers` consistent matches (typically 1, with 3 circles it's a pose-from-3-points) | fewer than 3 matches; circles on board boundary → unreliable alignment | `match_params.min_offset_inliers` (default `1`) |
| 4 | per-corner offset mapping | matches + alignment | offset `(di, dj)` per circle | apply `alignment.transform` to each candidate cell coord; compute delta from expected | — | — |
| 5 | emit detection | chessboard + circles + alignment | `MarkerBoardDetectionResult { corners, alignment }` + `MarkerBoardDiagnostics { inliers, circle_candidates, circle_matches, alignment_inliers }` | emit typed marker-board corners with optional IDs / target positions; circle evidence is returned through the diagnostics channel | — | — |

## What the marker board inherits from seed-and-grow

Stages 0-10 of seed-and-grow (BFS, validation, Stage 6 / 6.5 / 6.75
including mandatory geometry check). The 3-circle pattern serves only
to **anchor** the labelled grid to a known frame — wrong `(i, j)`
labels at the chessboard layer would mis-align every alignment-derived
ID.

This detector uses `detect` (single best component) rather than
`detect_all` — multi-component splits are not supported.

## Diagnose dump

`MarkerBoardDetectionResult { corners, alignment:
Option<GridAlignment> }` carries the facts a consumer needs to use a
detection. The circle evidence — every scored `CircleCandidate`, the
per-expected-circle `CircleMatch` list, the per-corner `inliers`
provenance, and the `alignment_inliers` count — is returned through
`MarkerBoardDiagnostics` by the detector's `*_with_diagnostics` entry
points (`detect_from_corners_with_diagnostics`,
`detect_from_image_and_corners_with_diagnostics`).

`CircleMatch.offset_cells` records the `(di, dj)` of each detected
circle relative to the expected board position — useful for spotting
misaligned alignments.

## Cross-references

- `crates/calib-targets-chessboard/docs/PIPELINE.md` — upstream stages.
- `CLAUDE.md` "Marker decoding" — grid-aware sampling convention.
