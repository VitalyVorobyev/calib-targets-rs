# PuzzleBoard detection pipeline — atomic stages

Concise stage-by-stage map of `calib-targets-puzzleboard`'s detector.
The PuzzleBoard is a self-identifying chessboard — every interior edge
carries a midpoint dot, and the dot pattern identifies a fragment's
position on a 501×501 master code (Stelldinger 2024,
[arXiv:2409.20127](https://arxiv.org/abs/2409.20127)).

## Stage table

| # | Name | In | Out | Decision | Failure modes | Knobs |
|---|---|---|---|---|---|---|
| 0 | chessboard grid detect | `&[Corner]` (ChESS raw) | `Vec<ChessDetection>` (multi-component) | `ChessDetector::detect_all` on the **topological** grid builder (the only builder; `graph_build_algorithm` is a single-variant reserved seam) | no grid components qualify | every `chessboard.*` knob from `DetectorParams` (full pipeline of `crates/calib-targets-chessboard/docs/PIPELINE.md`) |
| 1 | edge sampling | labelled corners + image | `Vec<PuzzleBoardObservedEdge>` (`bit ∈ {0,1}, confidence ∈ [0,1]` per interior edge) | per-edge: sample a disk of radius `sample_radius_rel × edge_len` (min 1 px) centred at the edge midpoint; compute local bright/dark references from adjacent cells; classify mid-pixel against threshold; **confidence** = `clip\|(midpoint − ref_mean) / (0.5 × dynamic_range)\|` | edge midpoint outside image; low-contrast cell pair (bright ≈ dark) | `sample_radius_rel` (default `1/6`) |
| 2 | bit confidence filter | observed edges | edges with `confidence ≥ min_bit_confidence` | hard threshold drop | low-confidence bits become unknown; if too few survive → `NotEnoughEdges` error | `min_bit_confidence` (default `0.15`) |
| 3 | minimum-window gate | filtered edges | pass / fail | require `edges_filtered ≥ required_edges(min_window)` **and** a corner span ≥ `min_window` on *both* axes (a wide-but-short strip meets the count while carrying too little code distance on its thin axis) | sparse grid / small ROI / thin strip fails immediately | `min_window` (default `7` corners → `required_edges(7)` = 60 inner edges) |
| 4a | (Full × Hard) origin decode | filtered edges + master maps A, B | `(D4 rotation, master_origin_row, master_origin_col)` + BER | build the cyclic class tables per transform (`O(501·N)`), then collapse the 501² origin argmax to `O(501)` by crossed-CRT separation (safe because the ranking key is an integer match count); retain iff `BER ≤ max_bit_error_rate` | board too small or too noisy (every hypothesis exceeds the BER gate); less robust than soft on ambiguous fragments | `max_bit_error_rate` (default `0.3`), `search_mode = Full`, `scoring_mode = HardWeighted`, `symmetry_mode = Rotations` (default) |
| 4b | (Full × Soft) origin decode | filtered edges + master maps | best `(D4, origin)` + soft score + margin | same class tables, additionally accumulating `log_sigmoid(κ × bit_confidence × ±1)` clipped to `per_bit_floor`; the `f32` key blocks the CRT separation, so the 501² walk stays — stripped to two table reads and a compare per origin; tracks `(best − runner_up)` margin | very few high-confidence bits; near-symmetric fragments produce small margin | `bit_likelihood_slope` (κ), `per_bit_floor`, `alignment_min_margin`, `symmetry_mode = Rotations` (default) |
| 4c | (FixedBoard × Hard) origin decode | filtered edges + declared `PuzzleBoardSpec` | `(D4, origin)` within board bounds | the declared board is cut from the master, so this is 4a restricted to the board's origin rectangle — reusing the same tables, built over only the residue classes that rectangle reaches, and scanning only shifts that keep every observation on the board | board origin unknown / wrong spec / fragment does not fit the declared board | `search_mode = FixedBoard`, `scoring_mode = HardWeighted`, `symmetry_mode = Rotations` (default) |
| 4d | (FixedBoard × Soft) origin decode | filtered edges + spec | `(D4, origin)` + soft score | same restriction as 4c with the soft ranking of 4b; one pass yields both the log-likelihood top-2 (margin gate) and the matched-count top-2 (uniqueness gate) | — | `search_mode = FixedBoard`, `scoring_mode = SoftLogLikelihood` (**default pair when the board is known**), `symmetry_mode = Rotations` (default; set `RotationsAndReflections` only for mirrored optics) |
| 5 | best-component selection | per-component decode results | single `PuzzleBoardDecodeInfo` | when `search_all_components = true`, rank components by `edges_matched` (primary), then BER (secondary), then soft-score / hard-tie-break; **conflict detection**: two well-supported components disagreeing on master origin → `InconsistentPosition` error | multiple sub-grids with disagreeing decodes (unrecoverable ambiguity) | `search_all_components` (default `true`) |
| 4e | uniqueness gate | winner + closest competing origin | accept / decline | accept iff `margin > k_winner` (`margin = best_matched − runner_up_matched`, `k_winner = edges_observed − best_matched`); parameter-free, applied to both scorers, with the runner-up taken across all eight transforms | a fragment too small to break D4 symmetry declines rather than inventing an orientation | — |
| 6 | emit detection | best decode | `PuzzleBoardDetection { corners, alignment, decode: PuzzleBoardDecodeInfo }` | wrap master coords into `[0, 501)`, assign `id` and `target_position`, rebase `(i, j)` to non-negative, sort by `(j, i)` | — | — |

## What PuzzleBoard inherits from the chessboard detector

The full chessboard topological pipeline runs on the input ChESS
corners (prefilter, axis clustering, the topological grid walk,
booster-driven component recovery, **mandatory final geometry check**).
Wrong `(i, j)` labels at the chessboard layer become wrong absolute
master labels under decode — same precision-unrecoverable property as
ChArUco.

PuzzleBoard already defaulted to the topological builder, which is now
the only builder. `graph_build_algorithm` is a single-variant reserved
seam.

## Complexity

With `N` filtered edges and `L_r × L_c` the shift rectangle a declared board
admits:

| path | cost |
|---|---|
| Full × Hard | `O(8 · 501 · N)` |
| Full × Soft | `O(8 · (501 · N + 501²))` |
| FixedBoard (either scorer) | `O(8 · (reachable classes · N + L_r · L_c))` |

`reachable classes ≤ 501`, with equality only when the declared board spans the
maps' 167-long period — so declaring a board is cheaper than not declaring one
at every board size below the master's own. `decode::tests::decode_scaling_report`
measures it.

## Diagnose dump

`PuzzleBoardDecodeInfo`:
- `edges_matched` — count of bits where decoded ↔ observed agree
- `bit_error_rate` — `1 − edges_matched / total_bits`
- `master_origin_row`, `master_origin_col` — chosen origin on the
  501×501 master
- D4 rotation index (0..7)
- (when soft mode) `soft_score`, `runner_up_score`, `score_margin`

For upstream grid-stage investigation, run the chessboard topological
trace (`trace_topological`, behind the chessboard crate's opt-in
`diagnostics` feature) on the same input corners — it is the production
grid path serialized stage-by-stage.

## Cross-references

- `crates/calib-targets-chessboard/docs/PIPELINE.md` — upstream stages.
- `docs/algorithms/puzzle_detection_spec.md` — the decoder specification.
- `docs/development/private-dataset-policy.md` — where the regression
  contract and its baseline numbers live (local-only surfaces).
