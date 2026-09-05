# Marker board detection pipeline — atomic stages

Concise stage-by-stage map of `calib-targets-marker`'s detector. The
target is a chessboard with three reference circles in known cells;
the circles anchor the otherwise-unlabelled chessboard grid to a
known frame.

## Stage table

| # | Name | In | Out | Decision | Failure modes | Knobs |
|---|---|---|---|---|---|---|
| 0 | chessboard grid detect | `&[Corner]` (ChESS raw) | `ChessDetection` (single best, no multi-component) | `ChessDetector::detect` on the **topological** grid builder (the only builder; `graph_build_algorithm` is a single-variant reserved seam) | no chessboard found | every `chessboard.*` knob from `DetectorParams` (full pipeline of `crates/calib-targets-chessboard/docs/PIPELINE.md`) |
| 1 | circle candidate detection | corner map + image | `Vec<CircleCandidate>` (per cell: position, contrast, polarity, squareness) | for each complete 4-corner cell, warp the cell into a square-normal patch; accept when the disk-vs-ring **level** difference clears `min_contrast` *and* the **shape gate** is silent — a probe ladder from `0.50 r` to `2.02 r` whose 4-theta harmonic, minus the 2-theta one that an ellipse would explain, must stay under `0.20` of the contrast on every ring. The level test alone accepts any centred blob, which is how an `inner_square_rel` white inset came to score as a disk on every black square (issue #96) | marker circles absent / wrong polarity (white circle on white cell) / very low contrast / not actually round | `circle_score: CircleScoreParams`, `board.circle_diameter_rel` (all probe radii are relative to it), `roi_cells: Option<[i0, j0, i1, j1]>`, `match_params.max_candidates_per_polarity` (default `6`) |
| 2–3 | board-frame resolution | candidates + board spec | `GridAlignment` (rotation + translation in `(i, j)`-space) + `Vec<CircleMatch>` + inlier / runner-up counts | hypothesis-and-verify, not matching: every `(rotation ∈ C4, translation)` pinned by one expected-circle-to-candidate seed is enumerated, deduped, and scored by how many expected circles it explains exactly (integer cell coincidence, polarity enforced). Accept only when the best frame explains `≥ min_offset_inliers` circles **and** strictly beats every other frame | fewer than `min_offset_inliers` circles found (`AlignmentFailed`); a second frame explains them equally well (`AlignmentAmbiguous`) | `match_params.min_offset_inliers` (default `3`, the whole layout), `match_params.max_candidates_per_polarity` |
| 4 | corner-frame shift | alignment | alignment in inner-corner coordinates | a circle layout names *squares*, so the resolved frame labels corners `1..=cols`; shift by `(-1, -1)` onto the inner-corner indexing the printable spec's `resolved_points`, the corner ids, and the non-negative-label invariant all use | — | — |
| 5 | emit detection | chessboard + circles + alignment | `MarkerBoardDetectionResult { corners, alignment }` + `MarkerBoardDiagnostics { inliers, circle_candidates, circle_matches, alignment_inliers, alignment_runner_up_inliers, alignment_ambiguous }` | emit typed marker-board corners with optional IDs / target positions; circle evidence is returned through the diagnostics channel | — | — |

## What the marker board inherits from the chessboard detector

The full chessboard topological pipeline (prefilter, axis clustering,
the topological grid walk, booster-driven component recovery, and the
mandatory final geometry check). The 3-circle pattern serves only to
**anchor** the labelled grid to a known frame — wrong `(i, j)` labels at
the chessboard layer would mis-align every alignment-derived ID.

Because that anchoring is the circles' *only* job, the frame it produces
is held to the same asymmetric contract as the labels: a miss is
acceptable, a wrong frame is not. A single circle is consistent with all
four rotations and two leave no redundancy to check a wrong pairing
against, so anything short of the full layout agreeing — or a second
frame agreeing just as well — is reported as a typed failure rather than
resolved.

This detector uses `detect` (single best component) rather than
`detect_all` — multi-component splits are not supported.

## Diagnose dump

`MarkerBoardDetectionResult { corners, alignment:
Option<GridAlignment> }` carries the facts a consumer needs to use a
detection. The circle evidence — every scored `CircleCandidate`, the
per-expected-circle `CircleMatch` list, the per-corner `inliers`
provenance, and the frame-sweep counts — is returned through
`MarkerBoardDiagnostics` by the detector's `diagnose` /
`diagnose_with_corners` entry points, behind the `diagnostics` feature.

`CircleMatch.offset_cells` records the `(di, dj)` of each detected
circle relative to the expected board position — useful for spotting
misaligned alignments.

## Cross-references

- `crates/calib-targets-chessboard/docs/PIPELINE.md` — upstream stages.
- `CLAUDE.md` "Marker decoding" — grid-aware sampling convention.
