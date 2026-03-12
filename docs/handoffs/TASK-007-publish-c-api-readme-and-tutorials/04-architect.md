# Publish C API README and concise tutorials

- Task ID: `TASK-007-publish-c-api-readme-and-tutorials`
- Backlog ID: `FFI-006`
- Role: `architect`
- Date: `2026-03-11`
- Status: `ready_for_human`

## Inputs Consulted
- `docs/handoffs/TASK-007-publish-c-api-readme-and-tutorials/01-architect.md`
- `docs/handoffs/TASK-007-publish-c-api-readme-and-tutorials/02-implementer.md`
- `docs/handoffs/TASK-007-publish-c-api-readme-and-tutorials/03-reviewer.md`
- `docs/backlog.md`
- `README.md`
- `docs/ffi/README.md`

## Summary
`FFI-006` is complete. The repo now has release-facing native documentation in the two places a downstream user will actually start: the top-level README points native consumers at the C API, and `docs/ffi/README.md` is now a concise user guide rather than an internal planning document. Reviewer approved the task without findings and confirmed that the docs stay aligned to the current repo-local FFI surface without overpromising packaging or support.

## Decisions Made
- `FFI-006` should be closed in the backlog as complete.
- The current C API release blocker set is now satisfied; the remaining planned native work is post-release `FFI-007`, not a blocker for shipping the current C API.

## Files/Modules Affected
- `README.md`
- `docs/ffi/README.md`
- `docs/handoffs/TASK-007-publish-c-api-readme-and-tutorials/04-architect.md`

## Validation/Tests
- Reviewed reviewer evidence: `mdbook build book` — passed
- Reviewed reviewer evidence: `cargo run -p calib-targets-ffi --bin generate-ffi-header -- --check` — passed
- Reviewed reviewer evidence: `cargo test -p calib-targets-ffi --test native_consumer_smoke -- --nocapture` — passed
- Reviewed implementer evidence: `cargo fmt --all --check` — passed
- Reviewed implementer evidence: `cargo clippy --workspace --all-targets -- -D warnings` — passed
- Reviewed implementer evidence: `cargo test --workspace --all-targets` — passed
- Reviewed implementer evidence: `cargo doc --workspace --all-features --no-deps` — passed
- Reviewed implementer evidence: `.venv/bin/python crates/calib-targets-py/tools/generate_typing_artifacts.py --check` — passed
- Reviewed implementer evidence: `.venv/bin/python -m maturin develop -m crates/calib-targets-py/Cargo.toml` — passed
- Reviewed implementer evidence: `.venv/bin/python -m pytest crates/calib-targets-py/python_tests` — passed
- Reviewed implementer evidence: `.venv/bin/python -m pyright --pythonpath .venv/bin/python crates/calib-targets-py/python_tests/typecheck_smoke.py` — passed
- Reviewed implementer evidence: `.venv/bin/python -m mypy crates/calib-targets-py/python_tests/typecheck_smoke.py` — passed

## Risks/Open Questions
- The user-facing docs now cover the current repo-local native flow clearly enough to ship, but they still intentionally stop short of a packaged C++/CMake integration story. That remains the planned post-release scope in `FFI-007`.

## Role-Specific Details

### Architect Closeout
- Delivered scope:
  Added a native/C API entry point to the workspace README and rewrote `docs/ffi/README.md` into a release-facing guide with support boundaries, build/link commands, ownership/error rules, query/fill documentation, and concise C/C++ tutorials tied to the shipped examples.
- Reviewer verdict incorporated:
  `approved`; no review findings remain.
- Human decision requested:
  Accept `FFI-006` as complete and treat the current C API release docs set as sufficient to ship. Keep `FFI-007` as planned post-release ergonomic packaging work rather than a blocker for the current release.
- Suggested backlog follow-ups:
  None beyond the already-tracked `FFI-007`.

## Next Handoff
Human: close `FFI-006` in the release checklist, treat the current C API docs set as release-ready, and decide when to start the post-release `FFI-007` C++/CMake packaging work.
