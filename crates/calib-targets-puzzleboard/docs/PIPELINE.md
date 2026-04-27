# PuzzleBoard detection pipeline — atomic stages

Concise stage-by-stage map of `calib-targets-puzzleboard`'s detector.
The PuzzleBoard is a self-identifying chessboard — every interior edge
carries a midpoint dot, and the dot pattern uniquely identifies any
≥ 4×4 fragment's position on a 501×501 master code (Stelldinger 2024,
[arXiv:2409.20127](https://arxiv.org/abs/2409.20127)).

## Stage table

| # | Name | In | Out | Decision | Failure modes | Knobs |
|---|---|---|---|---|---|---|
| 0 | chessboard grid detect | `&[Corner]` (ChESS raw) | `Vec<ChessDetection>` (multi-component) | `ChessDetector::detect_all` with caller-chosen `graph_build_algorithm` (defaults to `ChessboardV2`; PuzzleBoard does NOT pin) | no grid components qualify | every `chessboard.*` knob from `DetectorParams` (full pipeline of `crates/calib-targets-chessboard/docs/PIPELINE.md`) |
| 1 | edge sampling | labelled corners + image | `Vec<PuzzleBoardObservedEdge>` (`bit ∈ {0,1}, confidence ∈ [0,1]` per interior edge) | per-edge: sample a disk of radius `sample_radius_rel × edge_len` (min 1 px) centred at the edge midpoint; compute local bright/dark references from adjacent cells; classify mid-pixel against threshold; **confidence** = `clip\|(midpoint − ref_mean) / (0.5 × dynamic_range)\|` | edge midpoint outside image; low-contrast cell pair (bright ≈ dark) | `sample_radius_rel` (default `0.2`) |
| 2 | bit confidence filter | observed edges | edges with `confidence ≥ min_bit_confidence` | hard threshold drop | low-confidence bits become unknown; if too few survive → `NotEnoughEdges` error | `min_bit_confidence` (default `0.5`) |
| 3 | minimum-edges gate | filtered edges | pass / fail | require `edges_filtered ≥ required_edges(min_window)`, where `min_window² ≥ 4²` is the paper's uniqueness floor for a 501×501 code | sparse grid / small ROI fails this gate immediately | `min_window` (default `4` → 16 inner edges) |
| 4a | (Full × Hard) origin sweep | filtered edges + master maps A, B | `(D4 rotation, master_origin_row, master_origin_col)` + BER | enumerate **all** `8 × 501 × 501` hypotheses; per-hypothesis: majority-vote bits, count matches, compute BER; retain iff `BER ≤ max_bit_error_rate` | board too small or too noisy (every hypothesis exceeds BER gate); **does not handle ambiguous fragments well — soft mode below is more robust** | `max_bit_error_rate` (default `0.3`), `search_mode = Full`, `scoring_mode = HardMajority` |
| 4b | (Full × Soft) origin sweep | filtered edges + master maps | best `(D4, origin)` + soft score + margin | per-hypothesis: sum `log_sigmoid(κ × bit_confidence × ±1)` clipped to `per_bit_floor`; pick max; track `(best − runner_up)` margin for ambiguity gating | very few high-confidence bits; near-symmetric fragments produce small margin | `bit_likelihood_slope` (κ), `per_bit_floor`, `score_margin_gate` |
| 4c | (FixedBoard × Hard) origin sweep | filtered edges + declared `PuzzleBoardSpec` | `(D4, origin)` within board bounds | scan only `8 × (rows+1)²` hypotheses (the declared board); same BER gate as Full | board origin unknown / wrong spec | `search_mode = FixedBoard`, `scoring_mode = HardMajority` |
| 4d | (FixedBoard × Soft) origin sweep | filtered edges + spec | `(D4, origin)` + soft score | same scope reduction as 4c, soft-likelihood ranking from 4b | — | `search_mode = FixedBoard`, `scoring_mode = SoftLogLikelihood` |
| 5 | best-component selection | per-component decode results | single `PuzzleBoardDecodeInfo` | when `search_all_components = true`, rank components by `edges_matched` (primary), then BER (secondary), then soft-score / hard-tie-break; **conflict detection**: two well-supported components disagreeing on master origin → `InconsistentPosition` error | multiple sub-grids with disagreeing decodes (unrecoverable ambiguity) | `search_all_components` (default `true`) |
| 6 | emit detection | best decode | `PuzzleBoardDetectionResult { detection, decode: PuzzleBoardDecodeInfo }` | rebase `(i, j)` to non-negative; sort by `(j, i)` | — | — |

## What PuzzleBoard inherits from chessboard-v2

The full chessboard-v2 pipeline runs on the input ChESS corners
(BFS, validation loop, Stage 6 / 6.5 / 6.75, boosters, **mandatory
geometry check**). Wrong `(i, j)` labels at the chessboard layer
become wrong absolute master labels under decode — same precision-
unrecoverable property as ChArUco.

PuzzleBoard does **not** pin `graph_build_algorithm`. The caller can
choose Topological for clean planar boards or ChessboardV2 (default)
when in doubt.

## Decoder algorithm decision (2026-04-20, see agent memory)

The naive hard-bit decoder + 501²×D4 exhaustive sweep + hard BER gate
already clears precision/recall at zero wrong labels on the
`130x130_puzzle` regression set (119/120). **Do not pre-emptively
rewrite to a coherent-hypothesis matcher** without a concrete
precision gap demonstrated on a new dataset.

## Diagnose dump

`PuzzleBoardDecodeInfo`:
- `edges_matched` — count of bits where decoded ↔ observed agree
- `bit_error_rate` — `1 − edges_matched / total_bits`
- `master_origin_row`, `master_origin_col` — chosen origin on the
  501×501 master
- D4 rotation index (0..7)
- (when soft mode) `soft_score`, `runner_up_score`, `score_margin`

The embedded chessboard-v2 `DebugFrame` is preserved for upstream-stage
investigation.

## Cross-references

- `crates/calib-targets-chessboard/docs/PIPELINE.md` — upstream stages.
- `CLAUDE.md` "Regression dataset: 130x130_puzzle (puzzleboard)" —
  precision contract + harness commands.
- `docs/datasets/130x130_puzzle.md` (gitignored, local-only) —
  baseline numbers.
