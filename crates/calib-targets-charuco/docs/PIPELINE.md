# ChArUco detection pipeline — atomic stages

Concise stage-by-stage map of `calib-targets-charuco`'s detector. ChArUco
runs the full chessboard-v2 pipeline first (forced via
`graph_build_algorithm = ChessboardV2` — see Stage 0 below for why), then
adds marker decoding and corner ID assignment on top.

## Stage table

| # | Name | In | Out | Decision | Failure modes | Knobs |
|---|---|---|---|---|---|---|
| 0 | chessboard grid detect | `&[Corner]` (ChESS raw) | `Vec<ChessDetection>` (one per disconnected component) | `ChessDetector::detect_all` with **forced** `ChessboardV2` (`pipeline.rs:175` overrides caller choice unconditionally — marker-cell features defeat the topological per-cell axis test, so this pin is a precision guarantee, not a configuration choice) | no grid components qualify (empty / sparse corner cloud) | every `chessboard.*` knob from `DetectorParams` (Stages 0-10 of `crates/calib-targets-chessboard/docs/PIPELINE.md`) |
| 1 | grid smoothness pre-filter | grid corners + image | redetected / removed corners | per-corner position vs midpoint-averaged neighbours; deviation `> grid_smoothness_threshold_rel × px_per_square` triggers a local ChESS redetection or a drop | blurry cells, over-aggressive threshold flags perspective drift | `grid_smoothness_threshold_rel` (default `0.05`), `corner_redetect_params` |
| 2 | marker cell enumeration | corner map `{(i,j) → Point2}` | `Vec<MarkerCell>` | per-cell 4-corner completeness check `{(i,j), (i+1,j), (i+1,j+1), (i,j+1)}`; missing any corner → skip | grid edge / hole cells silently excluded — fewer candidate cells | — |
| 3a | (legacy) marker scan + alignment | cells + image | `Vec<MarkerDetection>` → `CharucoAlignment` | per-cell hard-decode (Otsu threshold, rotation vote, translation vote); `select_alignment` solves for board → image transform from inlier marker IDs | blurry / low-contrast markers; rotation-vote ambiguity at cell edges; wrong-id markers passing decode | `scan.*` (`ScanDecodeConfig`), `max_hamming` |
| 3b | (board-level, soft-bit) marker matcher | cells + image + board spec | `Vec<MarkerDetection>` + chosen D4 / origin hypothesis | per-cell sampled bits → `log_sigmoid(κ × (bit_confidence × {±1}))`; enumerate `8 × translated_hypotheses`; pick max-likelihood; **margin gate** `(best − runner-up)/|best| ≥ alignment_min_margin` | margin below gate (ambiguous decodes / heavy bit noise); zero cells map to board (ROI mismatch) | `use_board_level_matcher` (default `false`), `bit_likelihood_slope` (κ=36, tuned on multi-camera 22×22 + 68×68 datasets), `per_bit_floor` (−6.0), `alignment_min_margin` (`0.05`), `cell_weight_border_threshold` (`0.5`) |
| 4 | alignment validation | markers + board spec | filtered marker inliers | inlier count `≥ min_marker_inliers` (primary component) or `≥ min_secondary_marker_inliers` (subsequent components) | weak camera pose / occlusion → too few inliers; component refused | `min_marker_inliers` (default `8`), `min_secondary_marker_inliers` (default `2`) |
| 5 | ChArUco corner mapping | chessboard corners + alignment + board | `Detection { corners: LabeledCorner[] }` with global IDs | map each board-spec marker corner position through the alignment transform; only inner-cell intersections (not marker corners themselves) are emitted | marker pattern asymmetry can produce false corners; weak alignment drifts inner corners | — |
| 6 | corner validation | mapped corners + markers + image | validated corners (drop false positives) | each detected corner's position is checked against the marker-predicted seed; deviation `> corner_validation_threshold_rel × px_per_square` triggers a marker-constrained redetection or drop | marker-constrained redetection misses true corners in low-contrast regions | `corner_validation_threshold_rel` (default `0.08`) |
| 7 | emit detection | validated corners + alignment | `CharucoDetectionResult { detection, markers, alignment, ... }` | sort by `(j, i)`; refuse if surviving count below caller's threshold | — | — |

## What ChArUco inherits from chessboard-v2

ChArUco runs **all** chessboard-v2 stages: BFS grow, validation loop,
Stage 6 / 6.5 / 6.75 (refit + BFS regrow), boosters, and the **mandatory
final geometry check** (largest-connected-component + looser-`validate()`).
The chessboard-v2 precision contract carries forward — wrong `(i, j)`
labels at the chessboard layer would corrupt every downstream marker
match.

The one override: `chessboard.graph_build_algorithm` is **always**
`ChessboardV2`. Topological grid finding cannot survive marker-internal
X-corners polluting per-cell axis tests.

## Diagnose dump

`CharucoDetectDiagnostics { components: Vec<ComponentDiagnostics> }`,
one entry per chessboard component returned by `detect_all`. Each
component carries:

- `chess_corner_count`, `candidate_cell_count`
- matcher kind (`legacy` / `board-level`)
- `BoardMatchDiagnostics` (when board-level): per-cell Otsu threshold,
  border-black estimate, sampled bits, best-vs-runner-up hypotheses,
  chosen score and margin
- `ComponentOutcome { status, markers, charuco_corners }` — final
  bookkeeping

The embedded chessboard-v2 `DebugFrame` (with its `IterationTrace.{
extension, rescue, refit, extension2, rescue2, geometry_check }`) is
preserved per component for the same diagnose-driven workflow used on
the chessboard side.

## Cross-references

- `crates/calib-targets-chessboard/docs/PIPELINE.md` — the upstream
  chessboard-v2 stages this detector inherits.
- `CLAUDE.md` "Graph-build algorithm selection" — why ChArUco pins
  ChessboardV2.
- `CLAUDE.md` "Evidence-driven detector debugging" — methodology for
  investigating ChArUco failures (start with the per-component
  `BoardMatchDiagnostics`, then the embedded `DebugFrame`).
