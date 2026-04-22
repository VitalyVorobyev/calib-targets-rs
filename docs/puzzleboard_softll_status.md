# PuzzleBoard Search/Scoring Status

_Last updated: 2026-04-21, branch `better_puzzles`_

## TL;DR

PuzzleBoard now ships two decode axes that are exposed consistently across
Rust, Python, WASM, the dataset runner, and the C ABI:

- `PuzzleBoardSearchMode::{Full, FixedBoard}`
- `PuzzleBoardScoringMode::{HardWeighted, SoftLogLikelihood}`

The soft scorer adds useful diagnostics (`score_best`, `score_runner_up`,
`score_margin`, runner-up origin / transform), but the decisive fix for the
real `130x130_puzzle` dataset was the D4-aware edge-lookup rewrite in the
decoder. That change removes the large axis-aligned master-alias jumps that
previously pushed different camera views of the same target into different
board-frame quadrants.

## Supported features

- `Full` search for unknown printed sub-rectangles of the 501×501 master.
- `FixedBoard` search for a known printed board while preserving
  partial-view correctness.
- `SoftLogLikelihood` scoring with a best-vs-runner-up margin gate.
- `HardWeighted` scoring as a legacy diagnostic / fallback mode.
- Per-frame decode diagnostics in `PuzzleBoardDecodeInfo`:
  - `scoring_mode`
  - `score_best`
  - `score_runner_up`
  - `score_margin`
  - `runner_up_origin_row` / `runner_up_origin_col`
  - `runner_up_transform`
- Dataset-runner flags:
  - `--search-mode full|fixed-board`
  - `--scoring-mode hard|soft`

## What changed

1. The decoder no longer applies the edge lookup shift in the wrong D4
   frame for sign-negating transforms.
2. Fixed-board origin recovery now reports the physical board placement
   directly instead of a CRT-selected master alias.
3. Full and fixed-board decode paths share the same transformed edge-lookup
   convention.
4. The public binding layers now expose the same PuzzleBoard config/result
   schema:
   - Rust crate
   - Python dataclasses + JSON helpers
   - WASM plain-object schema
   - C ABI structs / generated header

## Current dataset status: `130x130_puzzle`, `upscale=2`, `FixedBoard`, `hard`

- Detection rate: `119 / 120`
- Failure class: one `edge_sampling::NotEnoughEdges`
- BER median: `0`
- The previous `~350 mm` horizontal / vertical quadrant split is gone on the
  representative failing target (`target_00`).
- Sequential adjacent-camera shared-ID overlap improved from `4530` to
  `9400` shared corners across the dataset.
- Targets whose 6 ring-neighbour pairs all overlap in a shared board frame:
  `16 / 20` (up from `1 / 20` before the decoder fix).

## Interpretation

The remaining failures are not the old master-alias bug. The four targets
that still fail ring-overlap checks are driven by weak narrow views with low
corner support, not by class-dependent board-frame jumps. On the new run,
whenever two snaps share decoded corner IDs, their `target_position` values
are byte-identical.

That means the remaining gap is now about observation coverage / confidence,
not inconsistent coordinate recovery.

## Useful tooling

- `crates/calib-targets-puzzleboard/examples/run_dataset.rs`
  - emits per-snap JSON with search/scoring selection
- `crates/calib-targets-py/examples/overlay_puzzleboard_board_frame.py`
  - visual board-frame consistency overlays
- `crates/calib-targets-py/examples/inspect_puzzleboard_snap.py`
  - single-snap edge-confidence inspection
- `crates/calib-targets-py/examples/export_calibration_pairs.py`
  - calibration-pair export with PuzzleBoard diagnostics

## Remaining work

- Improve weak-view coverage on the four residual inconsistent targets.
- Keep using `score_margin` and BER as downstream trust signals.
- Preserve parity across Rust / Python / WASM / FFI whenever the PuzzleBoard
  result schema grows again.
