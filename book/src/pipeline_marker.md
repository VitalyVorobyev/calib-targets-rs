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
| 1 | circle candidate detection | corner map + image → `Vec<CircleCandidate>` | For each complete 4-corner cell, warp the cell to a square patch and accept when the disk-vs-ring **level** difference clears `min_contrast` *and* the **shape gate** is silent: the intensity on a ladder of probe rings must carry no 4-fold angular modulation beyond what an ellipse would explain. The level test alone accepts any centred blob of roughly the right size. |
| 2–3 | board-frame resolution | candidates + spec → `GridTransform` + `Vec<CircleMatch>` | Hypothesis-and-verify, not matching. Every `(rotation, translation)` pinned by one expected-circle-to-candidate seed is enumerated and scored by how many expected circles it explains exactly. Accepted only when the best frame explains `≥ min_offset_inliers` circles **and** strictly beats every other frame. Only the four rotations are searched — a reflection is not something a camera can produce from the printed side of an opaque board. |
| 4 | corner-frame shift | alignment → alignment | A circle layout names *squares*, so the resolved frame labels corners `1..=cols`; one shift puts them on the inner-corner indexing the printable spec and the corner ids use. |
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
| No grid / `Err(ChessboardNotDetected)` | Stage 0 (chessboard) | Sparse corner cloud or clustering failure — see the [chessboard failure modes](pipeline_chessboard.md#failure-modes). |
| No / too few circle candidates | Stage 1 | Circles absent, wrong polarity (e.g. white circle on white cell), low contrast, or not actually round — the `squareness` reading on each candidate says which. Adjust `circle_score.min_contrast`, check `board.circle_diameter_rel` matches what was printed, and check `roi_cells` is not excluding them. |
| `Err(AlignmentFailed)` (too few circles agree) | Stage 2–3 | Fewer than `min_offset_inliers` of the layout's circles were found, or polarity mismatches the spec. Verify the three `MarkerCircleSpec` cells + polarities against the printed board. |
| `Err(AlignmentAmbiguous)` | Stage 2–3 | Two board frames explain the circles equally well. Either the layout is rotationally symmetric — pick cells whose rotations differ — or the scorer produced spurious same-polarity candidates. The `circle_candidates` dump tells the two apart. |
| Grid found but `target_position` empty | output | `board.cell_size` is unset (or alignment failed) — `target_position` is only populated when both hold. |
| Wrong anchored IDs | **never** | A wrong chessboard `(i, j)` would cause this — file a bug at the chessboard layer. |

## Tuning

`MarkerBoardParams` is board layout + chessboard params + circle scoring +
matching:

- **`board`** — the `MarkerBoardSpec` (rows, cols, the three
  `MarkerCircleSpec` cells + polarities, `circle_diameter_rel`, optional
  `cell_size`). The marker circles supply the geometry constraint, so the
  v1 `expected_rows/cols` and `completeness_threshold` no longer apply.
  `circle_diameter_rel` is the one place the printed disc size is stated —
  every radius the scorer probes is relative to it.
- **`chessboard`** — a `ChessboardParams` for the underlying grid step.
- **`circle_score`** (`CircleScoreParams`) — `patch_size`,
  `ring_thickness_frac`, `ring_radius_mul`, `min_contrast`, `samples`,
  `center_search_px`.
- **`match_params`** (`CircleMatchParams`) — `max_candidates_per_polarity`
  (default `6`), `min_offset_inliers` (default `3`, the whole layout).
  Lowering `min_offset_inliers` buys recall on a board with an occluded
  marker at the cost of the guarantee that the frame is never wrong.
- **`roi_cells`** — optional `[i0, j0, i1, j1]` to restrict the circle
  search.

Cell coordinates `(i, j)` in the spec refer to **square cells** by their
top-left corner index; the cell center is at `(i + 0.5, j + 0.5)`. Use the
`diagnose` / `diagnose_with_corners` entry points to inspect scored
candidates, matches, and the frame-sweep counts when tuning.

## Cross-references

- [calib-targets-marker](marker.md) — the crate API and key types.
- [Chessboard pipeline](pipeline_chessboard.md) — the grid spine this
  detector anchors.
- `crates/calib-targets-marker/docs/PIPELINE.md` — the canonical stage map
  this page mirrors.
