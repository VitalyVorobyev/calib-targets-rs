# ChArUco detection pipeline — atomic stages

Concise stage-by-stage map of `calib-targets-charuco`'s detector. ChArUco
runs the chessboard grid detector first, then adds marker decoding and corner
ID assignment on top. Entry: `CharucoDetector::detect`
(`detector/pipeline.rs`).

## Stage table

| # | Name | In | Out | Decision | Failure modes | Knobs |
|---|---|---|---|---|---|---|
| 0 | chessboard grid detect | `&[ChessCorner]` | `Vec<ChessboardDetection>` (one per disconnected component) | `ChessboardDetector::detect_all`. The `min_corner_strength` floor from `CharucoParams::for_board` keeps marker-bit saddles out of the grid, so the topological per-cell axis test is never poisoned by marker-internal corners | no grid components qualify (empty / sparse corner cloud) | every `chessboard.*` knob (`ChessboardParams`; see `crates/calib-targets-chessboard/docs/PIPELINE.md`) |
| 1 | grid smoothness pre-filter | grid corners + image | redetected / removed corners | per-corner position vs midpoint-averaged neighbours; deviation `> grid_smoothness_threshold_rel × px_per_square` triggers a local ChESS redetection or a drop | blurry cells, over-aggressive threshold flags perspective drift | `grid_smoothness_threshold_rel` (default `0.05`), `corner_redetect_params` |
| 2 | marker cell enumeration | corner map `{(i,j) → Point2}` | `Vec<MarkerCell>` | per-cell 4-corner completeness check `{(i,j), (i+1,j), (i+1,j+1), (i,j+1)}`; missing any corner → skip | grid edge / hole cells silently excluded — fewer candidate cells | — |
| 3 | board-level marker matcher | cells + image + board spec | `Vec<MarkerDetection>` + chosen D4 / origin hypothesis | per-cell sampled bits → `log_sigmoid(κ × (bit_confidence × {±1}))`; enumerate `8 × translated_hypotheses`; pick max-likelihood; **margin gate** `(best − runner-up)/|best| ≥ alignment_min_margin`; re-emit markers under the chosen hypothesis (so a marker can never disagree with its alignment) | margin below gate (ambiguous decodes / heavy bit noise); zero cells map to board (ROI mismatch) | `inset_frac` (marker size and ring from the board), `bit_likelihood_slope` (κ=36, tuned conservatively across the internal regression sets), `per_bit_floor` (−6.0), `alignment_min_margin` (`0.05`), `cell_weight_border_threshold` (`0.5`) |
| 4 | alignment validation | markers + board spec | filtered marker inliers | inlier count `≥ min_marker_inliers` (primary component) or `≥ min_secondary_marker_inliers` (subsequent components); the board matcher is its own gate, so these floors stay low | weak camera pose / occlusion → too few inliers; component refused | `min_marker_inliers` (default `1`), `min_secondary_marker_inliers` (default `1`) |
| 5 | ChArUco corner mapping | chessboard corners + alignment + board | `CharucoCorner`s with global IDs | map each board-spec marker corner position through the alignment transform; only inner-cell intersections (not marker corners themselves) are emitted | marker pattern asymmetry can produce false corners; weak alignment drifts inner corners | — |
| 6 | corner validation | mapped corners + markers + image | validated corners (drop false positives) | each detected corner's position is checked against the marker-predicted seed; deviation `> corner_validation_threshold_rel × px_per_square` triggers a marker-constrained redetection or drop | marker-constrained redetection misses true corners in low-contrast regions | `corner_validation_threshold_rel` (default `0.08`) |
| 7 | emit detection | validated corners + alignment | `CharucoDetection { corners, markers, alignment }` | sort typed ChArUco corners by ID; refuse if surviving count below caller's threshold | — | — |

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
  per-cell axis test is never poisoned by marker-internal X-corners.
- **the chessboard's final wrong-label geometry check stays enabled**, exactly
  as for the standalone chessboard detector. Marker decoding is not a
  substitute: the board alignment searches `D4 × integer translation`, a
  *rigid* relabelling of whatever lattice the chessboard produced, so it
  cannot see or repair a labelling that is wrong *within* a component.

## Diagnose dump

`CharucoDetectDiagnostics { components: Vec<ComponentDiagnostics> }`,
one entry per chessboard component returned by `detect_all`. Each
component carries:

- `chess_corner_count`, `candidate_cell_count`
- `BoardMatchDiagnostics`: per-cell Otsu threshold, border-black estimate,
  sampled bits, best-vs-runner-up hypotheses, chosen score and margin
- `ComponentOutcome { status, markers, charuco_corners }` — final
  bookkeeping

For upstream grid-stage investigation, run the chessboard trace
(`trace_topological_detection`, feature `diagnostics`) on the same input
corners — it is the production path serialized stage-by-stage.

## Code map

| Stage | Function |
|---|---|
| 0 | `calib_targets_chessboard::ChessboardDetector::detect_all` |
| 1 | `detector/grid_smoothness.rs::smooth_grid_corners` |
| 2 | `detector/marker_sampling.rs::build_marker_cells` (+ `build_corner_map`) |
| 3–4 | `detector/board_match/mod.rs::match_board` (`match_board_diag` on the diagnose path); bits decoded via `calib-targets-aruco` |
| 5 | `detector/corner_mapping.rs::map_charuco_corners` |
| 6 | `detector/corner_refit.rs::validate_and_fix_corners` (homography via `core::estimate_homography_rect_to_img`) |
| 7 | `detector/merge.rs::merge_charuco_results` (multi-component merge) |

Off the detection path: `link_check.rs::validate_marker_corner_links` lets a
caller audit a finished result against the board spec.

## Cross-references

- `crates/calib-targets-chessboard/docs/PIPELINE.md` — the upstream
  topological grid stages this detector inherits.
- `docs/development/detection-pipeline.md` — why the topological builder
  is precision-safe on marker scenes given the `min_corner_strength`
  floor.
- `docs/development/debugging.md` — methodology for investigating
  failures (for ChArUco, start with the per-component
  `BoardMatchDiagnostics`, then the chessboard trace).
