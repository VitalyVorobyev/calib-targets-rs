# Performance: methodology, bottlenecks, optimization backlog

The living reference for detection performance: how it is measured, where the
time goes today, and what to optimize next. Capture how-to lives in
[`profiling.md`](profiling.md); this page is the *ranked* output of that tooling
plus the standing optimization plan.

All numbers here are from public `testdata/` images or synthetic fixtures.
Per-frame numbers from private regression sets stay in the local-only campaign
report (disclosure policy) — this page carries general ranges only.

## Methodology

Run `bash scripts/run-perf-campaign.sh` (see [`profiling.md`](profiling.md)). It
produces four complementary views, each measuring a different thing:

| View | Tool | What it isolates |
|---|---|---|
| End-to-end latency | `bench run` | Whole chessboard pipeline per frame (p50/p95/max). |
| Per-stage breakdown | `topo_stage_timing` | Tracing-span stages, corner-detect → output assembly ([`profiling.md`](profiling.md)). |
| Micro-benches | `cargo bench` | Grid build (synthetic), and the corners/chessboard/decode split per target. |
| Flamegraphs | samply | Where self-time concentrates inside the hot binary. |

The criterion *corners / chessboard / decode* split is the key separator: it
attributes cost to **external corner detection** vs **our grid build** vs **our
marker decode**.

The published report under `.github/pages/performance/` is refreshed by
`scripts/gen-perf-data.sh`, which also regenerates the four committed preview
PNGs (`img/{small,mid,large,oblique}.png`) as **detection
overlays** — grid corners + edges, plus decoded ArUco marker quads on the
ChArUco cards — drawn by `full_stage_timing --overlay-dir` from the same
detection the card's numbers come from (`large.png` ships at half size). The OpenCV baseline comparison block is a
separate, opencv-dependent refresh — see `scripts/gen-comparison-data.sh`.

### OpenCV baseline comparison

`scripts/gen-comparison-data.sh` adds a `comparison` block to the report
(`tools/compare_opencv_baseline.py`, run from the binding venv with
`opencv-python-headless`). It pits `calib-targets` against OpenCV on the two
public report frames — `mid.png` (`findChessboardCornersSB`) and `small.png`
(`aruco.CharucoDetector`) — on **recall** and **runtime**. Honesty rules baked
into the harness:

- **Runtime is each detector's native p50.** OpenCV is timed in `cv2`; ours is
  read from the Rust `full_stage_timing` measurement in `data.json` — *not*
  timed through the Python binding, whose result-marshalling adds ~10× overhead
  unrelated to detection. So run `gen-perf-data.sh` before this.
- **Recall only where it is well-defined.** `mid.png` is a full board (known
  77 inner corners), so recall is matched/77 for each detector, and OpenCV's
  all-or-nothing failure mode is shown explicitly. `small.png` is a partial
  ChArUco view with no independent ground truth, so it is a *detected-count*
  comparison (markers + corners), never dressed up as recall.
- **OpenCV gets its best shot** (both ChArUco pattern conventions are tried;
  the better is reported), so the comparison never sandbags it.

## Where the time goes

Cost is regime-dependent. The external ChESS corner detector dominates
plain-chessboard and large ChArUco frames; on small, marker-dense ChArUco frames
the corner, grid and decode stages are comparable; on dense, oblique PuzzleBoard
frames the owned grid build leads. The largest *owned* costs are the dense-board
grid build and, on marker-dense boards, decode. Absolute per-frame numbers
depend on the `chess-corners` version and the machine, so they are produced by
the campaign (`full_stage_timing`, the published report) rather than recorded
here.

**Covariance and quality cost.** The corner covariance is off by default
(`Uncertainty::Off`); enabling it (`Uncertainty::Propagated`) adds a per-frame
noise estimate plus a per-corner Jacobian, paid for every raw corner, including
those the grid builder later discards. Measuring the per-detection
`DetectionQuality` (covariance statistics plus the leave-one-out local check) is
under 0.5 % of end-to-end. The per-corner local check uses the
opposite-neighbour line intersection rather than an explicit 8-parameter
homography fit, which is an order of magnitude costlier.

The `small.png` grid-build and decode figures include the chessboard wrong-label
geometry check, which ChArUco keeps enabled — a correctness cost, not a
regression to recover.

### Tier 1 — ChESS corner detection (external `chess-corners`)

The largest stage on the plain chessboard and the large ChArUco frame; it scales
with image area. We do not own the implementation, so the levers are
configuration (resolution, ROI, orientation method), not code.

### Tier 2 — marker decode (our code)

- **PuzzleBoard decode.** Complexity per search mode and transform set is
  specified in [`puzzle_detection_spec.md`](../algorithms/puzzle_detection_spec.md).

  Measure with `decode::tests::decode_scaling_report` (decode only) and
  `puzzleboard_stage_timing` (per-stage, public fixture). The share depends on
  mode and declared board size — on a public 20×20 fixture decode is ~61 % of
  `detect` under `Full × Soft` but ~5 % under `FixedBoard`; declaring a mid-size
  board puts it at ~46 %, a full 501×501 board at ~75 %. The published
  `decode_ms` also includes edge-dot sampling, which the transform count does
  not touch.

  Inside the precompute, `build` splits into `fill` (bucket observations by
  residue, `O(N)`) and `class_credit` (a cyclic cross-correlation of the residue
  histogram against the map,
  `H[a][b] = Σ_{row,col} S[row][col] · M[(a+row) mod 167][(b+col) mod 3]`,
  `O(buckets · 501)`), timed separately by `puzzleboard_stage_timing`.
  Observations are nearly free; residue buckets are what cost. `class_credit`
  folds each residue family (buckets sharing a long-axis residue) into
  `T[pattern][short class]` first, so three 501-cell sweeps become one 167-cell
  sweep; on `oblique.png` (`Full × Soft`) it is ≈0.125 ms of a ≈0.17 ms
  `decode_edges`, and ≈0.013 ms under `FixedBoard`. `tables::tests` pins the
  folded result against an `O(501 · N)` transcription of the definition.

  **Measured and rejected — do not retry:**
  - *Merging the bit-0 and bit-1 bucket at one residue into a signed sweep.*
    Valid but useless: zero residues carry both bits across all three public
    fixtures (period-3 redundancy makes every observation at a residue a replica
    of the same code bit).
  - *`get_unchecked` in the inner loop.* 0.125 → 0.115 ms, 0.3 % of `detect`,
    inside noise; the loop is ~0.43 ns/element, near the store-port limit. Not
    worth the first `unsafe` outside the FFI crate on a precision-critical path.
  - *Fusing the physical and voted views into one traversal.* Caps out at
    ≈0.06 ms (2 % of `detect`) for a refactor threading two observation sets
    through the hard, soft and fixed-board scans and the uniqueness fold.
- **ChArUco board match.** A per-cell bit-log-likelihood table keeps
  `log_sigmoid` out of the hypothesis-scoring inner loop. Decode is
  the largest or near-largest stage only on marker-dense boards. Not a current
  optimization target.

### Tier 3 — topological grid build (our code)

**Corner-count-bound, so regime-dependent.** Sub-millisecond on sparse ~1 MP
boards (≈0.3 ms), but **~1–5 ms on dense, high-resolution boards** (thousands
of corners); synthetic `detect_grid_all/square_positions` is ≈4.5 ms. Ranked
p50 on the clean set (`testdata/02-topo-grid`, ring-fit):

| Stage | p50 |
|---|---|
| `ordering` (build detections) | 0.114 ms |
| `recovery` | 0.055 ms |
| `clustering` | 0.020 ms |
| `walk` (label components) | 0.015 ms |
| `edge_classification` | 0.011 ms |
| `cell_size_filter` / `triangulation` | ~0.007 ms |
| `triangle_merge` / quad filters | ~0.001 ms |
| `component_merge` | ~0 (single component) |

On a dense 4032×3024 public frame (`puzzleboard_reference/example6.png`, ~20 k
corners, single component) the grid build is **27.5 ms**, and two stages own it:

| Stage | p50 (ms) | % of grid build |
|---|---|---|
| `ordering` (`build_topological_detections`) | 8.97 | 33% |
| `recovery` (`recover_topological_components`) | 7.46 | 27% |
| clustering | 2.00 | 7% |
| everything else | <1 each | — |

Both are dominated by the **per-corner local-homography solve** in the
precision gate (`validate` → `local_h_residual`, an 8×8 LU per labelled corner,
re-run each grow iteration during recovery). The same hot spot shows on small
distorted boards: `oblique.png` (361 corners) spends ≈0.91 ms of a ≈1.73 ms grid
build in `ordering` (52 %); `example2.png` (180 corners) 0.48 of ≈1.07 ms (45 %).
Both are single-component, so `merge_components_local` is ≈0 there.

Caveats:

- **`merge_components_local`** is `O(C² × 8 transforms)`, so it grows on
  multi-component (distorted / occluded) frames, which single-component timings
  under-represent.
- **Orientation-free grid build is ~8.5× the oriented path.** Synthetic
  `detect_grid_all`: `square_positions` ≈4.5 ms vs `square_oriented2` ≈0.53 ms
  (`hex_positions` ≈0.19 ms). It matters for standalone orientation-free use of
  `projective-grid`.

## Optimization backlog

Ranked by measured impact. **Correctness first: no item may trade a
false-positive risk for speed** — a wrong `(i, j)` label is unrecoverable for
calibration.

1. **Corner-detection configuration (Tier 1, highest leverage).** Keep `RingFit`
   default; offer optional downscale for large frames and ROI when a board prior
   exists. *Risk:* downscale trades corner-localization precision — validate
   recall/precision, never silently.
2. **Per-corner local-H solve in the precision gate (Tier 3, top owned grid
   cost).** `ordering` + `recovery` own 60 % of a 27.5 ms dense-board grid build
   and `ordering` alone 45–52 % on small distorted boards. *Approach (TODO):*
   memoize per-component local-H bases across grow iterations, or reduce
   validation re-runs — **not** a different solver (FP drift). *Risk:* HIGH —
   this is the false-positive gate with determinism contracts; must stay
   byte-exact on both regression sets, in a dedicated behaviour-gated PR.
   Allocation removal in `pick_local_h_base` measured within noise (the cost is
   the LU + neighbour lookups) — do not retry without a flamegraph showing
   allocation dominating.
3. **PuzzleBoard declared-full-master readout.** The only remaining heavy decode
   case: the origin-rectangle readout (`T × 252k` table lookups) when a full
   501×501 board is declared — nothing left to restrict and the CRT collapse
   does not apply to a sub-rectangle. Natural SIMD target, low urgency
   (declaring the full master conveys no information). Options: SIMD; `rayon`
   over searched transforms; the same family fold for `decode_fixed_board_soft`
   (different table shape). *Risk:* the workspace has no parallelism in its own
   code and a past `HashMap`-iteration non-determinism bug — output must stay
   bit-exact and deterministic.
4. **`merge_components_local` `O(C²)` on multi-component frames.** Prune the
   transform/component search. Changes which candidates are considered → **not**
   byte-exact, needs a behaviour gate. *Risk:* preserve the `min(i,j) → (0,0)`
   rebase and never introduce a false merge.
5. **Orientation-free positions-only grid path.** Profile the positions-only
   cell-test / clustering cost and cut the constant (`square_positions` ≈8.5×
   `square_oriented2`). Correctness-neutral.
