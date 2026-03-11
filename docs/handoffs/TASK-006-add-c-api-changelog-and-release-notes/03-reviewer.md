# Add C API changelog entries and release notes

- Task ID: `TASK-006-add-c-api-changelog-and-release-notes`
- Backlog ID: `FFI-005`
- Role: `reviewer`
- Date: `2026-03-11`
- Status: `complete`

## Inputs Consulted
- `docs/handoffs/TASK-006-add-c-api-changelog-and-release-notes/01-architect.md`
- `docs/handoffs/TASK-006-add-c-api-changelog-and-release-notes/02-implementer.md`
- `CHANGELOG.md`
- `docs/releases/ffi-c-api-release-draft.md`
- `docs/ffi/README.md`
- `crates/calib-targets-ffi/Cargo.toml`
- `crates/calib-targets-ffi/include/calib_targets_ffi.hpp`
- `crates/calib-targets-ffi/tests/native_consumer_smoke.rs`
- `docs/handoffs/TASK-003-add-conservative-detector-handles-and-detection-entry-points/04-architect.md`
- `docs/handoffs/TASK-004-add-c-examples-cpp-raii-wrapper-and-abi-verification/03-reviewer.md`
- `docs/handoffs/TASK-004-add-c-examples-cpp-raii-wrapper-and-abi-verification/04-architect.md`

## Summary
`FFI-005` adds the missing release-facing text for the C API launch without drifting into the broader usage-guide work reserved for `FFI-006`. The new `Unreleased` changelog entry accurately summarizes the shipped FFI scope, and the checked-in release-note draft is appropriately conservative about current support boundaries: repo-local `calib-targets-ffi`, grayscale-only input, built-in dictionaries only, current C++17 helper-wrapper expectations, and the absence of ergonomic CMake packaging in this release. I reproduced the high-risk validation path directly: the generated-header check and the full `cargo test --workspace --all-targets` run, which includes the external native consumer smoke test.

## Decisions Made
- Verdict: `approved`
- The release-note draft is sufficiently concise and accurate for a checked-in source document; it does not overstate packaging or support guarantees.
- Using an `Unreleased` changelog section is the correct implementation of the architect’s version-agnostic staging requirement while the final release number remains undecided.

## Files/Modules Affected
- `CHANGELOG.md`
- `docs/releases/ffi-c-api-release-draft.md`
- `docs/handoffs/TASK-006-add-c-api-changelog-and-release-notes/03-reviewer.md`

## Validation/Tests
- Reproduced: `cargo run -p calib-targets-ffi --bin generate-ffi-header -- --check` — passed
- Reproduced: `cargo test --workspace --all-targets` — passed
- Reviewed implementer evidence only: `cargo fmt --all --check`
- Reviewed implementer evidence only: `cargo clippy --workspace --all-targets -- -D warnings`
- Reviewed implementer evidence only: `cargo doc --workspace --all-features --no-deps`
- Reviewed implementer evidence only: `mdbook build book`
- Reviewed implementer evidence only: `.venv/bin/python crates/calib-targets-py/tools/generate_typing_artifacts.py --check`
- Reviewed implementer evidence only: `.venv/bin/python -m maturin develop -m crates/calib-targets-py/Cargo.toml`
- Reviewed implementer evidence only: `.venv/bin/python -m pytest crates/calib-targets-py/python_tests`
- Reviewed implementer evidence only: `.venv/bin/python -m pyright --pythonpath .venv/bin/python crates/calib-targets-py/python_tests/typecheck_smoke.py`
- Reviewed implementer evidence only: `.venv/bin/python -m mypy crates/calib-targets-py/python_tests/typecheck_smoke.py`
- Reviewed implementer evidence only: `cargo test -p calib-targets-ffi --test native_consumer_smoke -- --nocapture`

## Risks/Open Questions
- No blocking risks remain within `FFI-005` scope. The only unresolved item is the human release-prep choice of final version number, which is why the changelog entry remains under `Unreleased`.

## Role-Specific Details

### Reviewer
- Review scope:
  Architect acceptance criteria, implementer claims, the new changelog entry, the checked-in release-note draft, and the reproduced validation commands tied to the FFI release claims.
- Findings:
  1. No findings.
- Verdict:
  `approved`
- Required follow-up actions:
  1. Architect: write `04-architect.md`, preserve that `FFI-005` is approved, and hand the remaining version-number finalization to the human release step.

## Next Handoff
Architect: write `docs/handoffs/TASK-006-add-c-api-changelog-and-release-notes/04-architect.md`, summarize the approved release-facing documentation scope, and ask the human to finalize the release version/tag context when closing out the task.
