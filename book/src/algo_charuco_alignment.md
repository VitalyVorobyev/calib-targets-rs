# ChArUco alignment & corner IDs

> Code: `calib-targets-charuco` (the marker matcher + alignment + corner
> mapping stages). Layout-compatible with OpenCV's aruco/charuco.

A ChArUco board carries an ArUco marker in every white square. Alignment
recovers the board→image transform from the sampled marker cells and then
assigns each chessboard inner corner its **absolute, OpenCV-compatible
corner ID**. This is what makes ChArUco robust to partial views: even a
handful of identifiable markers anchor the whole detected grid to the
board's canonical frame.

## Inputs

- The labelled chessboard grid from the [topological grid
  finder](algo_topological_grid.md) (one or more components).
- Per-cell sampled marker bits (the candidate marker cells extracted from
  the grid).
- The board specification (`CharucoBoardSpec`): rows, cols, dictionary,
  marker layout.

## Alignment — the board-level matcher

The detector uses a single, board-level matcher that chooses the whole
board placement jointly rather than decoding each cell independently and
then voting. It solves one question: which board placement — a D4 rotation
together with an integer `(Δcol, Δrow)` translation on the grid — is most
consistent with *all* the sampled cells at once.

1. **Per-cell × per-marker score matrix.** Each candidate cell is sampled
   into a small bit grid. For every board marker `m` (and each of its four
   rotations) the matcher accumulates a soft-bit log-likelihood
   `Σ_bits max(log_sigmoid(κ · sign · (otsu − mean)/255), per_bit_floor)`,
   where `sign = ±1` is the marker's expected bit and `κ`
   (`bit_likelihood_slope`) sets the per-bit confidence. The `per_bit_floor`
   clip stops a single wildly-wrong bit from dominating a cell's score.
   Each cell also gets a weight, attenuated toward zero when its border did
   not read as black (`cell_weight_border_threshold`).

2. **Hypothesis enumeration.** For each of the four D4 rotations the matcher
   maps the observed cells onto the board and enumerates exactly the integer
   translations that keep every cell inside the board. Each
   `(rotation, translation)` hypothesis scores `Σᵢ wᵢ · sᵢ(m_{p_i(H)})` —
   the weighted score of the marker each cell *would* contain under that
   placement. A hypothesis with no contributing cells is rejected (it would
   otherwise score zero and beat genuine negative-log-likelihood evidence).

3. **Maximum-likelihood placement + margin gate.** The matcher keeps the
   best and runner-up hypotheses and computes the relative margin
   `(best − runner-up)/max(|best|, |runner-up|)`. The placement is accepted
   only when that margin clears `alignment_min_margin`; below it, detection
   is **rejected rather than mislabelled** — heavy bit noise or a
   near-symmetric layout produces a near-tie, and a coin-flip alignment
   would risk a wrong ID.

4. **Constrained re-emit.** Under the chosen placement every cell has a
   single expected marker. The matcher re-emits each cell's marker under
   that identity, so a returned marker can never disagree with the alignment
   it was matched against — the wrong-id count is zero by construction.

The brute-force hypothesis space is tiny (four rotations × a bounded
translation window), so the joint search is cheap. Because the decision is
made over all cells jointly, the matcher tolerates per-cell bit noise that
would defeat an independent hard decode, which is what lets it recover
blurred, tiny-marker, and large-board frames.

The accepted alignment must still clear the downstream inlier floors
`min_marker_inliers` (primary component) / `min_secondary_marker_inliers`
(non-primary components); because the margin gate already does the real
accept/reject work, `for_board` keeps these floors low.

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
