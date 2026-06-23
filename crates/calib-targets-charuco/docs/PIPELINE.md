# ChArUco detection pipeline — atomic stages

Concise stage-by-stage map of `calib-targets-charuco`'s detector. ChArUco
runs the chessboard grid detector first (on the **topological** grid builder —
the workspace-wide sole builder), then adds marker decoding and corner ID
assignment on top.

## Stage table

| # | Name | In | Out | Decision | Failure modes | Knobs |
|---|---|---|---|---|---|---|
| 0 | chessboard grid detect | `&[Corner]` (ChESS raw) | `Vec<ChessDetection>` (one per disconnected component) | `ChessDetector::detect_all` on the **topological** builder (the only builder; `GraphBuildAlgorithm` is single-variant). The `min_corner_strength` floor from `CharucoParams::for_board` keeps marker-bit saddles out of the grid, so the topological per-cell axis test is never poisoned by marker-internal corners | no grid components qualify (empty / sparse corner cloud) | every `chessboard.*` knob from `DetectorParams` (`crates/calib-targets-chessboard/docs/PIPELINE.md`) |
| 1 | grid smoothness pre-filter | grid corners + image | redetected / removed corners | per-corner position vs midpoint-averaged neighbours; deviation `> grid_smoothness_threshold_rel × px_per_square` triggers a local ChESS redetection or a drop | blurry cells, over-aggressive threshold flags perspective drift | `grid_smoothness_threshold_rel` (default `0.05`), `corner_redetect_params` |
| 2 | marker cell enumeration | corner map `{(i,j) → Point2}` | `Vec<MarkerCell>` | per-cell 4-corner completeness check `{(i,j), (i+1,j), (i+1,j+1), (i,j+1)}`; missing any corner → skip | grid edge / hole cells silently excluded — fewer candidate cells | — |
| 3 | board-level marker matcher | cells + image + board spec | `Vec<MarkerDetection>` + chosen D4 / origin hypothesis | per-cell sampled bits → `log_sigmoid(κ × (bit_confidence × {±1}))`; enumerate `8 × translated_hypotheses`; pick max-likelihood; **margin gate** `(best − runner-up)/|best| ≥ alignment_min_margin`; re-emit markers under the chosen hypothesis (so a marker can never disagree with its alignment) | margin below gate (ambiguous decodes / heavy bit noise); zero cells map to board (ROI mismatch) | `scan.*` (`ScanDecodeConfig`), `max_hamming`, `bit_likelihood_slope` (κ=36, tuned conservatively across the internal regression sets), `per_bit_floor` (−6.0), `alignment_min_margin` (`0.05`), `cell_weight_border_threshold` (`0.5`) |
| 4 | alignment validation | markers + board spec | filtered marker inliers | inlier count `≥ min_marker_inliers` (primary component) or `≥ min_secondary_marker_inliers` (subsequent components); the board matcher is its own gate, so these floors stay low | weak camera pose / occlusion → too few inliers; component refused | `min_marker_inliers` (default `1`), `min_secondary_marker_inliers` (default `1`) |
| 5 | ChArUco corner mapping | chessboard corners + alignment + board | `Detection { corners: LabeledCorner[] }` with global IDs | map each board-spec marker corner position through the alignment transform; only inner-cell intersections (not marker corners themselves) are emitted | marker pattern asymmetry can produce false corners; weak alignment drifts inner corners | — |
| 6 | corner validation | mapped corners + markers + image | validated corners (drop false positives) | each detected corner's position is checked against the marker-predicted seed; deviation `> corner_validation_threshold_rel × px_per_square` triggers a marker-constrained redetection or drop | marker-constrained redetection misses true corners in low-contrast regions | `corner_validation_threshold_rel` (default `0.08`) |
| 7 | emit detection | validated corners + alignment | `CharucoDetectionResult { corners, markers, alignment }` | sort typed ChArUco corners by ID; refuse if surviving count below caller's threshold | — | — |

## What ChArUco inherits from the chessboard detector

ChArUco runs the full chessboard topological pipeline: prefilter, axis
clustering, the topological grid walk, the booster-driven component
recovery, and the **mandatory final geometry check**
(largest-connected-component + looser-`validate()` + the topological
wrong-label check). The chessboard precision contract carries forward —
wrong `(i, j)` labels at the chessboard layer would corrupt every
downstream marker match.

`CharucoParams::for_board` sets two chessboard knobs that adapt the shared
detector to marker scenes:

- **`min_corner_strength = 33.0`** — an absolute ChESS-strength floor that
  cuts the weak corner responses on ArUco marker-bit saddles *before* the
  grid grows. Those weak corners are grid-consistent (they pass the
  homography validation) but lie inside the marker interior; cutting them
  early keeps the topological grid out of the markers entirely, so the
  per-cell axis test is never poisoned by marker-internal X-corners. This
  floor — not marker presence — is the precision lever that the old
  `SeedAndGrow` pin used to provide.
- **`enable_final_edge_shape_check = false`** — the standalone chessboard
  detector enables the stricter final edge-shape gate by default; ChArUco
  keeps the chessboard component recall-oriented because the marker-ID and
  board-alignment validation downstream is its precision gate.

`chessboard.graph_build_algorithm` is the topological builder (the only
builder; the field is a single-variant reserved seam). A config-supplied
chessboard override carrying a legacy value is re-pinned to `Topological`
on load.

## Diagnose dump

`CharucoDetectDiagnostics { components: Vec<ComponentDiagnostics> }`,
one entry per chessboard component returned by `detect_all`. Each
component carries:

- `chess_corner_count`, `candidate_cell_count`
- `BoardMatchDiagnostics`: per-cell Otsu threshold, border-black estimate,
  sampled bits, best-vs-runner-up hypotheses, chosen score and margin
- `ComponentOutcome { status, markers, charuco_corners }` — final
  bookkeeping

For upstream grid-stage investigation, run the chessboard topological
trace (`pipeline::trace_topological`) on the same input corners — it is
the production path serialized stage-by-stage. See the chessboard
diagnose workflow referenced below.

## Cross-references

- `crates/calib-targets-chessboard/docs/PIPELINE.md` — the upstream
  topological grid stages this detector inherits.
- `docs/development/detection-pipeline.md` — why the topological builder
  is precision-safe on marker scenes given the `min_corner_strength`
  floor (the concern the old ChArUco `SeedAndGrow` pin guarded against).
- `CLAUDE.md` "Evidence-driven detector debugging" — methodology for
  investigating ChArUco failures (start with the per-component
  `BoardMatchDiagnostics`, then the embedded `DebugFrame`).
