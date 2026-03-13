# Improve inferred 3-corner cell geometry for marker sampling

- Task ID: `TASK-014-improve-inferred-3-corner-cell-geometry-for-marker-sampling`
- Backlog ID: `ALGO-002`
- Role: `reviewer`
- Date: `2026-03-13`
- Status: `complete`

## Inputs Consulted
- `docs/handoffs/TASK-014-improve-inferred-3-corner-cell-geometry-for-marker-sampling/03-reviewer.md`
- `docs/handoffs/TASK-014-improve-inferred-3-corner-cell-geometry-for-marker-sampling/01-architect.md`
- `docs/handoffs/TASK-014-improve-inferred-3-corner-cell-geometry-for-marker-sampling/02-implementer.md`
- `docs/templates/task-handoff-report.md`
- `crates/calib-targets-charuco/src/detector/marker_sampling.rs`
- `crates/calib-targets-charuco/src/detector/marker_decode.rs`
- `crates/calib-targets-charuco/tests/regression.rs`
- `tmpdata/3536119669_first4_diag_rework/`
- `tmpdata/3536119669_first4_local_lattice_decode_fallback/`
- Fresh reviewer rerun under `tmpdata/3536119669_first4_reviewer_recheck/`

## Summary
No blocking findings remain. The implementer preserved the intended local-lattice estimator in `marker_sampling.rs`, fixed the prior good-strip regression by rejecting invalid local/axis quads before fallback suppression and by adding an inferred-only decode-time comparison against the legacy parallelogram cell in `marker_decode.rs`, and added real-data strip regressions to lock the specific failure modes from the previous review. I reproduced the acceptance-critical checks: the new `ALGO-002` regression tests pass, a fresh four-target `charuco_investigate` rerun into `tmpdata/3536119669_first4_reviewer_recheck` matches the implementer’s reported table exactly, the previously regressed good strips are restored to baseline, and the weak-strip gains remain. Manual overlay review of the only changed weak strips still shows accepted marker quads sitting on visible marker cells. Verdict: `approved`.

## Decisions Made
- Treated the `marker_decode.rs` retry as within architect scope because it stays detector-local, only applies to inferred cells, does not alter thresholds, and acts as explicit conservative fallback arbitration rather than a broader decode-policy rewrite.
- Treated the fresh first-four rerun as the decisive acceptance artifact because architect criterion 4 was the previous blocker and is now satisfied.
- Treated the existing Cargo doc filename-collision warning and the existing `tools/plot_charuco_overlay.py` path bug as non-blocking because both predate `ALGO-002`.

## Files/Modules Affected
- `crates/calib-targets-charuco/src/detector/marker_sampling.rs`
- `crates/calib-targets-charuco/src/detector/marker_decode.rs`
- `crates/calib-targets-charuco/tests/regression.rs`
- `docs/handoffs/TASK-014-improve-inferred-3-corner-cell-geometry-for-marker-sampling/03-reviewer.md`

## Validation/Tests
- `cargo fmt --all --check` — reproduced, passed
- `cargo test -p calib-targets-charuco --all-targets` — reproduced, passed
- `cargo test -p calib-targets-charuco --test regression algo_002_ -- --nocapture` — reproduced, passed
- `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image testdata/3536119669/target_0.png --out-dir tmpdata/3536119669_first4_reviewer_recheck/target_0` — reproduced, passed
- `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image testdata/3536119669/target_1.png --out-dir tmpdata/3536119669_first4_reviewer_recheck/target_1` — reproduced, passed
- `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image testdata/3536119669/target_2.png --out-dir tmpdata/3536119669_first4_reviewer_recheck/target_2` — reproduced, passed
- `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image testdata/3536119669/target_3.png --out-dir tmpdata/3536119669_first4_reviewer_recheck/target_3` — reproduced, passed
- Reviewed implementer evidence, but did not rerun, these remaining baseline commands because the reproduced crate tests plus fresh dataset rerun covered the acceptance-critical risk:
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace --all-targets`
  - `cargo doc --workspace --all-features --no-deps`
  - `mdbook build book`
  - `.venv/bin/python crates/calib-targets-py/tools/generate_typing_artifacts.py --check`
  - `.venv/bin/python -m maturin develop -m crates/calib-targets-py/Cargo.toml`
  - `.venv/bin/python -m pytest crates/calib-targets-py/python_tests`
  - `.venv/bin/python -m pyright --pythonpath .venv/bin/python crates/calib-targets-py/python_tests/typecheck_smoke.py`
  - `.venv/bin/python -m mypy crates/calib-targets-py/python_tests/typecheck_smoke.py`
- Compared `tmpdata/3536119669_first4_reviewer_recheck/*/summary.json` against `tmpdata/3536119669_first4_diag_rework/*/summary.json`:

  | Strip | Inferred Any Decode | Inferred Selected | Inferred Expected-ID Match | Markers | Final Corners |
  | --- | --- | --- | --- | --- | --- |
  | `0` | `8 -> 9` | `8 -> 8` | `8 -> 8` | `18 -> 18` | `83 -> 83` |
  | `1` | `13 -> 13` | `7 -> 7` | `7 -> 7` | `45 -> 45` | `178 -> 178` |
  | `2` | `5 -> 7` | `2 -> 3` | `2 -> 3` | `42 -> 43` | `173 -> 173` |
  | `3` | `5 -> 6` | `3 -> 5` | `0 -> 1` | `0 -> 0` | `0 -> 0` |
  | `4` | `12 -> 13` | `10 -> 12` | `10 -> 12` | `45 -> 47` | `179 -> 179` |
  | `5` | `8 -> 8` | `6 -> 6` | `6 -> 6` | `47 -> 47` | `181 -> 181` |

- Confirmed the previously regressed strips are restored:
  - `target_0/strip_1`: inferred `any 1 -> 1`, `selected 1 -> 1`, `match 1 -> 1`, `markers 8 -> 8`, `corners 38 -> 38`
  - `target_2/strip_5`: inferred `any 3 -> 3`, `selected 2 -> 2`, `match 2 -> 2`, `markers 11 -> 11`, `corners 44 -> 44`
- Confirmed the representative strip-`3` gain remains:
  - `target_2/strip_3`: inferred `any 2 -> 3`, `selected 1 -> 3`, `match 0 -> 1`
- Reviewed implementer-generated overlays for the only changed weak strips:
  - `tmpdata/3536119669_first4_local_lattice_decode_fallback/target_0/strip_0/markers_overlay.png`
  - `tmpdata/3536119669_first4_local_lattice_decode_fallback/target_2/strip_3/markers_overlay.png`
  Both overlays keep the accepted marker quads on visible marker cells with no obvious off-lattice drift or wrong-board placement.

## Risks/Open Questions
- The synthetic estimator tests still rely on the generic invalid-quad path plus the existing clockwise-order test rather than adding a dedicated self-crossing inferred-cell fixture. That is a non-blocking coverage gap, not a reproduced defect.
- `tools/plot_charuco_overlay.py` still does not resolve these `charuco_investigate` report paths correctly. The manual overlay review remains reproducible via the generated PNGs, so this does not block `ALGO-002`.

## Role-Specific Details

### Reviewer
- Review scope:
  Re-checked the architect acceptance criteria against the updated implementation, inspected the new fallback arbitration in `marker_decode.rs`, re-ran the crate tests and the acceptance-critical first-four real-data workflow, diffed the resulting summaries against `tmpdata/3536119669_first4_diag_rework`, and manually reviewed the only changed weak-strip overlays.
- Findings:
  No blocking findings. The previous `target_0/strip_1` and `target_2/strip_5` regressions are gone in a fresh rerun, the weak-strip gains remain, the detector still does not invent new final corners, and the added strip regressions in `crates/calib-targets-charuco/tests/regression.rs` now cover the specific reviewer-reported failure modes.
- Evidence-backed code points:
  - `crates/calib-targets-charuco/src/detector/marker_sampling.rs:140` now rejects invalid local/axis completions before they can suppress fallback.
  - `crates/calib-targets-charuco/src/detector/marker_sampling.rs:741` and `crates/calib-targets-charuco/src/detector/marker_sampling.rs:753` add deterministic inferred-cell geometry coverage for degenerate rejection and all four missing-corner cases.
  - `crates/calib-targets-charuco/src/detector/marker_decode.rs:146` and `crates/calib-targets-charuco/src/detector/marker_decode.rs:183` add inferred-only decode-time fallback arbitration against the legacy parallelogram cell.
  - `crates/calib-targets-charuco/tests/regression.rs:329` and `crates/calib-targets-charuco/tests/regression.rs:424` lock the reviewer-requested real-data non-regression and strip-`3` gain cases.
- Verdict:
  `approved`
- Follow-up actions:
  1. No implementer rework required.
  2. Architect should prepare `04-architect.md` and may note the non-blocking overlay-tool issue and the optional future explicit self-crossing inferred-cell fixture as follow-up work, not as a reopen of `ALGO-002`.

## Next Handoff
Architect: produce the final human-facing synthesis in `04-architect.md`, using the approved `ALGO-002` implementation evidence from the fresh reviewer rerun in `tmpdata/3536119669_first4_reviewer_recheck`.
