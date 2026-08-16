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

Three tiers, in order of cost. The headline, **as of the ChArUco decode rewrite
(PR #71)**: the external ChESS corner detector dominates the plain-chessboard and
large ChArUco frames, where decode used to be the largest stage. That lead isn't
universal: on the smaller, marker-dense ChArUco frame the three stages are
close, with decode now narrowly ahead; and the PuzzleBoard frame is a clearer
exception, where the balance depends on the mode and declared board size (see
Tier 2). The largest *owned* costs are the dense-board grid build and, on
marker-dense boards, decode; the once-dominant ChArUco decode has dropped to a
minor stage on the high-resolution frame.

Per-stage p50 on the four public report frames (`full_stage_timing`, M4 Pro,
100 reps — the same numbers the published report renders):

| Frame | px | corner detect | grid build | decode | end-to-end |
|---|---|---:|---:|---:|---:|
| `mid.png` (chessboard) | 1024×576 | **0.86** | 0.33 | — | 1.19 |
| `small.png` (ChArUco) | 720×540 | 0.89 | 0.70 | **0.95** | 2.53 |
| `oblique.png` (PuzzleBoard) | 640×480 | 0.68 | **1.62** | 1.03 | 3.33 |
| `large.png` (ChArUco) | 2048×1536 | **4.51** | 3.39 | 2.23 | 10.13 |

Corner detection leads on the chessboard and the large ChArUco frame. On the
small ChArUco frame the three stages are close, with decode (0.95 ms) narrowly
ahead of corner detection (0.89 ms) and grid build (0.70 ms) — a marker-dense
board at this size keeps per-cell decode competitive even after #71. On the
large ChArUco frame decode is the smallest stage, as the #71 rewrite intended.
On the PuzzleBoard row — where the master sweep used to dominate at 4.6 ms —
decode has fallen further still and that row spends more in the grid build than
in decode. The PuzzleBoard frame's oblique, corner-dense 640×480 image drives an
outsized grid build for its size (see Tier 3).

The `small.png` grid-build and decode figures rose slightly when ChArUco stopped
disabling the chessboard's wrong-label geometry check (#86). That is a
correctness cost, not a regression to recover.

### Tier 1 — ChESS corner detection (external `chess-corners`)

Corner detection is **the largest stage on half the public frames** — the plain
chessboard (~72 % of its end-to-end) and the large ChArUco frame. It scales with
image area (`large.png`, 3 MP, ≈4.5 ms; the ~0.4–1 MP frames ≈0.7–0.9 ms). It no
longer leads on the other two: on the small ChArUco frame decode has edged
narrowly ahead instead (see Tier 2), and on the PuzzleBoard frame the owned grid
build has caught up with and passed it (see Tier 3). The `disk-fit` orientation
method roughly **doubles** corner-detection cost vs `ring-fit` — the standing
reason `RingFit` is the default.

We *tune* this stage (resolution, ROI, orientation method) but do not own the
implementation, so the levers are configuration, not code. It remains the
highest-leverage target on the frames where it leads — the largest single figure
of any stage on any frame (`large.png`, ≈4.5 ms) and the majority of a
plain-chessboard's budget; on marker-dense or dense-corner boards the owned
stages (Tier 2 decode, Tier 3 grid build) now matter more.

### Tier 2 — marker-decode sweeps (our code)

- **PuzzleBoard decode.** All four paths share one cyclic class precompute
  (`decode/tables.rs`), `O(501 · N)` per *searched* transform — by default the
  four 90° rotations (`PuzzleBoardSymmetryMode::Rotations`), or all eight
  dihedral transforms when `RotationsAndReflections` is opted in for a
  handedness-flipping optical path (mirror, beam splitter). An ordinary camera
  imaging an opaque printed board cannot see a mirrored view, so the four
  reflections in the old default were physically unreachable; the restricted
  default search is both faster and more unique (fewer aliases means fewer
  ambiguous-decode rejections). Let `T` be the number of searched transforms (4
  by default, 8 under `RotationsAndReflections`). On top of the shared
  precompute:
  - `Full × Hard` collapses the 501² origin argmax to `O(501)` by crossed-CRT
    separation → `O(T · 501 · N)`.
  - `Full × Soft` cannot (an `f32` key breaks the separation's
    strictly-below-the-max step), so it keeps the 501² walk stripped to two
    table reads and a compare → `O(T · (501 · N + 501²))`.
  - `FixedBoard` (either scorer) is the same problem restricted to the declared
    board's origin rectangle, over only the residue classes that rectangle
    reaches → `O(T · (reachable · N + L_r · L_c))`, strictly below the full
    search until the board spans the maps' 167-long period.

  Measure with `decode::tests::decode_scaling_report` (decode only, no image
  pipeline) and `puzzleboard_stage_timing` (per-stage, on a public fixture).
  Where the time sits depends on the mode and the *declared board size* — on a
  public 20×20 fixture, decode is ~61 % of `detect` under `Full × Soft` but only
  ~5 % under `FixedBoard`; declaring a 130×130 board puts it back at ~46 %, and
  a full 501×501 board at ~75 %. Restricting the default search from eight
  transforms to four roughly **halves** the isolated transform-search cost, but
  the published per-stage `decode_ms` also includes edge-dot sampling, which the
  transform count doesn't touch — so the measured drop on the report fixture is
  well short of 2×: 1.26 → 1.03 ms on `oblique.png` (≈18 %).

  **Inside the precompute, the observations are nearly free.** `build` splits
  into `fill` (bucket the observations by residue, `O(N)`) and `class_credit`
  (credit each bucket into the classes it reaches, `O(buckets · 501)`), timed
  separately by `puzzleboard_stage_timing`. On `oblique.png`: `build` 0.385 ms,
  of which `fill` is **0.023 ms** and `class_credit` **0.360 ms** — 94 %. Adding
  observations therefore costs almost nothing; adding *residue buckets* costs
  501 cells each.

  What `class_credit` computes is a **cyclic cross-correlation** of the
  residue histogram against the map: for the H table,
  `H[a][b] = Σ_{row,col} S[row][col] · M[(a+row) mod 167][(b+col) mod 3]`,
  which is why it is `O(buckets · 501)` and why the observation count drops out.
  Two measured levers follow from that shape (`oblique.png`, `Full × Soft`:
  16 `build` calls per `detect` = 2 components × 4 transforms × 2 views, each
  with 51 H and 51 V residues, so ≈818 k cell visits at ≈0.44 ns each):

  - **Fuse the two views.** The physical and voted observation sets have the
    *same* residues — measured 51/51 in both, which is exactly what period-3
    voting guarantees — so the two sweeps walk an identical
    `(class, map cell)` sequence and differ only in the values added. One
    traversal emitting both accumulator sets is worth **2×**.
  - **Amortise the 3-wide axis.** The 51 H residues are 17 rows × 3 column
    residues, and `M[r][·]` is one of only 8 three-bit patterns. Precomputing
    `T[row][pattern][b]` turns three 501-cell sweeps per row into one 167-cell
    sweep emitting 3 values, worth a further **≈3×**. The V table is the mirror
    image (short axis on rows, patterns over `map_a` columns).

  Together ≈6×: `class_credit` 0.360 → ≈0.06 ms, about 10 % of `detect` here
  and considerably more in the regimes where decode dominates. Both are exact
  rearrangements, and the current implementation is a byte-exact oracle to test
  a rewrite against. Neither is implemented.

  **Refuted, do not retry:** merging the bit-0 and bit-1 bucket at one residue
  into a single signed sweep. It is a valid identity but buys nothing —
  instrumenting `fill` across all three public fixtures found **zero** residues
  carrying both bits. Period-3 redundancy is why: every observation at one
  residue is a replica of the same code bit, so the two buckets coexist only
  where a dot was misread, and the authors' own photographs decode at BER 0.
- **ChArUco board match — minor on the large frame, competitive on the small one
  (PR #71 closed the old bottleneck).** Precomputing a per-cell
  bit-log-likelihood table removed the `O(cells × markers × 4 × bits²)`
  `log_sigmoid` evaluations from the hypothesis-scoring inner loop: the
  board-level matcher dropped ~13×. On the public report frames decode is now
  **0.95 ms (`small.png`)** and **2.23 ms (`large.png`)**. On `large.png` that is
  below both corner detection and grid build; on the smaller, marker-dense
  `small.png` the three stages are close and decode is narrowly the largest
  (see the per-stage table above) — the expected shape for a board this size,
  not a regression. It is no longer a top owned cost on the large frame and is
  **not** a current optimization target.

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

The same hot spot shows on the **public report PuzzleBoard frame** at a fraction
of the resolution. `oblique.png` (640×480, 361 corners, *single
component*): grid build ≈1.73 ms, of which `ordering` alone is ≈0.91 ms (**52 %**)
and `recovery` ≈0.10 ms. The smaller `example2.png` (same 640×480, 180 corners):
grid build ≈1.07 ms, `ordering` 0.48 ms (**45 %**), `recovery` 0.20 ms (19 %). So
the local-H gate dominates grid build even on small distorted boards, not just
12 MP frames — and because both frames are single-component,
`merge_components_local` is ≈0 on them: the elevated grid build is the local-H
solve, *not* the merge.

Two further caveats:

- **`merge_components_local`** reads as ≈0 above because these frames form one
  component. Structurally it is `O(C² × 8 transforms)`, so it grows on
  **multi-component** (distorted / occluded) frames — which the single-component
  timing under-represents. The per-merge full-`HashMap` clone in its fixed-point
  loop has been removed (a `mem::take` of each just-killed component's map;
  byte-exact, `bench check` green on both regression sets) — backlog item 5.
- **Orientation-free grid build is ~8.5× the oriented path.** Synthetic
  `detect_grid_all`: `square_positions` (positions-only evidence) ≈4.5 ms vs
  `square_oriented2` ≈0.53 ms (and `hex_positions` ≈0.19 ms). The positions-only
  path matters for the orientation-free standalone use of `projective-grid`.

## Optimization backlog

Prioritized by measured impact, **re-ranked after PR #71** (ChArUco decode
rewrite). **Every item is correctness-first: none may trade a false-positive
risk for speed** — a wrong `(i, j)` label is unrecoverable for calibration (the
asymmetric detection contract). Optimization work is *planned* here, not yet
applied. With ChArUco decode now a minor stage on the high-resolution frame
(though still competitive on smaller marker-dense boards, see Tier 2), the two
live owned candidates are the dense-board grid build and edge sampling; the
external corner detector still leads on the sparse and high-resolution frames.

1. **Corner-detection configuration levers (Tier 1, highest leverage).**
   *Evidence (refreshed):* the largest stage on the plain-chessboard frame
   (~72 % of its end-to-end) and on the 3 MP frame (≈4.5 ms); `disk-fit`
   ≈2× `ring-fit`. On the other two public frames an owned stage now leads
   instead — decode on the small ChArUco frame, grid build on the PuzzleBoard
   frame. *Approach:* keep `RingFit` default; offer optional downscale
   for large frames and ROI when a board prior exists. *Risk:* downscale trades
   corner-localization precision — validate recall/precision, never silently.
2. **PuzzleBoard decode — largely done.**
   *Evidence:* `puzzleboard_stage_timing` and `decode::tests::decode_scaling_report`.
   *Done:* the declared-board search became a restricted master search rather
   than a separate correlation; the class precompute is now bounded by residue
   groups rather than observation count (`O(N + min(N, 6w) · 501)`, flat in `N`);
   and the soft scorer's log-likelihood moved to fixed point, which is what makes
   the crossed-CRT separation exact and collapsed its `501²` origin walk to
   `O(501)`. Default `Full + Soft` detect on the report fixture: 6.32 → 2.98 ms.
   *Remaining:* only the declared-full-master case, where decode was ~70 % of
   `detect` under the old eight-transform default — the origin-rectangle
   readout, now `4 × 252k` under the four-rotation default (`8 × 252k` under
   `RotationsAndReflections`), which none of the three changes reach because
   there is nothing left to restrict and the CRT collapse does not apply to a
   sub-rectangle. Halving the default transform count should shrink that share
   too, though it hasn't been remeasured for the full-master case specifically.
   It is pure table lookups and the natural SIMD target, with little practical
   urgency since declaring a full-master board conveys no information. Optional
   `rayon` over the searched transforms; and the same fusion for
   `decode_fixed_board_soft` (the fixed-board shift-scan second pass, left
   untouched this round — different table shape, not a free reuse). *Risk:* the
   workspace has **zero parallelism** in its own code today and a past
   non-determinism bug traced to `HashMap` iteration order — any parallelism
   must keep decode output bit-exact and deterministic.
3. **ChArUco board-match decode — CLOSED (PR #71).** The per-cell
   bit-log-likelihood table removed the hypothesis-scoring inner loop's
   `log_sigmoid` evaluations (~13× faster matcher). Public report decode is now
   0.95 ms (`small.png`) / 2.23 ms (`large.png`) — below corner detection and
   grid build on the large frame, and narrowly the largest of the three on the
   smaller, marker-dense small frame (expected at that board size, see Tier 2).
   No further decode optimization is warranted; reopen only if a future profile
   shows it dominating a frame the way it once did on `large.png`.
4. **Per-corner local-H solve in the precision gate (Tier 3 — the top owned grid
   cost across regimes).** *Evidence:* `ordering` + `recovery` own 60 % of a
   27.5 ms grid build on a 12 MP board; `ordering` alone owns **45–52 %** of the
   ~1–1.9 ms grid build on the small (640×480) public PuzzleBoard/distorted
   frames. All are dominated by the 8×8 LU in `validate → local_h_residual`,
   re-run per grow iteration. *Approach (deferred, TODO):* memoize the
   per-component local-H bases across grow iterations, or reduce the number of
   validation re-runs — **not** a different solver (FP drift). *Risk:* HIGH —
   this is the false-positive gate with documented determinism contracts; any
   change must stay byte-exact on both regression sets, so it is a dedicated
   behaviour-gated PR, not a drive-by. A safe allocation-removal experiment in
   `pick_local_h_base` was tried and measured **within noise** (the cost is the
   LU + neighbour lookups, not the small-Vec allocations), so it was reverted —
   do not re-attempt allocation tuning here without a flamegraph showing
   allocation as the dominant frame.
5. **`merge_components_local` `O(C²)` (multi-component frames).** *Evidence:* ≈0
   on clean single-component grids but `O(C²×8)` on multi-component
   (distorted / occluded) frames. *Done:* the per-merge full-`HashMap` clone in
   the fixed-point loop is gone — replaced by a `mem::take` of each just-killed
   component's map (byte-exact; `bench check` green with `pos=id=dup=0` on both
   regression sets). *Remaining:* prune the transform/component search (changes
   which candidates are considered → **not** byte-exact, needs a behaviour gate).
   *Risk:* preserve the `min(i,j) → (0,0)` rebase and never introduce a false
   merge.
6. **Orientation-free positions-only grid path.** *Evidence:* `square_positions`
   ≈8.5× `square_oriented2` in `detect_grid_all`. *Approach:* profile the
   positions-only cell-test / clustering cost and cut the constant. *Risk:*
   correctness-neutral (perf only).
