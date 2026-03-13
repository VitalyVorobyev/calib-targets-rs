# Strengthen discrete patch placement scoring with richer local evidence

- Task ID: `TASK-015-strengthen-discrete-patch-placement-scoring-with-richer-local-evidence`
- Backlog ID: `ALGO-003`
- Role: `architect`
- Date: `2026-03-13`
- Status: `ready_for_human`

## Inputs Consulted
- `docs/handoffs/TASK-015-strengthen-discrete-patch-placement-scoring-with-richer-local-evidence/01-architect.md`
- `docs/handoffs/TASK-015-strengthen-discrete-patch-placement-scoring-with-richer-local-evidence/02-implementer.md`
- `docs/handoffs/TASK-015-strengthen-discrete-patch-placement-scoring-with-richer-local-evidence/03-reviewer.md`
- `docs/backlog.md`
- `crates/calib-targets-charuco/src/detector/patch_placement.rs`
- `crates/calib-targets-charuco/src/detector/pipeline.rs`
- `crates/calib-targets-charuco/src/detector/result.rs`
- `crates/calib-targets-charuco/examples/charuco_investigate.rs`
- `crates/calib-targets-charuco/tests/regression.rs`

## Summary
`ALGO-003` is complete. The ChArUco detector now scores legal patch placements with explicit per-candidate evidence rather than sparse matched-ID counts alone, while keeping the default path discrete, local, and calibration-free. Reviewer approved the task with one minor follow-up on closeout wording: on the first four real composites, `target_2/strip_3` is the only failure-status change (`3 -> 4` alignment inliers), but three already-successful strips also gain one inferred marker each without changing final corner counts or introducing an obviously wrong placement.

## Decisions Made
- `ALGO-003` should be closed in the backlog as complete.
- The next algorithmic gate remains `ALGO-004`: `ALGO-003` improved placement explainability and some weak-strip support, but it did not yet reach the `24/24` successful-strip target.

## Files/Modules Affected
- `crates/calib-targets-charuco/src/detector/patch_placement.rs`
- `crates/calib-targets-charuco/src/detector/pipeline.rs`
- `crates/calib-targets-charuco/src/detector/result.rs`
- `crates/calib-targets-charuco/src/io.rs`
- `crates/calib-targets-charuco/examples/charuco_investigate.rs`
- `crates/calib-targets-charuco/tests/regression.rs`
- `docs/handoffs/TASK-015-strengthen-discrete-patch-placement-scoring-with-richer-local-evidence/04-architect.md`
- `docs/backlog.md`

## Validation/Tests
- Reviewed reviewer evidence: `cargo test --workspace --all-targets` — passed
- Reviewed reviewer evidence: `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image testdata/3536119669/target_0.png --out-dir tmpdata/3536119669_first4_patch_scoring_reviewer_recheck/target_0` — passed
- Reviewed reviewer evidence: `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image testdata/3536119669/target_1.png --out-dir tmpdata/3536119669_first4_patch_scoring_reviewer_recheck/target_1` — passed
- Reviewed reviewer evidence: `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image testdata/3536119669/target_2.png --out-dir tmpdata/3536119669_first4_patch_scoring_reviewer_recheck/target_2` — passed
- Reviewed reviewer evidence: `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image testdata/3536119669/target_3.png --out-dir tmpdata/3536119669_first4_patch_scoring_reviewer_recheck/target_3` — passed
- Reviewed implementer evidence:
  - `cargo fmt --all --check` — passed
  - `cargo clippy --workspace --all-targets -- -D warnings` — passed
  - `cargo doc --workspace --all-features --no-deps` — passed
  - `mdbook build book` — passed
  - `.venv/bin/python crates/calib-targets-py/tools/generate_typing_artifacts.py --check` — passed
  - `.venv/bin/maturin develop -m crates/calib-targets-py/Cargo.toml` — passed
  - `.venv/bin/pytest crates/calib-targets-py/python_tests` — passed
  - `.venv/bin/python -m pyright --pythonpath .venv/bin/python crates/calib-targets-py/python_tests/typecheck_smoke.py` — passed
  - `.venv/bin/python -m mypy crates/calib-targets-py/python_tests/typecheck_smoke.py` — passed

## Risks/Open Questions
- The first-four gate is still not met: totals remain `18/24` successful strips and `15/24` strips with `>= 40` corners, so `ALGO-004` remains the next required algorithmic step.
- The additive `patch_placement` diagnostics are intentionally explanatory even when `covers_selected_evaluation = false`; downstream human-facing summaries should keep that distinction explicit.
- `tools/plot_charuco_overlay.py` still has the pre-existing tmpdata `image_path` resolution issue and should stay separate from the `ALGO-003` closeout.

## Role-Specific Details

### Architect Closeout
- Delivered scope:
  Refactored patch-placement scoring to compute explicit best/runner-up evidence summaries, rank legal placements lexicographically by exact matches, contradictions, and bounded support cues, and surface additive `patch_placement` diagnostics through detector reports and `charuco_investigate` summaries. Added deterministic comparator/report regression coverage and preserved the default detector policy (`min_marker_inliers = 6`, no new global-stage defaults).
- Reviewer verdict incorporated:
  `approved_with_minor_followups`; no implementer rework is required. The closeout corrects the first-four delta summary to include the three marker-support-only gains on already-successful strips.
- Human decision requested:
  Accept `ALGO-003` as complete and keep `ALGO-004` as the next algorithmic gate. The implementation improved placement explainability and weak-strip support without introducing a reviewed wrong placement, but it did not yet satisfy the `24/24` first-four success target.
- Suggested backlog follow-ups:
  - Continue with already-tracked `ALGO-004`.
  - Keep already-tracked `ALGO-005` after `ALGO-004` reaches the first-four gate.
  - Optionally backlog the `tools/plot_charuco_overlay.py` report-path bug as separate investigation tooling work if direct overlay review from `charuco_investigate` outputs should become frictionless.

## Next Handoff
Human: accept `ALGO-003` as closed in the backlog, use the reviewer-confirmed first-four results as the final record for this task, and decide when to start `ALGO-004`.
