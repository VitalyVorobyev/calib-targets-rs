# ChArUco alignment & corner IDs

> Code: `calib-targets-charuco` (the marker matcher + alignment + corner
> mapping stages). Layout-compatible with OpenCV's aruco/charuco.

A ChArUco board carries an ArUco marker in every white square. Once the
markers are decoded ([ArUco bit decode](algo_aruco_decode.md)), alignment
solves for the board→image transform from the decoded marker IDs and then
assigns each chessboard inner corner its **absolute, OpenCV-compatible
corner ID**. This is what makes ChArUco robust to partial views: even a
handful of correctly-decoded markers anchor the whole detected grid to the
board's canonical frame.

## Inputs

- The labelled chessboard grid from the [topological grid
  finder](algo_topological_grid.md) (one or more components).
- Per-cell decoded markers, each with an ID, sampled bits and a border
  score.
- The board specification (`CharucoBoardSpec`): rows, cols, dictionary,
  marker layout.

## Alignment

The detector ships two matchers that solve the same problem — which
board → image rigid/grid transform is consistent with the observed marker
IDs:

- **Legacy (hard-decode) matcher.** Each decoded marker votes for a board
  translation under each candidate D4 transform; the translation with the
  strongest inlier support wins (ties broken by inlier count). Inliers are
  markers whose grid coordinate maps exactly to the expected board cell for
  their ID. Fast and discrete.
- **Board-level (soft-bit) matcher.** Instead of committing to a hard
  decode per cell, it scores each `(D4, translation)` hypothesis by summing
  per-bit log-likelihoods `log_sigmoid(κ × bit_confidence × ±1)` over all
  cells, picks the maximum-likelihood hypothesis, and applies a **margin
  gate** `(best − runner-up)/|best| ≥ alignment_min_margin`. The margin
  gate flags ambiguous decodes (heavy bit noise, near-symmetric layouts)
  rather than committing to a coin-flip. Opt-in via `use_board_level_matcher`.

Either way, the chosen alignment is accepted only if its inlier count
clears `min_marker_inliers` (or `min_secondary_marker_inliers` for
non-primary components).

## Corner-ID assignment

With the alignment fixed, each board-spec inner-corner position is mapped
through the transform into the image and matched to a detected chessboard
corner. Only **inner-cell intersections** receive IDs — the marker
corners themselves are not emitted. Each emitted corner carries:

- its absolute ChArUco `id` (identical to OpenCV's `CharucoBoard`
  numbering),
- a `target_position` in board units (mm when `cell_size > 0`),
- the sub-pixel `position` from the chessboard stage.

A final **corner validation** pass checks each detected corner against its
marker-predicted seed; a corner that deviates beyond
`corner_validation_threshold_rel × px_per_square` triggers a
marker-constrained redetection or is dropped. This is the marker-aware
half of ChArUco's precision: the chessboard layer already guarantees no
wrong `(i, j)` labels, and marker-ID consistency guards the ID assignment
on top.

## Why marker-anchored, not grid-only

A bare chessboard grid has no canonical origin — the detector does not
know which physical corner is `(0, 0)`. The markers break that symmetry:
because each marker ID is unique and tied to a known board cell, even a
partial view recovers the *absolute* board frame, so corners from
different frames or cameras share IDs without manual stitching. This is
the same anchoring idea the [marker board](pipeline_marker.md) achieves
with three reference circles, but with per-cell IDs instead of a 3-point
pose.

## Cross-references

- [ArUco bit decode](algo_aruco_decode.md) — supplies the decoded marker
  IDs and confidences.
- [ChArUco pipeline](pipeline_charuco.md) — the full end-to-end stage map.
- [calib-targets-charuco crate](charuco.md) and
  [ChArUco Alignment and Refinement](charuco_alignment.md) — the crate's
  API and refinement pass.
