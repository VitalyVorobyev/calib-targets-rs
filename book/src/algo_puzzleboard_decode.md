# PuzzleBoard edge-code decode

> Code: `calib-targets-puzzleboard` (edge sampling + the `(D4, origin)`
> sweep). Based on Stelldinger 2024,
> [arXiv:2409.20127](https://arxiv.org/abs/2409.20127).

A PuzzleBoard is a self-identifying chessboard: every **interior edge**
carries a midpoint dot, and the dot pattern uniquely identifies any
≥ 4×4 fragment's position on a fixed **501×501 master code**. Edge-code
decode turns the visible dots into an absolute position on that master,
so a partial view still produces absolute corner IDs and object-space
coordinates.

## The master code

The board uses two embedded cyclic maps, committed as binary blobs so the
runtime detector constructs nothing:

- **map A**, shape `(3, 167)`, for **horizontal** interior edges.
- **map B**, shape `(167, 3)`, for **vertical** interior edges.

Dots encode bits directly: **white dot = 0, black dot = 1**. Around a
corner `(i, j)` the four incident interior edges read:

```text
corner (i,j) ---- A(j,i) ---- corner (i+1,j)
     |                            |
   B(j,i)                      B(j,i+1)
     |                            |
corner (i,j+1) -- A(j+1,i) -- corner (i+1,j+1)
```

The maps are cyclic of period 501, so any sufficiently large window of
edges pins a unique master origin — the paper's uniqueness property.

## Stages

1. **Edge sampling.** For each interior edge of the detected grid, sample
   a disk of radius `sample_radius_rel × edge_len` (min 1 px) at the edge
   midpoint, derive local bright/dark references from the two adjacent
   cells, and classify the midpoint into `bit ∈ {0, 1}` with a
   **confidence** `∈ [0, 1]` proportional to how far the midpoint sits
   from the reference mid-level.
2. **Confidence filter.** Drop bits below `min_bit_confidence`; a
   low-confidence bit becomes "unknown" rather than a guessed 0/1.
3. **Minimum-edges gate.** Require enough surviving edges for at least a
   `min_window × min_window` fragment (`min_window² ≥ 4²` is the paper's
   uniqueness floor for the 501×501 code). A sparse grid fails here
   immediately.
4. **Origin sweep.** Find the best `(D4 rotation, master_origin_row,
   master_origin_col)` hypothesis. This is a two-axis choice:
   - **Search scope** — `Full` enumerates all `8 × 501 × 501`
     hypotheses against the master maps; `FixedBoard` scans only
     `8 × (rows+1)²` hypotheses against a declared `PuzzleBoardSpec`'s own
     bit pattern (much cheaper, and it sidesteps the per-view origin
     drift described below).
   - **Scoring** — `HardMajority` majority-votes the bits and gates on a
     bit-error-rate threshold (`max_bit_error_rate`); `SoftLogLikelihood`
     sums `log_sigmoid(κ × bit_confidence × ±1)` per bit (clipped to a
     floor), picks the max, and tracks a `(best − runner-up)` **margin**
     for ambiguity gating. The soft mode is more robust on ambiguous /
     near-symmetric fragments.
5. **Best-component selection.** When several disconnected grid
   components decode, rank them by edges-matched, then BER, then soft
   score. **Conflict detection**: two well-supported components that
   disagree on the master origin are an unrecoverable ambiguity and are
   refused rather than guessed.

## The partial-view guarantee

For a given printed board, *any* subset of its corners decodes to the
same master IDs a full-view decode would produce. This holds across
single-camera captures that frame only part of a large board and across
multi-camera rigs where each camera sees a different fragment — in both
cases overlapping corners share master IDs without further stitching.

The per-view *master origin* is otherwise not fixed: it shifts with which
print-corner the chessboard stage picked as local `(0, 0)`, which depends
on what the camera saw. `FixedBoard` sidesteps that by scoring against the
declared board rather than the full master.

## Decoder-design note

The naive hard-bit decoder + `501² × D4` exhaustive sweep + hard BER gate
already clears precision and recall at zero wrong labels on the workspace
regression set. A coherent-hypothesis matcher upgrade is **deferred** —
do not pre-emptively rewrite without a concrete precision gap demonstrated
on a new dataset.

## Cross-references

- [PuzzleBoard pipeline](pipeline_puzzleboard.md) — the end-to-end target
  detector (chessboard grid + this decoder).
- [calib-targets-puzzleboard crate](puzzleboard.md) — the crate's API,
  search modes, and printable examples.
- [Topological grid finder](algo_topological_grid.md) — the upstream grid
  whose interior edges this decoder samples.
