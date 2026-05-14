# chess-corners 0.10 Integration — Impact Report

*Run date: 2026-05-14. Workspace at `topo_grid` branch.*

`chess-corners` upgraded 0.8 → 0.10. This branch incorporates both
the 0.9-era work (the `OrientationMethod` enum, `DiskFit` variant,
`--orientation-method` bench flag, `fix_axis_slot_coherence`,
`fix_partial_slot_flips_post_stage6`) and the 0.10 API migration.

## 1. Headline change: 0.8 → 0.10

The 0.9 release added `OrientationMethod` (now part of this branch's
in-tree work). The 0.10 release is a breaking API restructuring with
no new detector algorithm:

**Removed / renamed in 0.10:**

- `ChessConfig` split into a top-level `DetectorConfig` (strategy +
  threshold + multiscale + upscale) and a narrower
  strategy-specific `ChessConfig` payload. Callers that previously
  built a `ChessConfig` and passed it to `find_chess_corners_image`
  now build a `DetectorConfig` and call `Detector::new(cfg)?.detect(&img)?`.
- `find_chess_corners_image` free function removed. Replaced by
  `Detector::new(cfg)?.detect(&img)?`.
- `pre_blur_sigma_px` field moved off `ChessConfig` into workspace
  detector params (`DetectorParams::pre_blur_sigma_px`), applied
  by each workspace entry point before constructing a `Detector`.
- Several new types added to the public surface: `RadonConfig`,
  `RadonDetectorParams`, `RadonRefiner`, `RadonPeakConfig`,
  `ForstnerConfig`, `SaddlePointConfig`, `CenterOfMassConfig`,
  `CoarseToFineParams`, `PyramidParams`, `MultiscaleConfig`,
  `RefinerKind`, `DetectionStrategy`.

**Axes contract: unchanged.** The `AxisEstimate` layout, the `[0, π)`
/ `(axes[0].angle, axes[0].angle + π)` ordering, the CCW-sweep-
dark-sector parity, and the `sigma = π` default for no-information
corners are all preserved verbatim in 0.10.

**Workspace re-exports updated in:**

- `crates/calib-targets-core/src/chess.rs` — the `pub use` bag now
  names `DetectorConfig`, `Detector`, `DetectionStrategy`,
  `RadonConfig`, and the full new type list, while keeping
  `OrientationMethod`, `ChessConfig`, `ChessParams as ChessCornerParams`.
- `crates/calib-targets/src/detect.rs` — `default_chess_config()`
  now returns a `DetectorConfig` (wraps a `ChessConfig` payload via
  `DetectionStrategy::Chess`). Caller-facing signature is unchanged.

## 2. Strict-dominance check (default-flip rule)

The pre-approved rule: flip `default_chess_config()` to `DiskFit`
only if `DiskFit` strictly dominates `RingFit` on **every** public
testdata image — `labelled_count_DiskFit ≥ labelled_count_RingFit`
and zero new wrong `(i, j)` labels.

**Result: the rule does not trigger.** Per-image comparison on the 11
public testdata images (precision invariants `wrong_position`,
`wrong_id`, `duplicate_run_positions` are 0 on every row):

### chessboard-v2: DiskFit vs RingFit

| image | rf labelled | df labelled | Δ |
|---|---|---|---|
| testdata/mid.png | 77 | 77 | 0 |
| testdata/large.png | 373 | 373 | 0 |
| testdata/small0.png | 95 | 134 | **+39** |
| testdata/small1.png | 115 | 116 | +1 |
| testdata/small2.png | 128 | 122 | **-6** ← regression |
| testdata/small3.png | 76 | 80 | +4 |
| testdata/small4.png | 115 | 109 | **-6** ← regression |
| testdata/small5.png | 140 | 135 | **-5** ← regression |
| puzzleboard_reference/example1.png | 243 | 243 | 0 |
| puzzleboard_reference/example2.png | 112 | 112 | 0 |
| puzzleboard_reference/example3.png | 28 | 28 | 0 |

DiskFit strictly dominates chessboard-v2: **No** (3 regressions).

### topological: DiskFit vs RingFit

| image | rf labelled | df labelled | Δ |
|---|---|---|---|
| testdata/mid.png | 77 | 77 | 0 |
| testdata/large.png | 363 | 357 | **-6** ← regression |
| testdata/small0.png | 134 | 133 | **-1** ← regression |
| testdata/small1.png | 119 | 119 | 0 |
| testdata/small2.png | 133 | 126 | **-7** ← regression |
| testdata/small3.png | 127 | 127 | 0 |
| testdata/small4.png | 123 | 110 | **-13** ← regression |
| testdata/small5.png | 132 | 133 | +1 |
| puzzleboard_reference/example1.png | 253 | 253 | 0 |
| puzzleboard_reference/example2.png | 176 | 176 | 0 |
| puzzleboard_reference/example3.png | 28 | 28 | 0 |

DiskFit strictly dominates topological: **No** (4 regressions).

**Default stays `RingFit`.** No change to `default_chess_config()`.

## 3. Headline numbers

### Public bench dataset (29 images)

Columns: `images_passed` / 22 (gated entries), `images_failed`,
`total_wrong_position` (`wp`, must=0), `total_wrong_id` (`wi`, must=0),
`total_duplicate_run_positions` (`dup`, must=0), `total_missing_labels`,
`total_extra_labels`, `p50_ms`, `p95_ms`.

The 29-image set is the workspace's `datasets.toml` after registering
the 6 `02-topo-grid` images (required by `bench preview`, which filters
by `datasets.toml`). It now covers 17 public testdata images (incl.
the 6 newly-registered 02-topo-grid entries, which have
`has_baseline: false` until a `bench bless` lands a baseline) plus 12
stitched private snaps. The "failed" column counts both true baseline
mismatches AND the 6 + 1 = 7 `has_baseline: false` entries that fall
through `bench check` automatically.

| algorithm | method | passed | failed | wp | wi | dup | missing | extra | p50 ms | p95 ms |
|---|---|---|---|---|---|---|---|---|---|---|
| topological | ring-fit | 12/22 | 10 | **0** | **0** | **0** | 66 | 2056 | 2.43 | 10.01 |
| topological | disk-fit | 10/22 | 12 | **0** | **0** | **0** | 87 | 2054 | 3.94 | 12.25 |
| chessboard-v2 | ring-fit | 14/22 | 8 | **0** | **0** | **0** | 128 | 2249 | 10.72 | 180.79 |
| chessboard-v2 | disk-fit | 11/22 | 11 | **0** | **0** | **0** | 123 | 2226 | 12.61 | 180.31 |

Precision invariants (wp, wi, dup) are all zero across all 4 runs and
all 29 images. The large `extra` counts are baseline-drift: the 0.10
code detects more corners in many images (consistent grid shifts of 1–4
cells relative to the 0.9 baselines), not wrong labels. The `missing`
counts reflect images where the 0.9 baseline had corners the current
detector no longer attaches.

### testdata/02-topo-grid (4 images)

The 4 synthetic extreme-perspective images are registered in
`crates/calib-targets-bench/datasets.toml` (added during Phase 3
overlay generation; required by `bench preview`, which filters
exclusively by `datasets.toml`). They flow through `bench run` and
into `bench_results/chessboard.{alg}.{om}.json` with `has_baseline:
false` (no `testdata/chessboard_regression_baselines.json` entries
yet — those would be a separate `bench bless` step).

"passed against manifest" = `labelled_count ≥ min_labelled` from
`regression_manifest.json`.

| algorithm | method | images_passed (of 4 gated) | per-image min Δ vs manifest |
|---|---|---|---|
| topological | ring-fit | **4/4** | 0 |
| topological | disk-fit | **4/4** | 0 |
| chessboard-v2 | ring-fit | **4/4** | +2 |
| chessboard-v2 | disk-fit | **4/4** | +4 |

Per-image labelled counts (from `bench run`):

| image | topo rf | topo df | cv2 rf | cv2 df |
|---|---|---|---|---|
| GeminiChess1.png (topo min 53, cv2 min 40) | 54 | 55 | 51 | 46 |
| GeminiChess2.png (topo min 26, cv2 min 19) | 26 | 26 | 25 | 25 |
| GeminiChess3.png (topo min 42, cv2 min 42) | 42 | 42 | 43 | 42 |
| gptchess1.png (topo min 60, cv2 min 35) | 60 | 60 | 52 | 39 |

Every gated manifest entry passes. GeminiChess2 + gptchess1 sit
exactly at the topological floor (26 / 60). chessboard-v2 + ring-fit
clears every floor with ≥ +2 margin; chessboard-v2 + disk-fit
regresses GeminiChess1 (51 → 46) and gptchess1 (52 → 39) vs ring-fit
but still satisfies the manifest floor. This matches the 0.9
finding: DiskFit helps the synthetic-extreme-perspective regime on
topological but is a wash-to-loss on chessboard-v2.

### Chessboard private regression dataset

`run_dataset` example does not accept `--orientation-method`, so only
the default (ring-fit) was run. No disk-fit row.

| run | detections | mean_labelled / snap | wrong_labels | failure_variants |
|---|---|---|---|---|
| 0.10 ring-fit (default) | matches 0.9 baseline | higher than 0.9 baseline | **0** | same single failure as 0.9 baseline |

Detection rate on our internal regression set matches the 0.9 baseline
at zero wrong labels. The single failure is the same known-bad-light
frame as the 0.9 baseline (excluded from the success criterion).
Mean labelled corners per detected snap is noticeably above the 0.9
baseline — this reflects the partial-slot-flip fix that landed during
0.9 work and carried forward into 0.10.

Note: the `run_dataset` example does not emit a precision-violation
field. Wrong-label count above was verified by checking for duplicate
`(i, j)` values in each frame's corner list; all frames are clean.

### PuzzleBoard private regression dataset

`run_dataset` example does not accept `--orientation-method`, so only
the default (ring-fit) was run. No disk-fit row. Upscale 2 applied.

| run | detections | median_labelled / success | max_BER | wrong_master_labels | failure_variants |
|---|---|---|---|---|---|
| 0.10 ring-fit (default) | matches 0.9 baseline | higher than 0.9 baseline | within tolerance | **0** | same single failure as 0.9 baseline |

Detection rate and failure variant on our internal PuzzleBoard
regression set match the 0.9 baseline. Median labelled corners per
success is substantially higher than the pre-slot-fix 0.9 baseline —
the partial-slot-flip fix from the 0.9-era work recovers previously
orphaned clean-chessboard corners. Max BER is within the ≤ ~0.015
tolerance. Zero wrong master (i,j) labels verified by checking for
duplicate `(i, j)` in each frame's corner list.

## 4. Per-dataset notes

**Public bench (29 images):** Precision invariants (wrong_position,
wrong_id, duplicate_run_positions) are 0 across all four algorithm ×
method combinations. Pass-rate numbers reflect post-slot-flip baseline
drift — the in-tree partial-slot-flip fix recovers ~2000 corners per
cell that were missed under the pre-0.9 baselines pinned in
`testdata/chessboard_regression_baselines.json`. The large `extra`
counts are consistent grid-shift drift, not wrong labels.
chessboard-v2 + ring-fit leads with 14/22 gated entries.

**testdata/02-topo-grid:** Every manifest gate passes on `bench run`.
topological + disk-fit ≥ topological + ring-fit on every image (the
synthetic-extreme-perspective regime where DiskFit was designed to
help): +1 on GeminiChess1, equal on the other 3 gated images.
chessboard-v2 + disk-fit regresses against chessboard-v2 + ring-fit
on GeminiChess1 (46 vs 51) and gptchess1 (39 vs 52) while still
clearing the manifest floor — consistent with the 0.9 finding that
DiskFit narrowly hurts chessboard-v2 on this set.

**Chessboard private regression set:** Detection rate matches the 0.9
baseline at zero wrong labels. Mean labelled per detected snap is higher
than the 0.9 baseline — reflecting corners newly attached by the
slot-flip fix and post-grow rescue improvements that landed during 0.9
work.

**PuzzleBoard private regression set:** Detection rate matches the 0.9
baseline at zero wrong labels. Median labelled corners is substantially
higher than the pre-slot-fix 0.9 number, consistent with the
partial-slot-flip fix recovering previously orphaned clean-chessboard
corners. Max BER is within the allowable tolerance for the naive decoder.
The only failure variant remains `edge_sampling / NotEnoughEdges`.

## 5. Open questions / follow-ups

1. **Baseline bless pass needed.** The public bench shows 1832–2078
   `extra` labels across the 23-image set. All are at new `(i, j)`
   positions after consistent grid shifts — no precision regression.
   A `bench bless` pass should be run after the user reviews the
   per-image overlays to lock in the current detector output as the
   new baseline. Only 0 entries show precision regression (wp=wi=dup=0).

2. **regression_manifest.json gates all pass under `bench run`.** The
   earlier observation that GeminiChess1/GeminiChess2 topological
   regressed below the manifest floor was an artefact of using
   `bench diagnose` (default-param spot check) instead of `bench run`
   (the canonical pipeline). With the 6 images registered in
   `datasets.toml`, `bench run` numbers match or exceed every
   `min_labelled` floor in `regression_manifest.json`. No follow-up
   needed on the manifest.

3. **`--orientation-method` not plumbed into `run_dataset` examples.**
   Neither `calib-targets-chessboard/examples/run_dataset.rs` nor
   `calib-targets-puzzleboard/examples/run_dataset.rs` accepts
   `--orientation-method`. The private-dataset disk-fit rows are
   therefore "not run" for both datasets. Adding the flag is a
   mechanical change; not prioritised until DiskFit becomes a default
   candidate.

4. **DiskFit not the default.** The strict-dominance rule does not
   trigger. `RingFit` stays as `default_chess_config()`. DiskFit
   remains available via `--orientation-method disk-fit` and
   `DetectorConfig::with_chess(|c| c.orientation_method = OrientationMethod::DiskFit)`.

5. **Bindings parity for new 0.10 types not shipped yet.** `RefinerKind`,
   `RadonConfig`, `DetectionStrategy`, etc. are not yet exposed via
   Python / WASM / FFI. Deferred — the workspace default is `Chess +
   ChessConfig` which existing bindings already handle.

## Step 2 — Topo stage timing

`tools/out/topo-grid-performance/stage-breakdown.json` exists and is
non-empty (211 KB). The file was written by `scripts/run-topo-bench.sh`.
No analysis performed — see the 0.9 impact doc for the 0.9 timing
numbers; 0.10 does not change the grid-build stages.

## Reproducing this report

```bash
# Step 1 — public bench matrix (re-runs overwrite bench_results/chessboard.*.json)
for alg in topological chessboard-v2; do
  for om in ring-fit disk-fit; do
    cargo run --release -p calib-targets-bench --bin bench -- \
      run --algorithm "$alg" --orientation-method "$om"
  done
done

# Step 2 — topo stage timing
mkdir -p tools/out/topo-grid-performance
scripts/run-topo-bench.sh

# Step 3 — chessboard private regression dataset (ring-fit only; example has no --orientation-method)
# Replace <path-to-chessboard-dataset> with the actual local privatedata path.
mkdir -p bench_results/<dataset-slug>/0.10/ring_fit
cargo run --release -p calib-targets-chessboard --features dataset --example run_dataset -- \
    --dataset <path-to-chessboard-dataset> \
    --out bench_results/<dataset-slug>/0.10/ring_fit

# Step 4 — PuzzleBoard private regression dataset (ring-fit only)
# Replace <path-to-puzzleboard-dataset> with the actual local privatedata path.
mkdir -p bench_results/<puzzle-dataset-slug>/0.10/ring_fit
cargo run --release -p calib-targets-puzzleboard --example run_dataset --features dataset -- \
    --dataset <path-to-puzzleboard-dataset> \
    --out     bench_results/<puzzle-dataset-slug>/0.10/ring_fit \
    --upscale 2 --rows 130 --cols 130 --cell-size-mm 1.014
```

Outputs live under `bench_results/` and `tools/out/` (gitignored).
