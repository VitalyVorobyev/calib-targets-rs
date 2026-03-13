# Instrument ChArUco marker-path diagnostics on complete vs inferred cells

- Task ID: `TASK-013-instrument-charuco-marker-path-diagnostics-on-complete-vs-inferred-cells`
- Backlog ID: `ALGO-001`
- Role: `implementer`
- Date: `2026-03-13`
- Status: `ready_for_review`

## Inputs Consulted
- `docs/handoffs/TASK-013-instrument-charuco-marker-path-diagnostics-on-complete-vs-inferred-cells/03-reviewer.md`
- `docs/handoffs/TASK-013-instrument-charuco-marker-path-diagnostics-on-complete-vs-inferred-cells/01-architect.md`
- Previous `docs/handoffs/TASK-013-instrument-charuco-marker-path-diagnostics-on-complete-vs-inferred-cells/02-implementer.md`
- `docs/templates/task-handoff-report.md`
- `crates/calib-targets-charuco/src/detector/result.rs`
- `crates/calib-targets-charuco/src/detector/marker_decode.rs`
- `crates/calib-targets-charuco/src/detector/patch_placement.rs`
- `crates/calib-targets-charuco/src/detector/pipeline.rs`
- `crates/calib-targets-charuco/examples/charuco_investigate.rs`
- `crates/calib-targets-charuco/tests/regression.rs`
- Generated investigation artifacts under:
  - `tmpdata/3536119669_first4_diag_rework/`
  - `tmpdata/reviewer-rectified-target0-rework/`
  - `tmpdata/reviewer-rectified-target1-strip1-rework/`
  - `tmpdata/reviewer-rectified-target1-rework/`
  - `tmpdata/reviewer-rectified-target2-rework/`
  - `tmpdata/reviewer-rectified-target3-rework/`

## Summary
Addressed both reviewer findings without changing the detector’s acceptance rules. The expected-id accounting in `marker_path` now evaluates each selected marker in the same alignment frame used by the alignment solver, so rotated successful detections no longer report false zero-match diagnostics. For rectified-recovery cases, `marker_path` now carries an explicit `covers_selected_evaluation` flag, and the investigation summary redacts the rolled-up numeric counters when the chosen result includes augmented markers that the local cell-evidence diagnostics do not fully cover. The real-data reruns confirm the rotated-strip mismatch is fixed and the summary/report surfaces now stop silently exporting stale local-only counters for augmented selections.

## Decisions Made
- Fixed reviewer finding 1 by basing expected-id match/contradiction accounting on the selected marker’s aligned `MarkerDetection.gc`, not the sampled cell’s top-left `gc`.
- Kept `expected_marker_cell_count` as a candidate-cell geometry counter, but made the selected-marker outcome counters explicitly alignment-frame based. This preserves the stage split the architect asked for without changing detector behavior.
- Fixed reviewer finding 2 by adding additive coverage metadata to `MarkerPathDiagnostics`:
  - `covers_selected_evaluation = true` for local-only selected evaluations,
  - `covers_selected_evaluation = false` when rectified recovery contributes to the chosen augmented evaluation.
- Redacted the compact summary rollups (`summary.json` / `summary.csv`) whenever `covers_selected_evaluation` is false so the investigation workflow no longer silently presents local-only counts as the full selected path.
- Left patch-alignment selection logic unchanged to honor the architect’s “diagnostics only” constraint and avoid changing detection behavior in this task.

## Files/Modules Affected
- `crates/calib-targets-charuco/src/detector/result.rs`
- `crates/calib-targets-charuco/src/detector/marker_decode.rs`
- `crates/calib-targets-charuco/src/detector/patch_placement.rs`
- `crates/calib-targets-charuco/src/detector/pipeline.rs`
- `crates/calib-targets-charuco/examples/charuco_investigate.rs`
- `crates/calib-targets-charuco/tests/regression.rs`
- `docs/handoffs/TASK-013-instrument-charuco-marker-path-diagnostics-on-complete-vs-inferred-cells/02-implementer.md`

## Validation/Tests
- `cargo fmt --all` — passed
- `cargo fmt --all --check` — passed
- `cargo clippy --workspace --all-targets -- -D warnings` — passed
- `cargo test --workspace --all-targets` — passed
- `cargo test -p calib-targets-charuco --all-targets` — passed
- `cargo doc --workspace --all-features --no-deps` — passed, with the existing Cargo doc filename-collision warning between the `calib-targets` lib target and the `calib-targets-cli` bin target on `target/doc/calib_targets/index.html`
- `mdbook build book` — passed
- `.venv/bin/python crates/calib-targets-py/tools/generate_typing_artifacts.py --check` — passed
- `.venv/bin/python -m maturin develop -m crates/calib-targets-py/Cargo.toml` — passed
- `.venv/bin/python -m pytest crates/calib-targets-py/python_tests` — passed
- `.venv/bin/python -m pyright --pythonpath .venv/bin/python crates/calib-targets-py/python_tests/typecheck_smoke.py` — passed
- `.venv/bin/python -m mypy crates/calib-targets-py/python_tests/typecheck_smoke.py` — passed
- `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image target_0.png --out-dir tmpdata/3536119669_first4_diag_rework/target_0` — passed
- `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image target_1.png --out-dir tmpdata/3536119669_first4_diag_rework/target_1` — passed
- `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image target_2.png --out-dir tmpdata/3536119669_first4_diag_rework/target_2` — passed
- `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image target_3.png --out-dir tmpdata/3536119669_first4_diag_rework/target_3` — passed
- `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image target_0.png --rectified-recovery --out-dir tmpdata/reviewer-rectified-target0-rework` — passed
- `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image target_1.png --strip 1 --rectified-recovery --out-dir tmpdata/reviewer-rectified-target1-strip1-rework` — passed
- `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image target_1.png --rectified-recovery --out-dir tmpdata/reviewer-rectified-target1-rework` — passed
- `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image target_2.png --rectified-recovery --out-dir tmpdata/reviewer-rectified-target2-rework` — passed
- `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image target_3.png --rectified-recovery --out-dir tmpdata/reviewer-rectified-target3-rework` — passed
- Inspected updated default investigation artifacts:
  - `tmpdata/3536119669_first4_diag_rework/target_0/strip_2/report.json` now reports complete `(selected=9, matched=9, non_marker_confident=0)` and inferred `(selected=1, matched=1, non_marker_confident=0)`, fixing the reviewer’s zero-match repro on a successful rotated strip
  - `tmpdata/3536119669_first4_diag_rework/target_3/strip_3/report.json` now reports complete `(selected=7, matched=4, contradictions=3)` and inferred `(selected=1, matched=0, contradictions=1)`, which makes the weak-strip failure mode explicit instead of collapsing everything into contradictions/non-marker counts
- Inspected updated rectified-recovery artifacts:
  - `tmpdata/reviewer-rectified-target3-rework/summary.json` marks strips `1` and `4` with `marker_path_covers_selected_evaluation=false` and null compact rollups
  - the corresponding `report.json` files retain the local marker-path detail but also set `marker_path.covers_selected_evaluation=false`, making the partial coverage explicit instead of silently claiming full selected-evaluation accounting

## Risks/Open Questions
- `expected_marker_cell_count` remains candidate-cell based, while the selected-marker outcome counters (`expected_id_match_count`, `expected_id_contradiction_count`, `non_marker_confident_decode_count`) now use the selected marker’s alignment frame. That is intentional and keeps the stage names honest, but Reviewer should verify that this mixed-but-explicit interpretation is acceptable for the long-term report surface.
- The new coverage flag only turns false when the augmented rectified-recovery evaluation wins. Local-only runs and rectified runs that still select the local path remain fully covered, which is the intended behavior.
- The existing Cargo doc filename-collision warning remains outside this task’s scope.

## Role-Specific Details

### Implementer
- Checklist executed:
  1. Read the reviewer handoff and treated both findings as mandatory rework.
  2. Reworked expected-id diagnostics so selected markers are evaluated in the same grid frame as the alignment solver.
  3. Added coverage metadata to `MarkerPathDiagnostics` and wired the augmented rectified-recovery path to mark local-only accounting as partial when it no longer covers the selected evaluation.
  4. Updated the investigation summary writer to expose the new coverage flag and to redact stale numeric rollups when the selected evaluation is not fully covered.
  5. Added focused regression/unit coverage for:
     - selected-marker alignment-frame accounting in patch placement,
     - summary rollup redaction behavior,
     - stronger marker-path invariants in the ChArUco regression fixture.
  6. Reran the full required validation baseline, plus the architect’s real-dataset commands and the reviewer’s rectified-recovery probes.
- Code/tests changed:
  `crates/calib-targets-charuco/src/detector/result.rs`
  - added `MarkerPathDiagnostics.covers_selected_evaluation`

  `crates/calib-targets-charuco/src/detector/marker_decode.rs`
  - initializes `covers_selected_evaluation=true` for local cell-evidence summaries

  `crates/calib-targets-charuco/src/detector/patch_placement.rs`
  - switched expected-id diagnostics to use the selected marker’s aligned grid frame
  - added a focused regression for the rotated selected-marker frame mismatch reported by Reviewer

  `crates/calib-targets-charuco/src/detector/pipeline.rs`
  - marks `marker_path.covers_selected_evaluation=false` when an augmented rectified-recovery evaluation is selected

  `crates/calib-targets-charuco/examples/charuco_investigate.rs`
  - added `marker_path_covers_selected_evaluation` to strip summaries
  - changed the rolled-up marker-path counters to `Option<usize>` and emits `null` / empty CSV cells when coverage is partial
  - added example-local tests for the summary redaction helper

  `crates/calib-targets-charuco/tests/regression.rs`
  - now asserts `covers_selected_evaluation=true` on the default regression fixture
  - now asserts that each source bucket partitions selected markers into match / contradiction / non-marker outcomes
- Deviations from plan:
  - None beyond the reviewer-mandated rework. The implementation stayed within the architect’s diagnostics-only scope and did not alter detector acceptance policy.
- Remaining follow-ups:
  - Reviewer should confirm that the selected-marker-based expected-id accounting and the partial-coverage redaction semantics are the right long-term diagnostics contract.

## Next Handoff
Reviewer: verify that the rotated-strip expected-id accounting is now correct in the generated reports, that partial rectified-recovery coverage is explicitly flagged and redacted in summary artifacts, and that the updated marker-path invariants satisfy the architect acceptance criteria without changing detector behavior.
