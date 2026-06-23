# PuzzleBoard pipeline

> Composes: the [chessboard grid stack](pipeline_chessboard.md) +
> [PuzzleBoard edge-code decode](algo_puzzleboard_decode.md).
> **Source of truth:** `crates/calib-targets-puzzleboard/docs/PIPELINE.md`.
> Crate reference: [calib-targets-puzzleboard](puzzleboard.md).

A PuzzleBoard is a self-identifying chessboard: every interior edge
carries a midpoint dot, and the dot pattern uniquely identifies any ≥ 4×4
fragment's position on a 501×501 master code. The pipeline runs the full
[chessboard grid stack](pipeline_chessboard.md) first, then samples the
interior-edge dots and decodes them into **absolute master positions** —
so a visible fragment still yields absolute corner IDs and object-space
coordinates.

## End-to-end stages

| # | Stage | In → Out | What it does |
|---|---|---|---|
| 0 | chessboard grid detect | ChESS corners → `Vec<ChessDetection>` | The full [chessboard pipeline](pipeline_chessboard.md) (multi-component). Wrong `(i, j)` labels here become wrong absolute master labels — same precision-unrecoverable property as ChArUco. |
| 1 | edge sampling | labelled corners + image → observed edges | Per interior edge: sample a disk of radius `sample_radius_rel × edge_len` at the midpoint, derive bright/dark references from the adjacent cells, classify into `bit ∈ {0,1}` with a confidence `∈ [0,1]`. |
| 2 | bit-confidence filter | observed edges → high-confidence edges | Drop bits below `min_bit_confidence`; low-confidence bits become unknown. |
| 3 | minimum-edges gate | filtered edges → pass / fail | Require enough edges for a `min_window × min_window` fragment (`min_window² ≥ 4²`, the uniqueness floor). |
| 4 | origin sweep | filtered edges + master maps → `(D4, origin)` + score | Enumerate `(D4 rotation, master_origin)` hypotheses — `Full` (`8 × 501 × 501`) or `FixedBoard` (`8 × (rows+1)²`) scope, with `HardMajority` (BER gate) or `SoftLogLikelihood` (margin gate) scoring. See the [decode algorithm](algo_puzzleboard_decode.md). |
| 5 | best-component selection | per-component results → one decode | Rank components by edges-matched, then BER, then soft score. **Conflict detection**: two well-supported components disagreeing on master origin → `InconsistentPosition` (refuse, don't guess). |
| 6 | emit detection | best decode → result | Rebase `(i, j)` to non-negative; sort by `(j, i)`; assign absolute IDs (`j·501 + i`) and `target_position` (`i·cell_size, j·cell_size`). |

## What it inherits from the chessboard detector

The full chessboard topological pipeline runs on the input ChESS corners —
prefilter, [axis clustering](algo_axis_clustering.md), the
[topological grid walk](algo_topological_grid.md), booster-driven
[component recovery, and the mandatory final geometry
check](algo_recovery_validation.md). PuzzleBoard already defaulted to the
topological builder, which is now the only builder; `graph_build_algorithm`
is a single-variant reserved seam.

## The partial-view guarantee

For a given printed board, any subset of its corners decodes to the same
master IDs a full-view decode would produce — across single-camera
captures that frame only part of a large board and across multi-camera
rigs where each camera sees a different fragment. Overlapping corners
share master IDs without further stitching. `FixedBoard` mode sidesteps the
per-view master-origin drift by scoring against the declared board rather
than the full master.

## Failure modes

| Symptom | Likely stage | What it means / knob to try |
|---|---|---|
| No grid components | Stage 0 (chessboard) | Sparse / empty corner cloud — see the [chessboard failure modes](pipeline_chessboard.md#failure-modes). Try `--upscale` on small boards. |
| `NotEnoughEdges` | Stage 2–3 | Too few high-confidence edge bits survived. Lower `decode.min_bit_confidence`; check that interior dots are resolved at this image scale. |
| Every hypothesis over BER | Stage 4 (`HardMajority`) | Board too small or too noisy. Raise `decode.max_bit_error_rate`, or switch to `SoftLogLikelihood` (more robust on ambiguous fragments). |
| Small / ambiguous decode margin | Stage 4 (`SoftLogLikelihood`) | Near-symmetric fragment or few high-confidence bits. Capture a larger fragment; the margin gate is *correctly* refusing a coin-flip. |
| `InconsistentPosition` | Stage 5 | Two sub-grids disagree on the master origin — an unrecoverable ambiguity. Crop to a single board, or use `FixedBoard` with the known spec. |
| Wrong absolute IDs | **never** | A wrong chessboard `(i, j)` would cause this — file a bug at the chessboard layer; the decode itself is exhaustive and gated. |

## Tuning

The grid side is the standard chessboard `DetectorParams` (under
`params.chessboard`); the decode side is `params.decode`:

- **`min_bit_confidence`** (default `0.5`) — the confidence floor for an
  edge bit to count. Lower on blurry boards; too low admits noise bits.
- **`max_bit_error_rate`** (default `0.3`) — the `HardMajority` BER gate.
- **`min_window`** (default `4`) — the uniqueness floor; rarely changed.
- **`search_mode`** — `Full` (default) vs `FixedBoard` (cheaper, fixes
  per-view origin drift when the board is known).
- **`scoring_mode`** — `HardMajority` (default) vs `SoftLogLikelihood`
  (robust on ambiguous fragments; adds the margin gate).
- **`search_all_components`** (default `true`) — decode every grid
  component and pick the best, with conflict detection.

For threshold-sensitive images use
`PuzzleBoardParams::sweep_for_board(&spec)` with
`detect_puzzleboard_best`. The sweep tries the default soft scorer first,
then a hard-weighted fallback at the paper's 40% BER allowance for
high-distortion fragments. The naive hard-bit decoder already clears the
precision/recall contract at zero wrong labels; do not rewrite to a
coherent-hypothesis matcher without a demonstrated precision gap.

## Cross-references

- [PuzzleBoard edge-code decode](algo_puzzleboard_decode.md) — the decoder
  algorithm in detail.
- [calib-targets-puzzleboard](puzzleboard.md) — the crate API, search
  modes, and printable examples.
- `crates/calib-targets-puzzleboard/docs/PIPELINE.md` — the canonical
  stage map this page mirrors.
