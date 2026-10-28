# Marker board detection pipeline — atomic stages

Concise stage-by-stage map of `calib-targets-marker`'s detector. The
target is a chessboard with three reference circles in known cells;
the circles anchor the otherwise-unlabelled chessboard grid to a
known frame. Entry: `MarkerBoardDetector::detect` (`detector.rs`).

## Stage table

| # | Name | In | Out | Decision | Failure modes | Knobs |
|---|---|---|---|---|---|---|
| 0 | chessboard grid detect | `&[ChessCorner]` | `ChessboardDetection` (single best, no multi-component) | `ChessboardDetector::detect` | no chessboard found | every `chessboard.*` knob (`ChessboardParams`; full pipeline in `crates/calib-targets-chessboard/docs/PIPELINE.md`) |
| 1a | empty-square reference | corner map + image | per square colour: what a disk-free square reads; the grid's black-to-white range | warp every complete 4-corner cell into a square-normal patch and read the disk-vs-ring level difference at its centre; split the cells by `(i + j)` parity (one parity per square colour). A colour's reference is its **median** reading, kept only when it points toward the colour itself (the colour's direction is its median ring level against the other colour's) — the only thing blur can do to an empty square; a typical reading toward the opposite colour is a printed `inner_square_rel` inset, left to the shape gate and not subtracted. The scale is the brightest minus the darkest disk or ring level read at any centre | either colour has fewer than `2·3 + 1` cells (the layout's disks could outvote the median) or the grid shows no level range → no candidates | — (built over the whole grid, whatever `roi_cells` says) |
| 1b | circle candidate detection | reference + cells | `Vec<CircleCandidate>` (per cell: position, raw `score`, `contrast`, polarity, squareness) | a candidate's **`contrast`** is how far its reading departs from its colour's reference, and the sign of that departure is its polarity. Accept when it reaches **half the black-to-white range** — the disk moves its cell's reading past the midpoint between a disk-free square and a fully printed disk — and `min_contrast`, *and* the **shape gate** is silent — a probe ladder from `0.50 r` to `2.02 r` whose 4-theta harmonic, minus the 2-theta one that an ellipse would explain, must stay under `0.20` of the contrast on every ring. A level test alone accepts any centred blob, which is how an `inner_square_rel` white inset came to score as a disk on every black square (issue #96); and under blur every empty square reads a level difference of its own colour, because the ring picks up the neighbouring squares (below) | marker circles absent / wrong polarity (white circle on white cell) / blurred below resolution / very low contrast / not actually round | `circle_score: CircleScoreParams`, `board.circle_diameter_rel` (all probe radii are relative to it), `roi_cells: Option<[i0, j0, i1, j1]>`, `match_params.max_candidates_per_polarity` (default `6`, ranked by `contrast`) |
| 2–3 | board-frame resolution | candidates + board spec | `GridAlignment` (rotation + translation in `(i, j)`-space) + `Vec<CircleMatch>` + inlier / runner-up counts | hypothesis-and-verify, not matching: every `(rotation ∈ C4, translation)` pinned by one expected-circle-to-candidate seed is enumerated, deduped, and scored by how many expected circles it explains exactly (integer cell coincidence, polarity enforced). Accept only when the best frame explains `≥ min_offset_inliers` circles **and** strictly beats every other frame | fewer than `min_offset_inliers` circles found (`AlignmentFailed`); a second frame explains them equally well (`AlignmentAmbiguous`) | `match_params.min_offset_inliers` (default `3`, the whole layout), `match_params.max_candidates_per_polarity` |
| 4 | corner-frame shift | alignment | alignment in inner-corner coordinates | a circle layout names *squares*, so the resolved frame labels corners `1..=cols`; shift by `(-1, -1)` onto the inner-corner indexing the printable spec's `resolved_points`, the corner ids, and the non-negative-label invariant all use | — | — |
| 5 | emit detection | chessboard + circles + alignment | `MarkerBoardDetection { corners, alignment }` + `MarkerBoardDiagnostics { inliers, circle_candidates, circle_matches, alignment_inliers, alignment_runner_up_inliers, alignment_ambiguous }` | relabel every grid corner into the board frame and keep only the board's **inner corners**, `0..cols` × `0..rows` — the layout fixes the extent, so a corner outside it is not one of this board's corners (in practice the T-junction where the outer squares meet the margin, which the chessboard stage can label one row past the board). Every delivered corner has an id, and a `target_position` when `cell_size` is set; circle evidence is returned through the diagnostics channel | — | — |

## Why a disk is judged against an empty square

The scorer's ring sits `1.6 r` out — `0.44` of the square side at the default
disk size — so on a small, blurred square it picks up the neighbouring
squares, and every *empty* square reads a disk-vs-ring difference of its own
colour's sign. On the ground-truth suite's 8 px squares at blur σ 1.5 px that
reading is ±42 levels on a 190-level step: an empty black square scores like a
black disk, an empty white one like a white disk, far above `min_contrast`.
Judged on the raw reading, a dozen evidence-free candidates reached the frame
search, which found a coincidental L among them (`inliers = 3`,
`runner_up = 2`) and labelled the board in a wrong frame.

Measured against the empty-square reference instead, over the suite's 80
marker-board conditions: empty squares stay at or under `0.05` of the
black-to-white range; disks the optics resolve (16 px squares) read
`0.58..0.97` of it; the 4 px disks at blur σ 1.5 read `0.26..0.31` and are
declined. The half-range bar is not fitted to either side: it is the midpoint
between a disk-free square and a fully printed disk, the point at which the
cell reads more like a disk than like its square.

The reference keeps only the part of a colour's typical reading that points
toward the colour itself, because that is all blur can do to an empty square.
A board printed with `inner_square_rel` insets has a typical black square that
reads like a *white* disk; subtracting that would erase the real disks, which
differ from the insets in shape, not level — the shape gate's job.

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

`MarkerBoardDetection { corners, alignment:
Option<GridAlignment> }` carries the facts a consumer needs to use a
detection. The circle evidence — every scored `CircleCandidate`, the
per-expected-circle `CircleMatch` list, the per-corner `inliers`
provenance, and the frame-sweep counts — is returned through
`MarkerBoardDiagnostics` by the detector's `diagnose` /
`diagnose_with_corners` entry points, behind the `diagnostics` feature.

`CircleMatch.offset_cells` records the `(di, dj)` of each detected
circle relative to the expected board position — useful for spotting
misaligned alignments.

## Code map

| Stage | Function |
|---|---|
| 0 | `calib_targets_chessboard::ChessboardDetector::detect`, then `detector.rs::build_corner_map` |
| 1a | `detect.rs::detect_circles_via_square_warp` → `circle_score.rs::DiskProbe::levels_at_centre`, `detect.rs::empty_square_references` |
| 1b | `circle_score.rs::DiskProbe::score` |
| 2–3 | `match_circles.rs::resolve_board_frame` |
| 4 | `detector.rs::corner_frame` |
| 5 | `detector.rs::MarkerBoardDetector::apply_board_frame` |

## Cross-references

- `crates/calib-targets-chessboard/docs/PIPELINE.md` — upstream stages.
- `.claude/CLAUDE.md` "Marker decoding" — grid-aware sampling convention.
