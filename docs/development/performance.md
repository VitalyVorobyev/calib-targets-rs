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
| Per-stage breakdown | `topo_stage_timing` | 14 tracing-span stages, corner-detect → ordering. |
| Micro-benches | `cargo bench` | Grid build (synthetic), and the corners/chessboard/decode split per target. |
| Flamegraphs | samply | Where self-time concentrates inside the hot binary. |

The criterion *corners / chessboard / decode* split is the key separator: it
attributes cost to **external corner detection** vs **our grid build** vs **our
marker decode**.

The published report under `.github/pages/performance/` is refreshed by
`scripts/gen-perf-data.sh`, which also regenerates the four committed preview
PNGs (`img/{small,mid,large,example2}.png`) as **detection overlays** — grid
corners + edges + decoded ArUco marker quads, drawn by `full_stage_timing
--overlay-dir` from the same detection the card's numbers come from
(`large.png` ships at half size). The OpenCV baseline comparison block is a
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

Three tiers, in order of cost. The headline, **as of the ChArUco decode rewrite
(PR #71)**: the external ChESS corner detector now dominates **every** public
frame — including the ChArUco ones, where decode used to be the largest stage.
The largest *owned* costs are the dense-board grid build and the PuzzleBoard
501² sweep; the once-dominant ChArUco decode has dropped to a minor stage.

Per-stage p50 on the four public report frames (`full_stage_timing`, M4 Pro,
100 reps — the same numbers the published report renders):

| Frame | px | corner detect | grid build | decode | end-to-end |
|---|---|---:|---:|---:|---:|
| `mid.png` (chessboard) | 1024×576 | 1.17 | 0.39 | — | 1.56 |
| `example2.png` (chessboard) | 640×480 | 0.78 | 1.09 | — | 1.87 |
| `small.png` (ChArUco) | 720×540 | 1.02 | 0.56 | **0.70** | 2.29 |
| `large.png` (ChArUco) | 2048×1536 | **5.93** | 2.87 | 2.08 | 10.88 |

Corner detection is the single largest stage on every row; on the ChArUco frames
decode (post-#71) is now *smaller* than both corner detection and grid build.

### Tier 1 — ChESS corner detection (external `chess-corners`)

Corner detection is **the largest stage on every public frame** — ~65–75 % of a
plain-chessboard end-to-end, and still the top stage on the ChArUco frames now
that decode has shrunk. It scales with image area (`large.png`, 3 MP, ≈5.9 ms;
the ~0.4–1 MP frames ≈0.8–1.2 ms). The `disk-fit` orientation method roughly
**doubles** corner-detection cost vs `ring-fit` — the standing reason `RingFit`
is the default.

We *tune* this stage (resolution, ROI, orientation method) but do not own the
implementation, so the levers are configuration, not code. It is the
highest-leverage target precisely because it is now unambiguously the dominant
cost across regimes.

### Tier 2 — marker-decode sweeps (our code)

- **PuzzleBoard master sweep — the top remaining owned decode cost.** The
  full-decode path grows with board size — synthetic `puzzleboard/full` goes
  **3.7 ms (8×8) → 18 ms (30×30)** — matching the `O(8 × 501² + N)` master-pattern
  sweep in `decode/hard.rs` / `soft.rs`. The `KnownOrigin` fast path
  (`fixed_board`) avoids the sweep for the common case. The public
  photo-realistic `synthetic_decode` bench (canonical-map renders) now measures
  this end to end without private data.
- **ChArUco board match — now a minor stage (PR #71 closed this).** Precomputing
  a per-cell bit-log-likelihood table removed the
  `O(cells × markers × 4 × bits²)` `log_sigmoid` evaluations from the
  hypothesis-scoring inner loop: the board-level matcher dropped ~13×. On the
  public report frames decode is now **0.70 ms (`small.png`)** and **2.08 ms
  (`large.png`)** — below corner detection and grid build on both. It is no
  longer a top owned cost and is **not** a current optimization target.

### Tier 3 — topological grid build (our code)

**Corner-count-bound, so regime-dependent.** Sub-millisecond on sparse ~1 MP
boards (≈0.3 ms total), but it grows to **~1–5 ms on dense, high-resolution
boards** (thousands of corners) — and the synthetic `detect_grid_all/
square_positions` is ≈4.5 ms. So on a large/dense board, grid build is
*comparable to* corner detection, not negligible; the 85/15 split is a
small-board phenomenon. Within the build, ranked p50 on the clean set:

| Stage | p50 (02-topo-grid, ring-fit) |
|---|---|
| `ordering` (build detections) | 0.114 ms |
| `recovery` | 0.055 ms |
| `clustering` | 0.020 ms |
| `walk` (label components) | 0.015 ms |
| `edge_classification` | 0.011 ms |
| `cell_size_filter` / `triangulation` | ~0.007 ms |
| `triangle_merge` / quad filters | ~0.001 ms |
| `component_merge` | ~0 (single component) |

On a **dense, high-resolution** board the picture flips. Per-stage on a public
4032×3024 frame (`puzzleboard_reference/example6.png`, ~20 k corners, single
component) — grid build is **27.5 ms**, and two stages own it:

| Stage | p50 (ms) | % of grid build |
|---|---|---|
| `ordering` (`build_topological_detections`) | 8.97 | 33% |
| `recovery` (`recover_topological_components`) | 7.46 | 27% |
| clustering | 2.00 | 7% |
| everything else | <1 each | — |

Both are corner-count-bound and dominated by the **per-corner local-homography
solve** in the precision gate (`validate` → `local_h_residual`, an 8×8 LU per
labelled corner, re-run each grow iteration during recovery). This is the real
owned hot spot — but it lives in determinism-contract-laden, false-positive-gate
code, so it is *not* a safe place to micro-optimize (see backlog item 4).

Two further caveats:

- **`merge_components_local`** reads as ≈0 above because these frames form one
  component. Structurally it is `O(C² × 8 transforms)` with per-iteration
  `HashMap` clones, so it grows on **multi-component** (distorted / occluded)
  frames — which the single-component timing under-represents.
- **Orientation-free grid build is ~8.5× the oriented path.** Synthetic
  `detect_grid_all`: `square_positions` (positions-only evidence) ≈4.5 ms vs
  `square_oriented2` ≈0.53 ms (and `hex_positions` ≈0.19 ms). The positions-only
  path matters for the orientation-free standalone use of `projective-grid`.

## Optimization backlog

Prioritized by measured impact, **re-ranked after PR #71** (ChArUco decode
rewrite). **Every item is correctness-first: none may trade a false-positive
risk for speed** — a wrong `(i, j)` label is unrecoverable for calibration (the
asymmetric detection contract). Optimization work is *planned* here, not yet
applied. With ChArUco decode now a minor stage, the two live owned candidates
are the PuzzleBoard sweep and the dense-board grid build; the dominant cost
across all regimes remains the external corner detector.

1. **Corner-detection configuration levers (Tier 1, highest leverage).**
   *Evidence (refreshed):* the single largest stage on *every* public frame —
   ~65–75 % of a plain-chessboard end-to-end and still the top stage on the
   ChArUco frames now that decode shrank; ≈5.9 ms on the 3 MP frame; `disk-fit`
   ≈2× `ring-fit`. *Approach:* keep `RingFit` default; offer optional downscale
   for large frames and ROI when a board prior exists. *Risk:* downscale trades
   corner-localization precision — validate recall/precision, never silently.
2. **PuzzleBoard 501² sweep (Tier 2 — top remaining owned decode cost).**
   *Evidence:* `full` path 3.7→18 ms with board size; the `O(8×501²)` loop in
   `decode/hard.rs`; now measurable on public canonical-map photos via
   `synthetic_decode`. *Approach:* the documented `O(501×N)` precompute
   alternative; optionally `rayon` over the 8 D4 transforms. *Risk:* the
   workspace has **zero parallelism** in its own code today and a past
   non-determinism bug traced to `HashMap` iteration order — any parallelism must
   keep decode output bit-exact and deterministic.
3. **ChArUco board-match decode — CLOSED (PR #71).** The per-cell
   bit-log-likelihood table removed the hypothesis-scoring inner loop's
   `log_sigmoid` evaluations (~13× faster matcher). Public report decode is now
   0.70 ms (`small.png`) / 2.08 ms (`large.png`) — below corner detection and
   grid build. No further decode optimization is warranted; reopen only if a
   future profile shows it back in the top tier.
4. **Per-corner local-H solve in the precision gate (Tier 3, dense boards).**
   *Evidence:* `ordering` + `recovery` own 60 % of a 27.5 ms grid build on a
   12 MP board; both are dominated by the 8×8 LU in
   `validate → local_h_residual`, re-run per grow iteration. *Approach (deferred,
   TODO):* memoize the per-component local-H bases across grow iterations, or
   reduce the number of validation re-runs — **not** a different solver (FP
   drift). *Risk:* HIGH — this is the false-positive gate with documented
   determinism contracts; any change must stay byte-exact on both regression
   sets. A safe allocation-removal experiment in `pick_local_h_base` was tried
   and measured **within noise** (the cost is the LU + neighbour lookups, not the
   small-Vec allocations), so it was reverted — do not re-attempt allocation
   tuning here without a flamegraph showing allocation as the dominant frame.
5. **`merge_components_local` `O(C²)` (multi-component frames).** *Evidence:* ≈0
   on clean grids but `O(C²×8)` + per-iteration `HashMap` clones. *Approach:*
   drop the per-iteration clones; prune the transform/component search. *Risk:*
   preserve the `min(i,j) → (0,0)` rebase and never introduce a false merge.
6. **Orientation-free positions-only grid path.** *Evidence:* `square_positions`
   ≈8.5× `square_oriented2` in `detect_grid_all`. *Approach:* profile the
   positions-only cell-test / clustering cost and cut the constant. *Risk:*
   correctness-neutral (perf only).
