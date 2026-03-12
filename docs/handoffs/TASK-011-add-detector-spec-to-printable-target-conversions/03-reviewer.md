# Add detector-spec to printable-target conversions

- Task ID: `TASK-011-add-detector-spec-to-printable-target-conversions`
- Backlog ID: `PRINT-002`
- Role: `reviewer`
- Date: `2026-03-12`
- Status: `complete`

## Inputs Consulted
- `docs/handoffs/TASK-011-add-detector-spec-to-printable-target-conversions/01-architect.md`
- `docs/handoffs/TASK-011-add-detector-spec-to-printable-target-conversions/02-implementer.md`
- `crates/calib-targets-print/src/model.rs`
- `crates/calib-targets-charuco/src/board.rs`
- `crates/calib-targets-marker/src/types.rs`

## Summary
The implementation matches the architect scope and keeps the change local to `calib-targets-print`. The new public API gives callers explicit millimeter-aware conversion entry points from `CharucoBoardSpec` and `MarkerBoardLayout` into printable specs/documents, while preserving the current crate layering and leaving rendering ownership in the print crate. I also reproduced the highest-risk checks around the new API surface: the print-crate test suite passes with the added conversion regressions, and rustdoc still builds successfully for the expanded public surface. No blocking review findings remain.

## Decisions Made
- Accept the explicit `*_mm` constructor naming as sufficiently clear about the unit contract.
- Accept the new printable-side `MissingMarkerBoardCellSize` error as the correct contract for detector layouts that cannot be printed deterministically.
- Accept the choice to keep this task code-only and not reopen the printable docs work, since the architect treated docs as optional and the new API is exposed through public inherent methods.

## Files/Modules Affected
- `crates/calib-targets-print/src/model.rs`
- `docs/handoffs/TASK-011-add-detector-spec-to-printable-target-conversions/03-reviewer.md`

## Validation/Tests
- Reviewed implementer evidence for the required local CI baseline:
  - `cargo fmt --all --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace --all-targets`
  - `cargo doc --workspace --all-features --no-deps`
  - `mdbook build book`
  - `.venv/bin/python crates/calib-targets-py/tools/generate_typing_artifacts.py --check`
  - `.venv/bin/python -m maturin develop -m crates/calib-targets-py/Cargo.toml`
  - `.venv/bin/python -m pytest crates/calib-targets-py/python_tests`
  - `.venv/bin/python -m pyright --pythonpath .venv/bin/python crates/calib-targets-py/python_tests/typecheck_smoke.py`
  - `.venv/bin/python -m mypy crates/calib-targets-py/python_tests/typecheck_smoke.py`
- Reproduced targeted high-risk checks:
  - `cargo test -p calib-targets-print` — passed, including the new ChArUco and marker-board conversion regression tests
  - `cargo doc --workspace --all-features --no-deps` — passed, with the pre-existing Cargo filename-collision warning between the facade lib and the repo-local CLI binary
- I did not rerun the full workspace baseline because the code change is localized to the printable model API and the implementer’s recorded baseline was coherent.

## Risks/Open Questions
- The ChArUco conversion helpers remain intentionally infallible and rely on later printable validation for malformed detector specs. That matches the existing printable API style, but downstream callers still need to validate/render before assuming a converted document is printable.
- The existing Cargo doc filename-collision warning between `calib-targets` and `calib-targets-cli` remains outside this task’s scope.

## Role-Specific Details

### Reviewer
- Review scope:
  Compared the implementation against the architect acceptance criteria, verified that the new API stays in `calib-targets-print` rather than pushing rendering concerns into detector crates, checked the field mapping against `CharucoBoardSpec` and `MarkerBoardLayout`, and reproduced the print-focused test/doc commands most likely to catch unit-contract or public-API regressions.
- Findings:
  1. No blocking findings remain. The new conversion helpers satisfy the architect requirements and preserve the intended crate boundaries.
- Verdict:
  `approved`
- Required follow-up actions:
  1. Architect: write `04-architect.md` for `TASK-011` and synthesize the approved scope for final human handoff.

## Next Handoff
Architect: prepare `docs/handoffs/TASK-011-add-detector-spec-to-printable-target-conversions/04-architect.md`, summarize the approved detector-spec conversion API, and hand the task back to the human for merge/backlog follow-up.
