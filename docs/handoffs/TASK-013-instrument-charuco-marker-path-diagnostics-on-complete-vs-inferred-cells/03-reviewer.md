# Instrument ChArUco marker-path diagnostics on complete vs inferred cells

- Task ID: `TASK-013-instrument-charuco-marker-path-diagnostics-on-complete-vs-inferred-cells`
- Backlog ID: `ALGO-001`
- Role: `reviewer`
- Date: `2026-03-13`
- Status: `complete`

## Inputs Consulted
- `docs/handoffs/TASK-013-instrument-charuco-marker-path-diagnostics-on-complete-vs-inferred-cells/03-reviewer.md`
- `docs/handoffs/TASK-013-instrument-charuco-marker-path-diagnostics-on-complete-vs-inferred-cells/01-architect.md`
- `docs/handoffs/TASK-013-instrument-charuco-marker-path-diagnostics-on-complete-vs-inferred-cells/02-implementer.md`
- `docs/templates/task-handoff-report.md`
- `crates/calib-targets-charuco/src/detector/result.rs`
- `crates/calib-targets-charuco/src/detector/marker_decode.rs`
- `crates/calib-targets-charuco/src/detector/patch_placement.rs`
- `crates/calib-targets-charuco/src/detector/pipeline.rs`
- `crates/calib-targets-charuco/src/detector/candidate_eval.rs`
- `crates/calib-targets-charuco/src/io.rs`
- `crates/calib-targets-charuco/examples/charuco_investigate.rs`
- `crates/calib-targets-charuco/tests/regression.rs`
- `tmpdata/reviewer-target0-default/summary.json`
- `tmpdata/reviewer-target0-default/strip_2/report.json`
- `tmpdata/reviewer-target3-rectified/summary.json`
- `tmpdata/reviewer-target3-rectified/strip_1/report.json`
- `tmpdata/reviewer-target3-rectified/strip_3/report.json`

## Summary
The implementer addressed both blocking reviewer findings without changing detector acceptance behavior. Rotated successful detections now account expected-id matches in the same grid frame used by the selected markers, and rectified-recovery-selected runs now explicitly mark the marker-path diagnostics as only partially covering the chosen evaluation while redacting stale rollups from `summary.json` and `summary.csv`. I reran the full required local CI baseline in this workspace and regenerated the disputed investigation artifacts; the task now satisfies the architect acceptance criteria.

## Decisions Made
- Accepted the selected-marker-frame fix in `patch_placement.rs` as resolving the prior rotated expected-id mismatch.
- Accepted the `covers_selected_evaluation` flag plus summary redaction as satisfying the prior rectified-recovery stale-accounting finding while staying within the architect’s diagnostics-only scope.
- Treated the existing Cargo doc filename-collision warning as non-blocking because it is unchanged and outside `ALGO-001`.

## Files/Modules Affected
- `crates/calib-targets-charuco/src/detector/result.rs`
- `crates/calib-targets-charuco/src/detector/marker_decode.rs`
- `crates/calib-targets-charuco/src/detector/patch_placement.rs`
- `crates/calib-targets-charuco/src/detector/pipeline.rs`
- `crates/calib-targets-charuco/src/io.rs`
- `crates/calib-targets-charuco/examples/charuco_investigate.rs`
- `crates/calib-targets-charuco/tests/regression.rs`
- `docs/handoffs/TASK-013-instrument-charuco-marker-path-diagnostics-on-complete-vs-inferred-cells/03-reviewer.md`

## Validation/Tests
- `cargo fmt --all --check` — passed
- `cargo clippy --workspace --all-targets -- -D warnings` — passed
- `cargo test --workspace --all-targets` — passed
- `cargo doc --workspace --all-features --no-deps` — passed, with the existing Cargo warning about `target/doc/calib_targets/index.html` filename collision between the `calib-targets` lib and `calib-targets-cli` bin targets
- `mdbook build book` — passed
- `.venv/bin/python crates/calib-targets-py/tools/generate_typing_artifacts.py --check` — passed
- `.venv/bin/python -m maturin develop -m crates/calib-targets-py/Cargo.toml` — passed
- `.venv/bin/python -m pytest crates/calib-targets-py/python_tests` — passed
- `.venv/bin/python -m pyright --pythonpath .venv/bin/python crates/calib-targets-py/python_tests/typecheck_smoke.py` — passed
- `.venv/bin/python -m mypy crates/calib-targets-py/python_tests/typecheck_smoke.py` — passed
- `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image target_0.png --out-dir tmpdata/reviewer-target0-default` — passed
- `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image target_3.png --rectified-recovery --out-dir tmpdata/reviewer-target3-rectified` — passed
- Targeted artifact inspection confirmed:
  - `tmpdata/reviewer-target0-default/strip_2/report.json` now reports rotated-strip matches as complete `(selected=9, matched=9)` and inferred `(selected=1, matched=1)` under a non-identity alignment instead of the prior false zero-match output
  - `tmpdata/reviewer-target3-rectified/summary.json` marks strips `1` and `4` with `marker_path_covers_selected_evaluation=false` and redacts the rolled-up marker-path counters to `null`
  - `tmpdata/reviewer-target3-rectified/strip_1/report.json` retains the local marker-path detail but also sets `diagnostics.detection.marker_path.covers_selected_evaluation=false`, making the partial-coverage contract explicit

## Risks/Open Questions
- Raw `report.json` files for rectified-recovery-selected runs still contain local-only marker-path counts, so downstream consumers must honor `marker_path.covers_selected_evaluation` before treating those counts as the full selected path. The shipped investigation summaries already do this correctly.
- I did not rerun the full `target_0` through `target_3` default-plus-rectified matrix myself because the full CI baseline was green and the two previously disputed artifact paths reproduced as fixed.

## Role-Specific Details

### Reviewer
- Review scope:
  Compared the rework against the architect acceptance criteria and the prior `changes_requested` findings, inspected the detector/report code paths that changed, reran the required local CI baseline, and regenerated the real-data investigation outputs that previously demonstrated the rotated expected-id mismatch and rectified-recovery stale-summary issue.
- Findings:
  No blocking findings.

  The prior rotated expected-id accounting bug is resolved. `crates/calib-targets-charuco/src/detector/patch_placement.rs` now maps the selected marker’s `gc` through the chosen alignment when attributing expected-id matches, and the focused test `add_alignment_match_diagnostics_uses_selected_marker_grid_frame` covers that case directly. The regenerated `tmpdata/reviewer-target0-default/strip_2/report.json` confirms the fix on a non-identity alignment with non-zero expected-id matches for the accepted markers.

  The prior rectified-recovery stale-accounting bug is also resolved. `crates/calib-targets-charuco/src/detector/pipeline.rs` marks augmented selected evaluations with `marker_path.covers_selected_evaluation=false`, and `crates/calib-targets-charuco/examples/charuco_investigate.rs` redacts the rolled-up summary counters whenever coverage is partial. The regenerated `tmpdata/reviewer-target3-rectified/summary.json` and `tmpdata/reviewer-target3-rectified/strip_1/report.json` show the flag and redaction working as intended.
- Verdict:
  `approved`
- Required follow-up actions:
  1. Architect: write `docs/handoffs/TASK-013-instrument-charuco-marker-path-diagnostics-on-complete-vs-inferred-cells/04-architect.md` summarizing that `ALGO-001` now ships additive per-source marker-path diagnostics, corrected rotated expected-id accounting, and explicit partial-coverage signaling for rectified-recovery-selected reports.

## Next Handoff
Architect: prepare `docs/handoffs/TASK-013-instrument-charuco-marker-path-diagnostics-on-complete-vs-inferred-cells/04-architect.md` for the final human-facing synthesis, carrying forward that the reviewer-approved implementation meets the diagnostics-only scope and acceptance criteria for `ALGO-001`.
