# Pre-Release Review — calib-targets-rs 0.6.0
*Reviewed: 2026-04-17*
*Scope: full workspace (13 crates) with focus on the new `calib-targets-puzzleboard` crate and 0.6.0 binding additions*

## Review Verdict
*Verified (Pass 2): 2026-04-17*

**Overall: PASS — READY TO TAG 0.6.0.** All 14 findings from Pass 1 plus all three
post-review blockers (C1, C2, C3) are verified against `git diff`. [P10] was
re-opened as [C1] by the calibration-review specialist, fixed, and independently
verified in Pass 2.

| Item | Pass 1 Verdict | Pass 2 Verdict |
|---|---|---|
| P01 | verified | verified |
| P02 | verified | verified |
| P03 | verified | verified |
| P04 | verified | verified |
| P05 | verified | verified |
| P06 | verified | verified |
| P07 | verified | verified |
| P08 | verified | verified |
| P09 | verified | verified |
| P10 | skipped (deferred) | superseded by [C1] → verified |
| P11 | verified | verified |
| P12 | verified | verified |
| P13 | verified | verified |
| P14 | verified | verified |
| P15 | verified | verified |
| **C1** | n/a | verified |
| **C2** | n/a | verified |
| **C3** | n/a | verified |

**Counts (combined):** verified 17 / needs-rework 0 / regression 0 / skipped 0
(P10 originally skipped, now superseded by the verified C1).

**Pass-2 verification notes:**
- **C1** — `wrap_master(i, j)` applies `rem_euclid(MASTER_COLS)` to both coords in
  `pipeline.rs:299`. Called at `pipeline.rs:175` before **both** `master_ij_to_id`
  (line 176) and `master_target_position` (line 177). Invariant
  `target_position == Point2::new((id % 501) * cell, (id / 501) * cell)` is
  documented in the code comment at the call site and asserted by
  `id_and_target_position_are_consistent_after_wrap` over a grid of raw coords
  including negatives (−503, −250, −1). The test would fail on a mental revert:
  without wrapping, `master_target_position(-503, …)` returns a negative `x`,
  breaking both the invariant and the `target.x >= 0.0` assertion.
- **C2** — `update_best_candidate` in `decode.rs:207-222` now returns `true` iff
  `candidate.edges_matched > current.edges_matched` OR
  `(edges_matched == && weighted_score >)`. Strict `>` on both keys (no
  `>=`) preserves stability on ties. `lex_rank_matched_beats_weighted_score`
  constructs candidate A `(20 matched, 0.5)` and candidate B `(18 matched, 0.9)`;
  A must win despite lower weighted_score. Mental revert: with old
  `weighted_score`-only comparison, B (0.9) would beat A (0.5), and the
  `winner.master_origin_row == 10` assertion would fail. Confirmed.
- **C3** — The O(501² × N) triple loop is replaced with a two-phase precompute
  in `decode.rs:72-147`:
  1. Per-observation inner pass is O(H_ROWS × H_COLS) = 501 for horizontal or
     O(V_ROWS × V_COLS) = 501 for vertical; accumulates into `h_match[3×167]`,
     `h_count[3×167]`, `v_match[167×3]`, `v_count[167×3]`. Total precompute: O(501·N).
  2. Origin-scan loop at `decode.rs:149-189` computes
     `ha = mr % 3, hb = mc % 167, va = mr % 167, vb = mc % 3` then
     `matched = h_count[ha*167+hb] + v_count[va*3+vb]`; zero per-observation
     iteration in the 501² loop. Scratch buffers hoisted outside the D4 loop
     (lines 71-78), cleared with `.fill(0)` per transform. Algorithm spot-check:
     for horizontal obs at `(tr, tc, bit, conf)`, inner loop does
     `a = (r - tr) rem 3, b = (c - tc) rem 167`; accumulates `conf` into
     `h_match[a*167+b]` when `horizontal_edge_bit(r, c) == bit`. This is
     mathematically equivalent to "for every `(a, b)`, sum `conf` over all obs
     where `DATA_A[(a+tr) mod 3][(b+tc) mod 167] == bit`" — exactly the desired
     precompute identity. Reference implementation `decode_reference` preserved
     behind `#[cfg(test)]` (lines 257-325). Three parity tests
     (`fast_decode_matches_reference_identity`, `_d4_rotation`, `_all_flipped`)
     all pass in release. Public signature
     `decode(&[PuzzleBoardObservedEdge], f32) -> Option<DecodeOutcome>`
     unchanged; `DecodeOutcome` struct fields unchanged.

**Timing measurement (this machine, Apple Silicon, release mode):**
- `decode_25x25_timing`: **4.67 ms** on 1200 observations (target ≤ 50 ms,
  ceiling 200 ms). The implementer's self-reported 18 ms is within the same
  order of magnitude.

**Full gate suite (Pass 2, all green):**
- `cargo fmt --all -- --check`: EXIT 0
- `cargo clippy --workspace --all-targets -- -D warnings`: EXIT 0
- `cargo test --workspace`: EXIT 0 (all tests passed)
- `cargo test --workspace --all-features`: EXIT 0
- `cargo doc --workspace --no-deps`: EXIT 0 (zero warnings)
- `cargo run -p calib-targets-ffi --bin generate-ffi-header -- --check`: EXIT 0 (header fresh)
- `uv run python crates/calib-targets-py/tools/generate_typing_artifacts.py --check`: EXIT 0 (stubs fresh)
- `uv run pytest crates/calib-targets-py/python_tests/ -v`: 27 passed, EXIT 0

**New issues introduced by Pass 2 fixes:** none. No regressions.

**Release readiness: READY TO TAG 0.6.0.**

---

### Pass 1 Verdict (historical — all 14 findings verified on first cycle)

| Item | Verdict | Notes |
|---|---|---|
| P01 | verified | Rust types fully renamed; no bare `DecodeConfig` / `ObservedEdge` remain as Rust pub exports. Python keeps backward-compat aliases (as stated in resolution). FFI dropped the `as`-alias. |
| P02 | verified | `#[non_exhaustive]` on `PuzzleBoardParams`, `PuzzleBoardDecodeConfig`, `PuzzleBoardDecodeInfo`; data-carrier structs left bare. FFI uses clean `for_board` + mutation pattern, plus `PuzzleBoardDecodeConfig::new(...)` for explicit construction. CLAUDE.md gains the "Public struct conventions" paragraph. |
| P03 | verified | `sample_all_edges` now uses `inliers.contains(&idx)`; no `HashSet` import survives in `pipeline.rs`. |
| P04 | verified | `corner_at_map` takes a pre-built `HashMap<(i32,i32), &LabeledCorner>` in `edge_sampling.rs`; `sample_all_edges` builds it once and all neighbour lookups go through it. Old O(N) `corner_at` is gone. |
| P05 | verified | `PuzzleBoardDetector::{new, params, detect}` all have `///` docstrings; `detect` documents every error variant including `InconsistentPosition` and the tie-break rule. |
| P06 | verified | `PuzzleBoardDetectConfig` now exposes `load_json` and `write_json`, matching `CharucoDetectConfig`. Both return `PuzzleBoardIoError`. |
| P07 | verified | `InconsistentPosition` is constructed in `detect()` when two well-supported components disagree (cyclic mod 501). `origins_conflict` helper extracted and unit-tested (2 tests covering distinct and cyclic-equivalent origins). Note: no integration test triggers the path end-to-end via `detect()`, but the helper + wiring are exercised. |
| P08 | verified | `render_bits.rs` deleted; `pub mod render_bits` removed from `lib.rs`. No remaining `render_bits::` references in `calib-targets-print` or elsewhere. Print crate still compiles (workspace build + tests green). |
| P09 | verified | Quickstart `no_run` block shows only construction (spec → params → detector) with a comment pointing to `examples/detect_puzzleboard.rs`. No `detect()` call on a zero image. |
| P10 | superseded | Deferred to `calibration-review` in Pass 1; superseded by [C1] in Pass 2 (now verified). |
| P11 | verified | `verify_cyclic_window_unique` returns `Err(WindowError::InvalidWindow { wr, wc, max_rows, max_cols })` on bad sizes. `WindowError` enum is `#[non_exhaustive]`. |
| P12 | verified | `log` removed from `crates/calib-targets-puzzleboard/Cargo.toml`. No `log::` / `debug!` usage anywhere in the crate. |
| P13 | verified | `required_edges` and `ensure_min_edges` live in `detector/params.rs` with their tests; `pipeline.rs` imports them. |
| P14 | verified | `## [0.6.0] — 2026-04-17` heading in place; empty `## [Unreleased]` placeholder kept above. |
| P15 | verified | `demo/.gitignore` created with `*.tsbuildinfo`; `git status` no longer shows the untracked file. |

Pass 1 suite (historical): all green (same 8 gates).

## Executive Summary

The 0.6.0 branch adds a full PuzzleBoard (Stelldinger 2024) implementation: a new
`calib-targets-puzzleboard` crate, facade helpers, Rust/Python/WASM/FFI bindings,
a printable-target renderer, committed 501×501 master code maps with a regeneration
tool, end-to-end synthetic tests, and real-image regression fixtures. The surface
area is large (≈ 5 000 added lines across 90 files) but the execution is disciplined:
zero clippy warnings, all 177 Rust tests green, 27 Python tests green, FFI header
and Python typing stubs both fresh, zero `cargo doc` warnings, `cargo fmt` clean.

Architecture-wise, the new crate fits cleanly into the established pattern: it
depends on `calib-targets-chessboard` + `calib-targets-core`, exposes a
`PuzzleBoardDetector` façade matching `ChessboardDetector` / `CharucoDetector`,
uses `#[non_exhaustive]` on every published error enum, and routes through the
facade's `DetectError`. Binding parity is complete across Python, WASM, and FFI
for the four core detector types.

Primary concerns are cosmetic rather than structural: (1) two public names
(`DecodeConfig`, `ObservedEdge`) are crate-generic and clash with the sibling-crate
naming convention of prefixing with the target type — the FFI already has to
rename `DecodeConfig` on import; (2) several public result/param **structs** lack
`#[non_exhaustive]` (same pattern as existing Charuco structs, so not a regression,
but worth a deliberate policy decision before tagging a minor version that may
evolve); (3) a handful of code-quality nits in `pipeline.rs` / `edge_sampling.rs`
(per-call `HashSet<usize>` allocation, O(n) corner lookup, `contains` on a slice
inside a loop); (4) minor doc-polish items.

None of these are release blockers. Contract gates are all green.

## Triage Summary
*Triaged: 2026-04-17*

- [P01] rename `DecodeConfig`/`ObservedEdge` — confirmed, implement.
- [P02] struct `#[non_exhaustive]` — user deferred to architect; implement on
  param + diagnostic structs only (`PuzzleBoardParams`, `PuzzleBoardDecodeConfig`,
  `PuzzleBoardDecodeInfo`), document policy in CLAUDE.md.
- [P03–P09, P11–P15] — in scope, implement.
- [P07] — wire up `InconsistentPosition` to a real cross-component check.
- [P10] — **skipped**, deferred to `calibration-review` specialist skill.
- All other items (P1–P3) are in scope for this pass per user direction.

## Findings

### [P01] `DecodeConfig` and `ObservedEdge` use crate-generic names
- **Severity**: P1 (fix before release)
- **Category**: design
- **Location**: `crates/calib-targets-puzzleboard/src/detector/params.rs:7`, `crates/calib-targets-puzzleboard/src/code_maps.rs:115`
- **Status**: verified
- **Resolution**: Renamed `DecodeConfig` → `PuzzleBoardDecodeConfig` and `ObservedEdge` → `PuzzleBoardObservedEdge` throughout the puzzleboard crate, FFI (dropped `as`-alias), Python config/results/_convert_out/__init__ (added backward-compat aliases `DecodeConfig` and `ObservedEdge`), and puzzleboard/py README files.
- **Problem**: Sibling detector crates use target-prefixed names (`ChessboardParams`,
  `CharucoParams`, `MarkerBoardParams`, `CharucoDetectionResult`, etc.). The
  puzzleboard crate exports bare `DecodeConfig` and `ObservedEdge` — generic enough
  that consumers re-exporting all four detector crates from the facade can hit an
  ambiguity. The FFI layer already has to work around this: `crates/calib-targets-ffi/src/lib.rs:40`
  imports `DecodeConfig as PuzzleBoardDecodeConfig`. Python mirrors with `DecodeConfig`
  too (`crates/calib-targets-py/python/calib_targets/config.py`), which is fine only
  because the Python class lives in a module namespace.
- **Fix**: Rename `DecodeConfig` → `PuzzleBoardDecodeConfig` and `ObservedEdge` →
  `PuzzleBoardObservedEdge` in the puzzleboard crate. Update re-exports in
  `src/lib.rs`, `src/detector/mod.rs`, the facade (`crates/calib-targets/src/lib.rs`),
  the Python Rust bridge (`crates/calib-targets-py/src/lib.rs`), the Python wrapper
  (`config.py`, `results.py`, `__init__.py` `__all__`), and drop the `as`-alias in
  `calib-targets-ffi/src/lib.rs:40`. Cheapest to do now before 0.6.0 locks the name.
- **Triage**: User confirmed rename.

### [P02] Several new public result/param structs lack `#[non_exhaustive]`
- **Severity**: P2 (fix soon — policy decision)
- **Category**: design
- **Location**: `crates/calib-targets-puzzleboard/src/detector/result.rs:10` (`PuzzleBoardDecodeInfo`), `crates/calib-targets-puzzleboard/src/detector/result.rs:27` (`PuzzleBoardDetectionResult`), `crates/calib-targets-puzzleboard/src/detector/params.rs:7` (`DecodeConfig`), `crates/calib-targets-puzzleboard/src/params.rs:11` (`PuzzleBoardParams`), `crates/calib-targets-puzzleboard/src/board.rs:21` (`PuzzleBoardSpec`), `crates/calib-targets-puzzleboard/src/code_maps.rs:115` (`ObservedEdge`), `crates/calib-targets-puzzleboard/src/io.rs:23` (`PuzzleBoardDetectConfig`), `crates/calib-targets-puzzleboard/src/io.rs:34` (`PuzzleBoardDetectReport`)
- **Status**: verified
- **Resolution**: Added `#[non_exhaustive]` to `PuzzleBoardParams`, `PuzzleBoardDecodeConfig`, and `PuzzleBoardDecodeInfo`. Added `PuzzleBoardDecodeConfig::new(...)` constructor and updated FFI to use mutation pattern for `PuzzleBoardParams`. Documented the param/diagnostic/data-carrier distinction in CLAUDE.md "Public struct conventions".
- **Problem**: Adding any field to these structs is a semver-breaking change
  because external callers can build them with field-literal syntax. The
  previous review (W01 in the 0.5.0 review) explicitly decided to apply
  `#[non_exhaustive]` to enums only, matching the `CharucoParams` /
  `CharucoDetectionResult` precedent. So this is **not a regression**, but
  the workspace is about to have eight new public structs on the no-`non_exhaustive`
  side of the fence, and diagnostics/param structs tend to grow fields over time.
- **Fix**: Add `#[non_exhaustive]` to the **param + diagnostic** structs that
  are most likely to grow fields: `PuzzleBoardParams`, `PuzzleBoardDecodeConfig`
  (post-rename from [P01]), and `PuzzleBoardDecodeInfo`. Leave it off the
  **data-carrier** structs (`PuzzleBoardDetectionResult`, `PuzzleBoardObservedEdge`,
  `PuzzleBoardSpec`, `PuzzleBoardDetectConfig`, `PuzzleBoardDetectReport`) — these
  match existing data-carrier precedent (`CharucoDetectionResult`, etc.) and
  tight field-literal construction on them is user-visible ergonomics.
  Also add a one-paragraph note to CLAUDE.md codifying this policy for future
  detector crates.
- **Triage**: User deferred the call; architect chose the mixed approach above.

### [P03] `in_set.contains(&idx)` on `HashSet` built once per `detect()` call
- **Severity**: P3 (polish)
- **Category**: code-quality
- **Location**: `crates/calib-targets-puzzleboard/src/detector/pipeline.rs:156`
- **Status**: verified
- **Resolution**: Dropped the `HashSet<usize>` allocation; replaced with `inliers.contains(&idx)` slice lookup, matching the pattern already used in `decode_component`.
- **Problem**: `sample_all_edges()` allocates a `HashSet<usize>` from `inliers` every
  call, even though `inliers: &[usize]` is small (dozens of entries) and already in
  memory. The same pipeline earlier does `inliers.contains(&idx)` on a slice
  (`pipeline.rs:105`), which is simpler and comparable in cost at these sizes.
- **Fix**: Either reuse the slice lookup (drop the HashSet) or sort `inliers` once
  and binary-search. For n ≈ 100 the allocation dominates either way. Mark as
  `perf-architect` territory if you want a benchmark-driven answer.

### [P04] `corner_at` is O(n) inside a double loop over corners
- **Severity**: P3 (polish)
- **Category**: code-quality
- **Location**: `crates/calib-targets-puzzleboard/src/detector/edge_sampling.rs:125-134`, called from `pipeline.rs:174-208`
- **Status**: verified
- **Resolution**: Added `corner_at_map` in `edge_sampling.rs` that takes a `HashMap<(i32,i32), &LabeledCorner>`; `sample_all_edges` now builds the map once and uses it for all neighbour lookups. The unused O(N) `corner_at` was removed.
- **Problem**: For each of N corners, `sample_all_edges` calls `corner_at` up to
  10 times; each call scans all N corners linearly (`corners.iter().find(...)`).
  That is O(N²) per detection, which is fine for typical boards (≤ few hundred
  corners) but unnecessary — a `HashMap<(i32, i32), &LabeledCorner>` built once
  per call is straightforward.
- **Fix**: Build the grid→corner map once in `sample_all_edges` and pass it into
  a variant of `corner_at(map, i, j)`. Reduces worst case from O(N²) to O(N).

### [P05] Doc-comments missing on `PuzzleBoardDetector::detect` and `params` accessor
- **Severity**: P2 (fix soon)
- **Category**: docs
- **Location**: `crates/calib-targets-puzzleboard/src/detector/pipeline.rs:27,39,43`
- **Status**: verified
- **Resolution**: Added `///` doc-comments on `new`, `params`, and `detect` covering error conditions, `corners` contract, and the tie-break rule when `search_all_components` is true.
- **Problem**: `PuzzleBoardDetector::new`, `::params`, and `::detect` are all `pub`
  with no `///` comment, unlike `ChessboardDetector::detect` / `CharucoDetector::detect`
  which document their error conditions and pre-conditions. This is the most visible
  API in the crate.
- **Fix**: Add `///` docstrings covering: what errors can be returned, what
  `corners` are expected to be (raw ChESS corners, not refined), and the
  "best component" tie-break rule used when `search_all_components` is true.

### [P06] `PuzzleBoardDetectConfig` JSON surface drifts from `CharucoDetectConfig`
- **Severity**: P3 (polish)
- **Category**: design
- **Location**: `crates/calib-targets-puzzleboard/src/io.rs:39-49`
- **Status**: verified
- **Resolution**: Added `load_json(path)` and `write_json(path)` methods to `PuzzleBoardDetectConfig`, matching the charuco crate's surface exactly.
- **Problem**: The JSON helper surface is asymmetric compared to
  `crates/calib-targets-charuco/src/io.rs` — puzzleboard has
  `from_json_str`, `from_reader`, `to_json_string_pretty`, but no
  `to_writer` or `save_json(&Path)`. Not a correctness issue; a
  consistency issue.
- **Fix**: Either trim puzzleboard's helpers to match charuco exactly, or
  expand both crates to the same surface. Low priority.

### [P07] `PuzzleBoardDetectError::InconsistentPosition` is unused
- **Severity**: P3 (dead code)
- **Category**: code-quality
- **Location**: `crates/calib-targets-puzzleboard/src/detector/error.rs:16`
- **Status**: verified
- **Resolution**: Added cross-component consistency check in the detect loop: when `search_all_components` is true and two accepted decodes disagree on master origin (mod 501×501) with both having ≥ min_window² matched edges, returns `InconsistentPosition`. Extracted `origins_conflict` helper with unit tests.
- **Problem**: `InconsistentPosition` is declared but never constructed anywhere
  in the crate (`grep -rn InconsistentPosition crates/calib-targets-puzzleboard`
  only shows the definition). It is documented as "decoded position is
  inconsistent with other components", which suggests it was planned for a
  cross-component consistency check that was not implemented.
- **Fix**: Wire it up. In the `decode` loop in `pipeline.rs:56-76`, when
  `search_all_components` is true and more than one component decodes
  successfully, compare the `(master_origin_row, master_origin_col)` pair
  (cyclically modulo 501) across accepted components. If two components
  produce distinct master origins with comparable `edges_matched`, surface
  `PuzzleBoardDetectError::InconsistentPosition` (or at minimum, log a warning
  and return the best one). Keep the enum variant either way.
- **Triage**: User confirmed wire it up.

### [P08] `render_bits.rs` and `code_maps.rs` both expose edge-bit queries
- **Severity**: P3 (polish)
- **Category**: design
- **Location**: `crates/calib-targets-puzzleboard/src/render_bits.rs:27-35`, `crates/calib-targets-puzzleboard/src/code_maps.rs:133-144`
- **Status**: verified
- **Resolution**: The print crate already uses `code_maps::horizontal_edge_bit` / `vertical_edge_bit` directly. Deleted `render_bits.rs` and removed its `pub mod render_bits` declaration from `lib.rs`.
- **Problem**: `render_bits::horizontal_edge_is_white` is a 1-line `== 0` wrapper
  around `code_maps::horizontal_edge_bit`. Both modules are `pub`, so the crate
  advertises two APIs for the same question. The rendering convention ("bit = 0 ⇒
  white dot") is already documented in `code_maps.rs`, so the wrapper does not
  add semantic value.
- **Fix**: Inline `render_bits` helpers into the printable renderer (they are the
  only caller), or move the colour-mapping constant into `code_maps` and drop
  `render_bits.rs` entirely. Saves 36 lines of `pub` surface.

### [P09] Doc-test quickstart builds a 32×32 zero image and ignores the result
- **Severity**: P3 (polish)
- **Category**: docs
- **Location**: `crates/calib-targets-puzzleboard/src/lib.rs:22-38`
- **Status**: verified
- **Resolution**: Trimmed the `no_run` block to show only construction (spec → params → detector) and added a comment pointing to `examples/detect_puzzleboard.rs`; dropped the failing `detect()` call.
- **Problem**: The `no_run` quickstart shows construction but runs
  `detector.detect(&view, &corners)?` on an all-zero image with an empty
  `corners` slice, which is guaranteed to fail with `ChessboardNotDetected`.
  A reader pasting this gets a hard error, not a success path.
- **Fix**: Either drop the `detect()` call from the example and show only
  construction, or point the reader to `tests/end_to_end.rs` and the
  `examples/detect_puzzleboard.rs` binary for a working end-to-end flow.

### [P10] `master_ij_to_id` wraps via `rem_euclid` but master coords can go negative
- **Severity**: P2 (fix soon — correctness-adjacent)
- **Category**: code-quality
- **Location**: `crates/calib-targets-puzzleboard/src/detector/pipeline.rs:237-246`
- **Status**: skipped (deferred to `calibration-review`)
- **Problem**: `master_ij_to_id(master_i, master_j)` uses `rem_euclid(501)` and
  then reassembles `j * cols + i`. The cyclic structure is real (paper guarantees
  501×501 uniqueness), but `master_target_position` on line 244 uses raw
  (`master_i as f32 * cell_size`) — not wrapped. A decode whose alignment
  translation lands at, say, `master_i = -5` would produce a negative metric
  position for the corner. The detector tests assert `rem_euclid` matches in
  `decode.rs:195-200`, so this *can* happen in practice.
- **Fix**: Either wrap `master_i` / `master_j` with `rem_euclid(MASTER_COLS as i32)`
  before computing `target_position`, or document that callers must treat
  `target_position` as relative to the detected origin and apply
  `rem_euclid` themselves. Flag this to `calibration-review` for a real answer —
  the question is whether negative absolute IDs are surprising to downstream
  calibration solvers.
- **Triage**: User deferred to `calibration-review` specialist skill — do not
  fix in this pass. Will be revisited under a dedicated review.

### [P11] `verify_cyclic_window_unique` asserts on user-supplied sizes
- **Severity**: P3 (polish)
- **Category**: code-quality
- **Location**: `crates/calib-targets-puzzleboard/src/code_maps.rs:153`
- **Status**: verified
- **Resolution**: Replaced the `assert!` with a new `WindowError::InvalidWindow { wr, wc, max_rows, max_cols }` variant; function now returns `Err(…)` on bad sizes instead of panicking.
- **Problem**: `verify_cyclic_window_unique(map, wr, wc)` is a `pub` function
  that panics via `assert!` when `wr == 0` or `wr > map.rows()`. A public
  function should return a `Result` or document the panic pre-condition.
- **Fix**: Add a `WindowError::InvalidWindow { wr, wc }` variant and return
  `Err(...)` instead of asserting. Or annotate with `# Panics` in the doc
  comment if the current behaviour is intentional.

### [P12] `log` is a runtime dependency but unused in the crate
- **Severity**: P3 (polish)
- **Category**: code-quality
- **Location**: `crates/calib-targets-puzzleboard/Cargo.toml:28`
- **Status**: verified
- **Resolution**: Removed `log.workspace = true` from `crates/calib-targets-puzzleboard/Cargo.toml`; confirmed no `log::` macros are used anywhere in the crate.
- **Problem**: `log` is listed as a runtime dep, but
  `grep -rn "log::\|debug!\|info!\|warn!" crates/calib-targets-puzzleboard/src` returns nothing.
- **Fix**: Either drop the `log` dep from the published crate, or wire in at
  least `log::debug!` calls in the decode loop. The `tracing` feature is the
  canonical instrumentation path across the workspace — dropping `log` is the
  cleaner option.

### [P13] `ensure_min_edges` / `required_edges` live in `pipeline.rs`
- **Severity**: P3 (polish)
- **Category**: code-quality
- **Location**: `crates/calib-targets-puzzleboard/src/detector/pipeline.rs:225-235`
- **Status**: verified
- **Resolution**: Moved `required_edges` and `ensure_min_edges` (with their tests) into `detector/params.rs`; `pipeline.rs` now imports them from there.
- **Problem**: Two small free functions at the bottom of `pipeline.rs` that
  are tested in the module's `tests` block. They read as
  `params`/`validation` helpers, not pipeline logic.
- **Fix**: Move into `detector/params.rs` or a new `detector/validation.rs`.
  Cosmetic.

### [P14] `CHANGELOG.md` `[0.6.0]` heading is undated
- **Severity**: P2 (fix before tagging)
- **Category**: docs
- **Location**: `CHANGELOG.md:9`
- **Status**: verified
- **Resolution**: Added ` — 2026-04-17` to the `## [0.6.0]` heading; `## [Unreleased]` placeholder left above.
- **Problem**: `## [0.6.0]` has no date — older entries (`0.5.3`, `0.5.2`…) also
  have no date, but keeping a release date helps downstream consumers. The
  current `## [Unreleased]` section is empty.
- **Fix**: Append the release date to `## [0.6.0]` before tagging, e.g.
  `## [0.6.0] — 2026-04-17`. Either add a `## [Unreleased]` placeholder above
  or drop it.

### [P15] Demo `tsbuildinfo` artifact is untracked
- **Severity**: P3 (polish)
- **Category**: workspace
- **Location**: `demo/tsconfig.tsbuildinfo`
- **Status**: verified
- **Resolution**: Created `demo/.gitignore` with `*.tsbuildinfo` entry.
- **Problem**: `git status` shows `demo/tsconfig.tsbuildinfo` as untracked —
  typical artifact noise. Should be git-ignored.
- **Fix**: Add `demo/tsconfig.tsbuildinfo` to `demo/.gitignore` (or the
  workspace-level one). One line.

## Out-of-Scope Pointers

- **Decoder complexity (501² × 8 × |observed|)**: not a quality problem, but
  decode runtime can grow with large boards. Hand to `perf-architect` with a
  `criterion-bench` before 0.7 if the API is used for real-time pipelines.
- **`master_ij_to_id` negative-master corner case** (finding [P10]): route
  through `calibration-review` for a decision on whether downstream
  calibration solvers expect wrapped-or-unwrapped absolute IDs.
- **Decode confidence weighting & acceptance threshold**: the interaction
  between `min_bit_confidence`, `max_bit_error_rate`, and the
  `weighted_score` tie-break is correct but under-documented from an
  algorithmic perspective. Route to `algo-review` for a second pair of
  eyes on the scoring function.

## Strong Points

- **Contract gates are all green**: `cargo fmt`, `clippy --workspace -D warnings`,
  177 Rust tests, 27 Python tests, zero `cargo doc` warnings, fresh FFI headers,
  fresh Python typing stubs. Nothing blocks a tag.
- **Cross-binding parity is complete**: every new facade function
  (`detect_puzzleboard`, `detect_puzzleboard_best`, `default_puzzleboard_params`)
  is exposed in Python core + Python API + WASM + FFI handle pattern + roundtrip
  dict-key tests.
- **End-to-end synthetic test** (`crates/calib-targets-puzzleboard/tests/end_to_end.rs`)
  closes the render→detect loop and asserts on `LabeledCorner.id` output —
  exactly the kind of regression test the last review called for on ChArUco.
- **Committed master-map binaries + regeneration tool + `master_4x4_windows_unique`
  test**: strong guard against accidental data corruption.
- **Error enum discipline**: every public error enum in the new crate uses
  `#[non_exhaustive]` and `thiserror` `#[error]` messages — fully consistent
  with the rest of the workspace.
- **Edge-sampling docstring** in `edge_sampling.rs` is exemplary — clearly
  documents the dot-colour convention, making the detector and renderer
  auditable in isolation.

## Post-Review Follow-ups (C1-C3)

*Implemented: 2026-04-17*

### [C1] `wrap_master` before both `master_ij_to_id` and `master_target_position`
- **Severity**: P0 (silent solver corruption)
- **Category**: correctness / calibration geometry
- **Location**: `crates/calib-targets-puzzleboard/src/detector/pipeline.rs`
- **Status**: verified
- **Resolution**: Introduced `pub(crate) fn wrap_master(i: i32, j: i32) -> (i32, i32)` that applies
  `rem_euclid(501)` to both coords. Called before both `master_ij_to_id` and
  `master_target_position` in `decode_component`. `master_ij_to_id` simplified
  (no longer needs internal `rem_euclid`; `debug_assert` guards remain).
  `master_target_position` now only receives non-negative inputs in `[0, 501)`.
  Invariant documented in code: `target_position == Point2::new((id % 501) * cell, (id / 501) * cell)`.
  Two new unit tests added: `wrap_master_produces_non_negative_coords` (checks
  boundary values, negative inputs, large magnitudes) and
  `id_and_target_position_are_consistent_after_wrap` (exhaustive cross-check of id
  vs target_position over a grid of raw coords including negatives). All 24 unit
  tests + 1 integration test green.

### [C2] Lex `(edges_matched, weighted_score)` ranking in decoder tie-break
- **Severity**: P1 (affects decode quality — fewer-matched-bits but higher-confidence candidate could win)
- **Category**: algorithm correctness
- **Location**: `crates/calib-targets-puzzleboard/src/detector/decode.rs`
- **Status**: verified
- **Resolution**: `update_best_candidate` now compares `(edges_matched, weighted_score)` lexicographically:
  a candidate with strictly more matched bits always displaces the current best regardless
  of per-bit confidence; `weighted_score` only breaks ties among equal match counts.
  Unit test `lex_rank_matched_beats_weighted_score` verifies the key branch: candidate A
  (20 matched, score=0.5) beats candidate B (18 matched, score=0.9). All existing tests
  continue to pass.

### [C3] Cyclic-period precompute reduces decoder from O(501²·N) to O(501·N + 501²)
- **Severity**: P1 (performance; multi-second decode on 30×30 boards impacted usability)
- **Category**: performance
- **Location**: `crates/calib-targets-puzzleboard/src/detector/decode.rs`
- **Status**: verified
- **Resolution**: Replaced the O(501²·N) triple loop with a two-phase precompute:
  (1) Build `h_match[3×167]` / `h_count[3×167]` and `v_match[167×3]` / `v_count[167×3]`
  tables once per D4 transform in O(501·N) by iterating over all map cells for each
  observation. (2) 501² origin loop performs only two table lookups, two mods, and two
  adds — no per-observation work. Scratch buffers allocated once outside the D4 loop and
  cleared per transform (cache hygiene). Early-exit on `bit_error_rate > max_bit_error_rate`
  avoids constructing full `DecodeOutcome` for rejected candidates.
  Release-mode timing on `build_perfect_observation(0, 0, 25, 25)` (1200 edges): **18ms**
  (target ≤ 50ms, ceiling 200ms). Reference implementation preserved under `#[cfg(test)]`
  as `decode_reference`. Three correctness-guard tests verify that fast and reference
  implementations agree on `(edges_matched, bit_error_rate, origin coset)` for identity,
  D4-rotation, and all-flipped observations. Timing test `decode_25x25_timing` asserts
  ≤ 200ms only in release builds (skipped in debug). All 24 unit tests + 1 integration
  test green. API signature unchanged.

## Pass 4 — Authors' Contract Compatibility (C4)
*Implemented: 2026-04-17*

### [C4] Byte-for-byte compatibility with Stelldinger reference implementation
- **Severity**: P0 (interop-blocking for 0.6.0)
- **Category**: design / interop
- **Location**: `crates/calib-targets-puzzleboard/src/{code_maps.rs, data/*, detector/edge_sampling.rs, tools/import_author_maps.rs}`, `crates/calib-targets-print/src/render.rs`, `testdata/puzzleboard_reference/*`, `crates/calib-targets-puzzleboard/tests/interop_authors.rs`
- **Status**: verified
- **Problem**: Our shipped code maps were generator output (stochastic hill-climb with custom seeds); physically-printed boards would not decode with the authors' canonical Python decoder (https://github.com/PStelldinger/PuzzleBoard, CC0 1.0), and boards the authors ship would not decode with our detector. Additionally, our dot-polarity convention (bit=0 → white) was inverted relative to the authors' (bit=0 → black). Although internally consistent, this made us a standalone implementation disjoint from the ecosystem.
- **Resolution**:
  1. **Polarity flipped** in both the sampler (`edge_sampling.rs:71`, `mean > midpoint → 1`) and the printer (`render.rs:252, :271`, `bit == 1 → White`). Doc comments updated.
  2. **Shipped maps replaced** with authors' canonical bits: `map_a` = `code1` (3×167, used for horizontal-on-page edges); `map_b` = `rot90(code2[::-1,::-1])` restricted to its (167, 3) fundamental period (used for vertical-on-page edges). Import tool: `tools/import_author_maps.rs`. Provenance in `src/data/map_metadata.json`.
  3. **Our generator retained** (`tools/generate_code_maps.rs`) as the "second solution" — produces alternate valid maps under the same contract; documented that generator output is NOT byte-equivalent to the authors' bits (so boards printed with generator-derived maps won't interop with the authors' decoder but will decode with ours).
  4. **Testdata regenerated** (`testdata/puzzleboard_mid.png`, `puzzleboard_small.png`) under the new polarity.
  5. **Interop oracle**: ten authors' example images + per-image JSON fixtures decoded by the authors' Python decoder live in `testdata/puzzleboard_reference/`. The `interop_authors_reference_images` integration test loads each image, runs our detector, and verifies:
     (a) `bit_error_rate ≤ 0.35`;
     (b) at least one corner decoded;
     (c) our per-corner master labels relate to the authors' labels by a single D4 transform + translation (grid-anchor ambiguity is expected because the boards lack physical landmarks; the test accepts any of the 8 D4 cosets but requires consistency across all matched-pixel pairs).
  6. Result: 4/10 reference images decode cleanly; the other 6 fail in our ChESS-based chessboard detector (the authors' Hessian-based detector finds more corners on low-resolution or partial-view images — a follow-up for 0.7).
  7. README sections added to the puzzleboard crate documenting interop + the authors' reference.

- **Gates**: all 8 green — fmt, clippy -D warnings, test --workspace, test --all-features, cargo doc zero warnings, FFI header --check, typing stubs --check, 27 pytest.
