# PuzzleBoard detection pipeline — atomic stages

Concise stage-by-stage map of `calib-targets-puzzleboard`'s detector.
The PuzzleBoard is a self-identifying chessboard — every interior edge
carries a midpoint dot, and the dot pattern identifies a fragment's
position on a 501×501 master code (Stelldinger 2024,
[arXiv:2409.20127](https://arxiv.org/abs/2409.20127)). Entry:
`PuzzleBoardDetector::detect` (`detector/pipeline.rs`).

## Stage table

| # | Name | In | Out | Decision | Failure modes | Knobs |
|---|---|---|---|---|---|---|
| 0 | chessboard grid detect | `&[ChessCorner]` | `Vec<ChessboardDetection>` (multi-component) | `ChessboardDetector::detect_all` | no grid components qualify | every `chessboard.*` knob (`ChessboardParams`; full pipeline in `crates/calib-targets-chessboard/docs/PIPELINE.md`) |
| 1 | edge sampling | labelled corners + image | `Vec<PuzzleBoardObservedEdge>` (`bit ∈ {0,1}, confidence ∈ [0,1]` per interior edge) | per-edge: sample a disk of radius `sample_radius_rel × edge_len` (min 1 px) centred at the edge midpoint; compute local bright/dark references from adjacent cells; classify mid-pixel against threshold; **confidence** = `clip\|(midpoint − ref_mean) / (0.5 × dynamic_range)\|` | edge midpoint outside image; low-contrast cell pair (bright ≈ dark) | `sample_radius_rel` (default `1/6`) |
| 2 | bit confidence filter | observed edges | edges with `confidence ≥ min_bit_confidence` | hard threshold drop | low-confidence bits become unknown; if too few survive → `NotEnoughEdges` error | `min_bit_confidence` (default `0.15`) |
| 3 | minimum-window gate | filtered edges | pass / fail | require `edges_filtered ≥ required_edges(min_window)` **and** a corner span ≥ `min_window` on *both* axes (a wide-but-short strip meets the count while carrying too little code distance on its thin axis) | sparse grid / small ROI / thin strip fails immediately | `min_window` (default `7` corners → `required_edges(7)` = 60 inner edges) |
| 4a | (Full × Hard) origin decode | filtered edges + master maps A, B | `(D4 rotation, master_origin_row, master_origin_col)` + BER | build the cyclic class tables per transform (`O(501·N)`), then collapse the 501² origin argmax to `O(501)` by crossed-CRT separation (safe because the ranking key is an integer match count); retain iff `BER ≤ max_bit_error_rate` | board too small or too noisy (every hypothesis exceeds the BER gate); less robust than soft on ambiguous fragments | `max_bit_error_rate` (default `0.3`), `search_mode = Full`, `scoring_mode = HardWeighted`, `symmetry_mode = Rotations` (default) |
| 4b | (Full × Soft) origin decode | filtered edges + master maps | best `(D4, origin)` + soft score + margin | same class tables, additionally accumulating `log_sigmoid(κ × bit_confidence × ±1)` clipped to `per_bit_floor`; the `f32` key blocks the CRT separation, so the 501² walk stays — stripped to two table reads and a compare per origin; tracks `(best − runner_up)` margin | very few high-confidence bits; near-symmetric fragments produce small margin | `bit_likelihood_slope` (κ), `per_bit_floor`, `alignment_min_margin`, `symmetry_mode = Rotations` (default) |
| 4c | (FixedBoard × Hard) origin decode | filtered edges + declared `PuzzleBoardSpec` | `(D4, origin)` within board bounds | the declared board is cut from the master, so this is 4a restricted to the board's origin rectangle — reusing the same tables, built over only the residue classes that rectangle reaches, and scanning only shifts that keep every observation on the board | board origin unknown / wrong spec / fragment does not fit the declared board | `search_mode = FixedBoard`, `scoring_mode = HardWeighted`, `symmetry_mode = Rotations` (default) |
| 4d | (FixedBoard × Soft) origin decode | filtered edges + spec | `(D4, origin)` + soft score | same restriction as 4c with the soft ranking of 4b; one pass yields both the log-likelihood top-2 (margin gate) and the matched-count top-2 (uniqueness gate) | — | `search_mode = FixedBoard`, `scoring_mode = SoftLogLikelihood` (**default pair when the board is known**), `symmetry_mode = Rotations` (default; set `RotationsAndReflections` only for mirrored optics) |
| 4e | uniqueness gate | winner + closest competing origin | accept / decline | accept iff `margin > k_winner` (`margin = best_matched − runner_up_matched`, `k_winner = edges_observed − best_matched`); parameter-free, applied to both scorers, with the runner-up taken across all eight transforms | a fragment too small to break D4 symmetry declines rather than inventing an orientation | — |
| 5 | best-component selection | per-component decode results | single `PuzzleBoardDecodeInfo` | when `search_all_components = true`, rank components by `edges_matched` (primary), then BER (secondary), then soft-score / hard-tie-break; **conflict detection**: two well-supported components disagreeing on master origin → `InconsistentPosition` error | multiple sub-grids with disagreeing decodes (unrecoverable ambiguity) | `search_all_components` (default `true`) |
| 6 | emit detection | best decode | `PuzzleBoardDetection { corners, alignment, decode: PuzzleBoardDecodeInfo }` | wrap master coords into `[0, 501)`, assign `id` and `target_position`, rebase `(i, j)` to non-negative, sort by `(j, i)` | — | — |

## What PuzzleBoard inherits from the chessboard detector

The full chessboard topological pipeline runs on the input ChESS
corners (prefilter, axis clustering, the topological grid walk,
booster-driven component recovery, **mandatory final geometry check**).
Wrong `(i, j)` labels at the chessboard layer become wrong absolute
master labels under decode — same precision-unrecoverable property as
ChArUco.

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

`PuzzleBoardDetection.decode` (`PuzzleBoardDecodeInfo`) is always present:
`edges_observed`, `edges_matched`, `mean_confidence`, `bit_error_rate`,
`logical_bits`, `logical_bit_error_rate`, `dot_dissent_rate`, and the chosen
`master_origin_row` / `master_origin_col`; the D4 transform is carried by
`alignment`. The `diagnose` entry points (feature `diagnostics`) add
`PuzzleBoardDiagnostics` — every `observed_edges` sample plus
`PuzzleBoardDecodeDiagnostics` (`score_best`, `score_runner_up`,
`score_margin`, and the runner-up origin / transform).

For upstream grid-stage investigation, run the chessboard trace
(`trace_topological_detection`, behind the chessboard crate's opt-in
`diagnostics` feature) on the same input corners — it is the production
grid path serialized stage-by-stage.

## Code map

| Stage | Function |
|---|---|
| 0 | `calib_targets_chessboard::ChessboardDetector::detect_all` |
| 1 | `detector/edge_sampling.rs::sample_all_edges` (period-3 consensus in `detector/consensus.rs`) |
| 4a–4d | `detector/decode/hard.rs::decode`, `soft.rs::decode_soft`, `fixed.rs::decode_fixed_hard` / `decode_fixed_soft`; CRT origin recovery in `decode/mod.rs::crt_master_row` / `crt_master_col` |
| 5 | `detector/pipeline.rs::is_better_component_decode` |
| 6 | `detector/pipeline.rs::master_ij_to_id` / `wrap_master` |

## PuzzlePole

`PuzzlePoleDetector::detect` (`pole/pipeline.rs`) runs the same pattern
wrapped round a cylinder. Stages 0–2 are unchanged. At a seamless period the
master repeats exactly two piece rows, so a seam-crossing window matches no
master position; the pole is therefore decoded against its own `p`-periodic
code stripe (`pole/code.rs::PoleCode`) through the **fixed**-board decoder
(`decode_fixed_hard` / `decode_fixed_soft` over the pole's board rectangle).
`hard.rs` / `soft.rs` cannot be used: they collapse the row axis by CRT,
which needs `gcd(3, 167) = 1`, while a pole's row moduli are 3 and `p` with
`3 | p`. After the decode, `pole/pipeline.rs` applies a rectangular
window-span floor (deliberately post-decode — before it, a 5×8 fragment
lying one way cannot be told from the same fragment lying the other, and
only one of the two decodes) and a repeated-id injectivity guard.
`pole/geometry.rs::object_position` lifts each corner to a 3-D object
point — the only place in the workspace that produces one. Construction
details: `docs/algorithms/puzzlepole_construction.md`.

## Cross-references

- `crates/calib-targets-chessboard/docs/PIPELINE.md` — upstream stages.
- `docs/algorithms/puzzle_detection_spec.md` — the decoder specification.
- `docs/development/private-dataset-policy.md` — where the regression
  contract and its baseline numbers live (local-only surfaces).
