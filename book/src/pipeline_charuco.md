# ChArUco pipeline

> Composes: the [chessboard grid stack](pipeline_chessboard.md) +
> [ArUco bit decode](algo_aruco_decode.md) +
> [ChArUco alignment & corner IDs](algo_charuco_alignment.md).
> **Source of truth:** `crates/calib-targets-charuco/docs/PIPELINE.md`.
> Crate reference: [calib-targets-charuco](charuco.md).

A ChArUco board carries an ArUco marker in every white square. The
pipeline runs the [chessboard grid detector](pipeline_chessboard.md) first,
then decodes the per-cell markers, aligns them to the board spec, and
assigns each inner corner its **absolute, OpenCV-compatible corner ID**.
The markers are what make ChArUco robust to partial views and unambiguous
about orientation.

## End-to-end stages

| # | Stage | In → Out | What it does |
|---|---|---|---|
| 0 | chessboard grid detect | ChESS corners → `Vec<ChessDetection>` | `ChessDetector::detect_all` on the [topological builder](pipeline_chessboard.md). The `min_corner_strength` floor keeps marker-bit saddles out of the grid (below). |
| 1 | grid smoothness pre-filter | grid corners + image → cleaned corners | Per-corner position vs midpoint-averaged neighbours; a deviation over `grid_smoothness_threshold_rel × px_per_square` triggers a local ChESS redetection or a drop. |
| 2 | marker cell enumeration | corner map → `Vec<MarkerCell>` | Per cell, require all four corners `{(i,j),(i+1,j),(i+1,j+1),(i,j+1)}`; skip incomplete cells. |
| 3 | marker decode + alignment | cells + image → markers + alignment | Decode each cell ([ArUco bit decode](algo_aruco_decode.md)), then solve the board→image transform. Two matchers: legacy hard-decode (rotation + translation vote) or the opt-in board-level soft-bit matcher with a margin gate. See [alignment](algo_charuco_alignment.md). |
| 4 | alignment validation | markers + spec → inliers | Require `≥ min_marker_inliers` (primary component) or `≥ min_secondary_marker_inliers` (subsequent). |
| 5 | ChArUco corner mapping | corners + alignment → IDed corners | Map each board-spec inner-corner position through the alignment; only inner-cell intersections get IDs (not marker corners). |
| 6 | corner validation | mapped corners + markers + image → validated corners | Check each corner against its marker-predicted seed; deviation over `corner_validation_threshold_rel × px_per_square` → marker-constrained redetect or drop. |
| 7 | emit detection | validated corners + alignment → result | Sort typed ChArUco corners by ID; refuse below the caller's threshold. |

## What it inherits from the chessboard detector — and the strength floor

ChArUco runs the full chessboard topological pipeline (prefilter,
[clustering](algo_axis_clustering.md), the [grid walk](algo_topological_grid.md),
[booster recovery, and the mandatory geometry
check](algo_recovery_validation.md)). The chessboard precision contract
carries forward: a wrong `(i, j)` label here would corrupt every
downstream marker match.

`CharucoParams::for_board` sets two chessboard knobs that adapt the shared
detector to marker scenes:

- **`min_corner_strength = 33.0`** — an absolute ChESS-strength floor that
  cuts the weak corner responses on ArUco marker-bit saddles *before* the
  grid grows. Those corners are grid-consistent but lie inside the marker
  interior; cutting them early keeps the topological per-cell axis test
  from being poisoned by marker-internal X-corners. This floor — not marker
  presence — is the precision lever here. (It is the concern the historical
  ChArUco builder pin used to guard; the topological builder is now safe on
  marker scenes *because of* this floor.)
- **`enable_final_edge_shape_check = false`** — ChArUco keeps the
  chessboard component recall-oriented because the marker-ID and
  board-alignment validation downstream is its precision gate.

`chessboard.graph_build_algorithm` is the single-variant topological seam;
a config carrying a legacy value is re-pinned to `Topological` on load.

## Failure modes

| Symptom | Likely stage | What it means / knob to try |
|---|---|---|
| No grid / `None` from Stage 0 | Stage 0 (chessboard) | Sparse corner cloud or clustering failure — see the [chessboard failure modes](pipeline_chessboard.md#failure-modes). |
| `NoMarkers` (grid found, no decodes) | Stage 3 (decode) | Wrong `dictionary` / `marker_size_rel`, or blur. Enable `multi_threshold`, lower `min_border_score`, verify the board spec. |
| `AlignmentFailed { inliers: 0 }` | Stage 4 | No decoded ID is in the layout — board-spec mismatch (`rows`, `cols`, `dictionary`, `marker_layout`) or a non-zero `first_marker` offset. |
| `AlignmentFailed`, small but non-zero inliers | Stage 4 | Partial view or strong perspective — lower `min_marker_inliers` to what you reliably see. |
| Small soft-matcher margin | Stage 3 (board-level matcher) | Ambiguous decodes / heavy bit noise — the margin gate is flagging a low-confidence alignment. |
| Corners drift off true intersections | Stage 6 | Weak alignment — verify the board pose / occlusion; check `corner_validation_threshold_rel`. |
| Wrong corner IDs | **never** | A wrong chessboard `(i, j)` would cause this — file a bug at the chessboard layer. |

## Tuning

ChArUco config layers three surfaces:

- **Chessboard grid** — `CharucoParams.chessboard` is a `DetectorParams`
  (stable core + opt-in `advanced`). `for_board` pre-sets the
  marker-scene-safe `min_corner_strength` floor; do not lower it on marker
  boards.
- **Marker decode** — `scan.*` (`ScanDecodeConfig`): `min_border_score`
  (default `0.75`; lower cautiously to `0.65` on blur), `multi_threshold`
  (default `true`), `inset_frac` (default `0.06`; raise when borders bleed
  into the bit grid). `marker_size_rel` must match the printed board.
- **Alignment** — `min_marker_inliers` (default `8`), the opt-in
  `use_board_level_matcher` and its `alignment_min_margin` /
  `bit_likelihood_slope`, and `corner_validation_threshold_rel`
  (default `0.08`).

Board sampling scale is `px_per_square` (starts at 60 in `for_board`);
adjust it first if the board appears at a very different pixel scale. For
challenging images use `CharucoParams::sweep_for_board(&board)` with
`detect_charuco_best`. See [Tuning the Detector](tuning.md) and
[Troubleshooting](troubleshooting.md).

## Cross-references

- [ArUco bit decode](algo_aruco_decode.md) and
  [ChArUco alignment & corner IDs](algo_charuco_alignment.md) — the two
  marker-side algorithms.
- [calib-targets-charuco](charuco.md) and
  [ChArUco Alignment and Refinement](charuco_alignment.md) — the crate API.
- `crates/calib-targets-charuco/docs/PIPELINE.md` — the canonical stage
  map this page mirrors.
