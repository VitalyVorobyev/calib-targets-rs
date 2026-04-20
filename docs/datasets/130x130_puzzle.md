# 130x130_puzzle regression dataset

**Private data. Same disclosure rules as `3536119669`** — never cite
the dataset name, raw counts, or per-frame identifiers in READMEs,
`book/src/`, `CHANGELOG.md`, rustdoc, Python-package docstrings,
commit messages on `main`, or PR descriptions. General performance
statements only in those surfaces. Concrete numbers are fine in this
file, in CLAUDE.md, and under `bench_results/`.

## Layout

`privatedata/130x130_puzzle/` (co-located with `privatedata/3536119669/`).

- 20 × `target_{0..19}.png` — each 4320×540 = 6 horizontally-stacked
  720×540 snaps (same convention as `3536119669`).
- 20 × `laser_{0..19}.png` — companion laser-stripe snaps, reserved
  for a downstream calibration step (not used for detection).
- `poses.json` — per-image `tcp2base` transform with `type:
  "double_snap"`.
- Total: **120 detection snaps**.

## Board geometry

- `rows = cols = 130` squares (inner-corner grid is 129×129).
- `cell_size_mm = 1.014`.
- `origin_row = origin_col = 0` on the 501×501 master PuzzleBoard.

## Required preprocessing

`--upscale 2` (minimum). A 1.014 mm cell gives ≲ 2 native pixels per
cell, which makes the ChESS corner detector underfire *and* starves
the puzzleboard edge-bit sampling disk (radius ≈ 1/6 × edge-length,
i.e. ≈ 0.3 native pixels — noise-level confidence).

## Baseline (2026-04-20, `--upscale 2`, naive decoder)

Source: `bench_results/130x130_puzzle/phase3/summary.json`.

| Metric | Value |
|---|---|
| Detections | **119/120** (99.17%) |
| Only failure | `edge_sampling / NotEnoughEdges` on `t11s2` (strong reflection, ~28 chess corners survive) |
| max BER across 119 successes | **0.0101** (median 0, p90 0) |
| edges_matched / edges_observed | equal on almost every frame |
| mean_confidence range | 0.42 – 0.55 (median 0.50) |
| labelled corners per success | min 61, median 385, max 575 |
| Per-stage median ms (1440×1080) | corners 3.9, chessboard 1.1, puzzleboard 3-config sweep 12.9 |
| Winning sweep config histogram | cfg0 = 84, cfg2 = 29, cfg1 = 6 |

User target was 116/120 with zero wrong labels — naive decoder
clears it by 3 snaps.

## Precision contract

Wrong master `(i, j)` labels are unrecoverable (they corrupt
calibration); missing corners are acceptable. Any change that raises
max BER above ~0.01, or introduces a failure variant other than
`edge_sampling / NotEnoughEdges`, is a regression.

## Decoder-algorithm decision

Current puzzleboard decode is the "naive" form:

- per-edge hard-bit decision plus a `[0, 1]` confidence weight
  (`edge_sampling.rs:24-73`),
- exhaustive 501² × D4 master-origin sweep with cyclic-class
  precompute (`decode.rs:222-367` + `69-208`),
- lexicographic `(edges_matched, mean_conf)` ranking
  (`decode.rs:380-395`),
- hard `bit_error_rate ≤ max_bit_error_rate` acceptance gate
  (`decode.rs:177`),
- multi-component conflict → `InconsistentPosition`
  (`pipeline.rs:96-114`).

A ChArUco-style coherent-hypothesis rewrite — soft per-bit
log-likelihoods (like `calib-targets-charuco/src/detector/board_match.rs`),
joint-likelihood scoring over (rotation, tx, ty), best-vs-runner-up
margin gate — was compared in detail on 2026-04-20 and **deferred**.
The naive decoder already clears the precision and recall targets on
this dataset; do not pre-emptively rewrite without a dataset that
demonstrates a concrete precision gap. See
`memory/feedback_puzzleboard_decoder_is_good_enough.md`.

## Harness commands

```bash
# End-to-end puzzleboard sweep (emits per-snap PuzzleboardFrameReport
# JSON + summary.json aggregate).
cargo run --release -p calib-targets-puzzleboard \
    --example run_dataset --features dataset -- \
    --dataset privatedata/130x130_puzzle \
    --out     bench_results/130x130_puzzle/phase3 \
    --upscale 2 --rows 130 --cols 130 --cell-size-mm 1.014

# Puzzleboard overlay — master-(row, col) gradient on labelled corners.
uv run python crates/calib-targets-py/examples/overlay_puzzleboard_dataset.py \
    --dataset privatedata/130x130_puzzle \
    --frames  bench_results/130x130_puzzle/phase3 \
    --out     bench_results/130x130_puzzle/phase3/png

# Phase 1/2 corner-cloud and labelled-grid overlays from the
# chessboard-only runner JSON (no detection re-run in Python).
cargo run --release -p calib-targets-chessboard \
    --example run_dataset --features dataset -- \
    --dataset privatedata/130x130_puzzle \
    --out     bench_results/130x130_puzzle/phase1 \
    --upscale 2
uv run python crates/calib-targets-py/examples/overlay_chessboard_corners.py \
    --dataset privatedata/130x130_puzzle \
    --frames  bench_results/130x130_puzzle/phase1 \
    --out     bench_results/130x130_puzzle/phase1/png
uv run python crates/calib-targets-py/examples/overlay_chessboard_grid.py \
    --dataset privatedata/130x130_puzzle \
    --frames  bench_results/130x130_puzzle/phase1 \
    --out     bench_results/130x130_puzzle/phase2/png

# Stage-isolation criterion benches (silently skip when the private
# dataset is absent; override default path via
# CALIB_PUZZLE_PRIVATE_DATASET).
cargo bench -p calib-targets-chessboard  --bench dataset_corners
cargo bench -p calib-targets-chessboard  --bench dataset_chessboard
cargo bench -p calib-targets-puzzleboard --bench dataset_decode
```
