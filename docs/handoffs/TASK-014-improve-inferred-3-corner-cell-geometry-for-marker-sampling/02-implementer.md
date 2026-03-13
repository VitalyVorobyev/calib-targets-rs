# Improve inferred 3-corner cell geometry for marker sampling

- Task ID: `TASK-014-improve-inferred-3-corner-cell-geometry-for-marker-sampling`
- Backlog ID: `ALGO-002`
- Role: `implementer`
- Date: `2026-03-13`
- Status: `ready_for_review`

## Inputs Consulted
- `docs/handoffs/TASK-014-improve-inferred-3-corner-cell-geometry-for-marker-sampling/03-reviewer.md`
- `docs/handoffs/TASK-014-improve-inferred-3-corner-cell-geometry-for-marker-sampling/01-architect.md`
- `docs/handoffs/TASK-014-improve-inferred-3-corner-cell-geometry-for-marker-sampling/02-implementer.md`
- `docs/templates/task-handoff-report.md`
- `crates/calib-targets-charuco/src/detector/marker_sampling.rs`
- `crates/calib-targets-charuco/src/detector/marker_decode.rs`
- `crates/calib-targets-charuco/tests/regression.rs`
- Existing baseline diagnostics under `tmpdata/3536119669_first4_diag_rework/`
- Reviewer rerun artifacts under `tmpdata/3536119669_first4_review_rerun/`

## Summary
Addressed the reviewer-requested regressions without giving back the weak-strip gains. The final implementation keeps the local-lattice and axis-step estimator in `marker_sampling.rs`, adds one sampling-stage guardrail so invalid local/axis quads fall through to later fallback instead of dropping the cell, and adds an inferred-only decode-time comparison against the legacy parallelogram quad in `marker_decode.rs` so each scan hypothesis keeps the geometry that actually produces the better marker decode. I also added strip-level real-data regression tests for `target_0/strip_1`, `target_2/strip_5`, and `target_2/strip_3`. The fresh first-four rerun in `tmpdata/3536119669_first4_local_lattice_decode_fallback` removes the reviewer’s reported good-strip regressions and improves inferred support overall (`cells_with_any_decode_count 51 -> 56`, `selected_marker_count 36 -> 41`, `expected_id_match_count 33 -> 37`, `marker_count 197 -> 200`) with no final-corner losses.

## Decisions Made
- Kept all geometry frame semantics unchanged: `CornerMap` points and inferred quads remain image-space, grid coordinates remain lattice-space selectors, and no hidden pixel-center shifts were introduced.
- Moved the non-regression safeguard to decode time for inferred cells only. Each scan hypothesis now compares the current inferred quad against the legacy parallelogram quad and keeps the better decode, without changing marker thresholds, placement scoring, or public/report surfaces.
- Preserved sampling-stage conservatism by refusing local/axis estimates that complete to invalid quads before they can suppress later fallback.
- Added real-data strip regression checks alongside the existing synthetic fixture so the reviewer’s reported failure modes stay locked in CI instead of living only in ad hoc reruns.
- Repeated the weak-strip manual review on the changed failing strips after the final code change.

## Files/Modules Affected
- `crates/calib-targets-charuco/src/detector/marker_sampling.rs`
- `crates/calib-targets-charuco/src/detector/marker_decode.rs`
- `crates/calib-targets-charuco/tests/regression.rs`
- `docs/handoffs/TASK-014-improve-inferred-3-corner-cell-geometry-for-marker-sampling/02-implementer.md`

## Validation/Tests
- `cargo fmt --all --check` — passed
- `cargo clippy --workspace --all-targets -- -D warnings` — passed
- `cargo test-fast` — passed
- `cargo test --workspace --all-targets` — passed
- `cargo doc --workspace --all-features --no-deps` — passed, with the existing Cargo warning about `target/doc/calib_targets/index.html` filename collision between the `calib-targets` lib target and the `calib-targets-cli` bin target
- `mdbook build book` — passed
- `.venv/bin/python crates/calib-targets-py/tools/generate_typing_artifacts.py --check` — passed
- `.venv/bin/python -m maturin develop -m crates/calib-targets-py/Cargo.toml` — passed
- `.venv/bin/python -m pytest crates/calib-targets-py/python_tests` — passed
- `.venv/bin/python -m pyright --pythonpath .venv/bin/python crates/calib-targets-py/python_tests/typecheck_smoke.py` — passed
- `.venv/bin/python -m mypy crates/calib-targets-py/python_tests/typecheck_smoke.py` — passed
- `cargo test -p calib-targets-charuco --all-targets` — passed
- `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image testdata/3536119669/target_0.png --out-dir tmpdata/3536119669_first4_local_lattice_decode_fallback/target_0` — passed
- `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image testdata/3536119669/target_1.png --out-dir tmpdata/3536119669_first4_local_lattice_decode_fallback/target_1` — passed
- `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image testdata/3536119669/target_2.png --out-dir tmpdata/3536119669_first4_local_lattice_decode_fallback/target_2` — passed
- `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image testdata/3536119669/target_3.png --out-dir tmpdata/3536119669_first4_local_lattice_decode_fallback/target_3` — passed
- Manual weak-strip marker-overlay review on the changed failing strips using a small `.venv` PIL snippet against the generated `input.png` files:
  - `tmpdata/3536119669_first4_local_lattice_decode_fallback/target_0/strip_0/markers_overlay.png`
  - `tmpdata/3536119669_first4_local_lattice_decode_fallback/target_2/strip_3/markers_overlay.png`
  Both overlays still show the accepted marker quads sitting on visible marker cells rather than drifting off-lattice.

## Risks/Open Questions
- `target_0/strip_0` still improves only at inferred any-decode level (`1 -> 2`) rather than at inferred selected/matched markers. That is better than baseline and no longer regresses any good strip, but reviewer should still confirm whether the current strip-`0` lift is enough for `ALGO-002`.
- `tools/plot_charuco_overlay.py` still cannot open these `charuco_investigate` outputs because the stored `image_path` is duplicated relative to the report directory. I used a small `.venv` snippet for manual review instead; the tool bug is pre-existing and outside this task.

## Role-Specific Details

### Implementer
- Checklist executed:
  1. Resolved `ALGO-002` to `TASK-014-improve-inferred-3-corner-cell-geometry-for-marker-sampling` and verified the architect handoff was implementable.
  2. Turned the reviewer’s two reported regressions plus one representative strip-`3` gain into strip-level regression tests in `crates/calib-targets-charuco/tests/regression.rs`.
  3. Updated `marker_sampling.rs` so invalid local/axis quads fall through to the next fallback instead of suppressing a later valid estimate.
  4. Updated `marker_decode.rs` so inferred cells retry the legacy parallelogram quad per scan hypothesis and keep the geometry that produces the better decode.
  5. Reran the first-four ChArUco investigation workflow into `tmpdata/3536119669_first4_local_lattice_decode_fallback` and compared it against `tmpdata/3536119669_first4_diag_rework`.
  6. Reviewed fresh marker overlays for the changed weak failing strips.
  7. Ran the full implementer validation baseline, including `cargo test-fast`, full workspace tests, Rust docs, mdBook, and repo-venv Python checks.
- Code/tests changed:
  `crates/calib-targets-charuco/src/detector/marker_sampling.rs`
  - kept the local-lattice homography and axis-step estimator from the first implementation
  - added a full-quad validity check before accepting local/axis predictions so invalid quads now fall through to later fallback instead of deleting the cell outright

  `crates/calib-targets-charuco/src/detector/marker_decode.rs`
  - added an inferred-only retry against the legacy parallelogram quad
  - compares the new inferred quad and the legacy quad per scan hypothesis and keeps the better detection without changing reliability thresholds or public diagnostics shape

  `crates/calib-targets-charuco/tests/regression.rs`
  - added `algo_002_preserves_good_strip_marker_support` to lock the reviewer-reported regressions on `target_0/strip_1` and `target_2/strip_5`
  - added `algo_002_keeps_target_2_strip_3_gain` to keep a representative strip-`3` gain in place
- Deviations from plan:
  - The reviewer-driven non-regression fix extends into `marker_decode.rs` instead of living only in `marker_sampling.rs`. I made that change because actual marker decode quality, not local lattice residual alone, was the reliable place to arbitrate between the new local quad and the legacy baseline. This stays within architect-allowed detector-local plumbing and does not alter thresholds or public APIs.
  - Used a small `.venv` PIL snippet instead of the exact `tools/plot_charuco_overlay.py` command from the architect test plan because the current report `image_path` resolution still breaks that script for `charuco_investigate` outputs.
- Remaining follow-ups:
  Reviewer-requested first-four strip table against `tmpdata/3536119669_first4_diag_rework`:

  | Strip | Inferred Any Decode | Inferred Selected | Inferred Expected-ID Match | Markers | Final Corners |
  | --- | --- | --- | --- | --- | --- |
  | `0` | `8 -> 9` | `8 -> 8` | `8 -> 8` | `18 -> 18` | `83 -> 83` |
  | `1` | `13 -> 13` | `7 -> 7` | `7 -> 7` | `45 -> 45` | `178 -> 178` |
  | `2` | `5 -> 7` | `2 -> 3` | `2 -> 3` | `42 -> 43` | `173 -> 173` |
  | `3` | `5 -> 6` | `3 -> 5` | `0 -> 1` | `0 -> 0` | `0 -> 0` |
  | `4` | `12 -> 13` | `10 -> 12` | `10 -> 12` | `45 -> 47` | `179 -> 179` |
  | `5` | `8 -> 8` | `6 -> 6` | `6 -> 6` | `47 -> 47` | `181 -> 181` |

  Key reviewer strips are restored:
  `target_0/strip_1` stayed at baseline (`i_any 1 -> 1`, `i_sel 1 -> 1`, `i_match 1 -> 1`, `markers 8 -> 8`, `corners 38 -> 38`) and `target_2/strip_5` stayed at baseline (`i_any 3 -> 3`, `i_sel 2 -> 2`, `i_match 2 -> 2`, `markers 11 -> 11`, `corners 44 -> 44`).

## Next Handoff
Reviewer: verify that the invalid-quad fallback and inferred-only decode-time parallelogram retry stay within `ALGO-002` scope, that the new strip-level regression tests cover the reviewer-reported failures, and that the fresh `tmpdata/3536119669_first4_local_lattice_decode_fallback` comparison plus weak-strip overlay review are sufficient evidence that `ALGO-002` now improves inferred marker sampling without regressing successful strips.
