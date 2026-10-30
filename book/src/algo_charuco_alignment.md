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
   `(rotation, translation)` hypothesis `H` scores
   `Σᵢ wᵢ · (sᵢ(m_{p_i(H)}) + bits² · ln 2)` over the cells it expects a
   marker in: the weighted log-likelihood *ratio* of the marker each cell
   would contain under that placement against a cell of random bits. The
   ratio is what makes hypotheses comparable. A ChArUco board has a marker in
   only half of its squares, so placements shifted by one square expect
   markers in different cells; a cell a hypothesis expects no marker in is
   explained by the random-bits model and adds `0`. A cell that carries no
   pattern (a plain square) scores about `0` against any marker, a cell that
   shows the expected marker scores up to `bits² · ln 2`, and one that
   contradicts it scores below `0`. Summing raw log-likelihoods instead
   (all `≤ 0`) would favour the placement that expects markers in the
   *fewest* informative cells — one that lays the board's plain squares over
   the observed markers. A hypothesis with no contributing cells is
   rejected.

3. **Maximum-evidence placement + margin gate.** The matcher keeps the best
   and runner-up hypotheses and computes the margin
   `(best − runner-up) / Σᵢ wᵢ · bits² · ln 2`, the evidence gap as a share
   of the evidence the best placement's cells could carry. The placement is
   accepted only when its cells favour it over chance (`best > 0`) and the
   margin clears `alignment_min_margin`; below it, detection is **rejected
   rather than mislabelled** — heavy bit noise or a near-symmetric layout
   produces a near-tie, and a coin-flip alignment would risk a wrong ID.
   Normalising by the attainable evidence keeps the gate meaningful when the
   evidence itself is weak: two placements supported only by nearly
   uninformative cells differ by a tiny share of what those cells could show.

4. **Constrained re-emit.** Under the chosen placement every cell has a
   single expected marker. The matcher emits a marker for a cell only when
   the cell's own bits favour that marker over chance, so a returned marker
   can never disagree with the alignment it was matched against, and is
   always backed by its pixels. The marker's `code` and `hamming` are the
   bits read from the cell and their distance to the expected marker.

The brute-force hypothesis space is tiny (four rotations × a bounded
translation window), so the joint search is cheap. Because the decision is
made over all cells jointly, the matcher tolerates per-cell bit noise that
would defeat an independent hard decode, which is what lets it recover
blurred, tiny-marker, and large-board frames.

The accepted alignment must still clear the downstream inlier floors
`min_marker_inliers` (primary component) / `min_secondary_marker_inliers`
(non-primary components); because the margin gate already does the real
accept/reject work, `for_board` keeps these floors low.

## Cell alignment and corner alignment

The matcher places *cells*: a cell is named by its top-left corner, and
the chosen placement carries each marker cell onto the board cell it
lands on. That is not yet the alignment of the corners. Under a rotation
a cell's four corners land on the four corners of its board cell, but its
top-left lands on a different one of them: the top-right under a quarter
turn, the bottom-right under a half turn. The two alignments share their
rotation and differ in translation by that offset.
`calib_targets_core::cell_to_corner_alignment` converts one to the other.
Corner IDs are assigned through the corner alignment, and it is what
`CharucoDetection::alignment` reports: `alignment.apply(corner_grid)` is
the board corner, and `alignment.apply(marker.gc)` the top-left board
corner of the marker's square.

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
