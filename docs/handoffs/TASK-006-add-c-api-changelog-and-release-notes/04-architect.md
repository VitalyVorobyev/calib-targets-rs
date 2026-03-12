# Add C API changelog entries and release notes

- Task ID: `TASK-006-add-c-api-changelog-and-release-notes`
- Backlog ID: `FFI-005`
- Role: `architect`
- Date: `2026-03-11`
- Status: `ready_for_human`

## Inputs Consulted
- `docs/handoffs/TASK-006-add-c-api-changelog-and-release-notes/01-architect.md`
- `docs/handoffs/TASK-006-add-c-api-changelog-and-release-notes/02-implementer.md`
- `docs/handoffs/TASK-006-add-c-api-changelog-and-release-notes/03-reviewer.md`
- `docs/backlog.md`
- `CHANGELOG.md`
- `docs/releases/ffi-c-api-release-draft.md`

## Summary
`FFI-005` is complete. The repo now has release-facing documentation for the C API launch in two forms: a staged `Unreleased` changelog entry and a checked-in release-note draft that can be used as the eventual tag or GitHub release body. Reviewer approved the task without findings and confirmed that the release text stays within the shipped `FFI-002` through `FFI-004` surface without overstating native packaging or support guarantees.

## Decisions Made
- `FFI-005` should be closed in the backlog as complete.
- The remaining unresolved item is not implementation work but human release preparation: choosing the final version number/tag context that will replace the temporary `Unreleased` staging label.

## Files/Modules Affected
- `CHANGELOG.md`
- `docs/releases/ffi-c-api-release-draft.md`
- `docs/handoffs/TASK-006-add-c-api-changelog-and-release-notes/04-architect.md`

## Validation/Tests
- Reviewed reviewer evidence: `cargo run -p calib-targets-ffi --bin generate-ffi-header -- --check` — passed
- Reviewed reviewer evidence: `cargo test --workspace --all-targets` — passed
- Reviewed implementer evidence: `cargo fmt --all --check` — passed
- Reviewed implementer evidence: `cargo clippy --workspace --all-targets -- -D warnings` — passed
- Reviewed implementer evidence: `cargo doc --workspace --all-features --no-deps` — passed
- Reviewed implementer evidence: `mdbook build book` — passed
- Reviewed implementer evidence: `.venv/bin/python crates/calib-targets-py/tools/generate_typing_artifacts.py --check` — passed
- Reviewed implementer evidence: `.venv/bin/python -m maturin develop -m crates/calib-targets-py/Cargo.toml` — passed
- Reviewed implementer evidence: `.venv/bin/python -m pytest crates/calib-targets-py/python_tests` — passed
- Reviewed implementer evidence: `.venv/bin/python -m pyright --pythonpath .venv/bin/python crates/calib-targets-py/python_tests/typecheck_smoke.py` — passed
- Reviewed implementer evidence: `.venv/bin/python -m mypy crates/calib-targets-py/python_tests/typecheck_smoke.py` — passed
- Reviewed implementer evidence: `cargo test -p calib-targets-ffi --test native_consumer_smoke -- --nocapture` — passed

## Risks/Open Questions
- The changelog entry is intentionally staged under `Unreleased`. Human release prep still needs to decide the final release version and whether to rename or copy the checked-in draft release note to match that version.
- `FFI-006` remains the release blocker for the broader C API guide/tutorial surface and should stay active until those docs land.

## Role-Specific Details

### Architect Closeout
- Delivered scope:
  Added a release-facing changelog entry for the C API launch and a checked-in release-note draft that captures shipped scope, validation, support boundaries, and deferred follow-up work.
- Reviewer verdict incorporated:
  `approved`; no review findings remain.
- Human decision requested:
  Accept `FFI-005` as complete, keep `FFI-006` as the remaining release-blocking docs task, and finalize the release version/tag context when moving the `Unreleased` entry into a tagged release.
- Suggested backlog follow-ups:
  None beyond the already-tracked `FFI-006` and `FFI-007`.

## Next Handoff
Human: close `FFI-005` in the release checklist, carry `FFI-006` forward as the remaining C API release blocker, and decide the final release version/tag naming when promoting the staged changelog and release-note draft.
