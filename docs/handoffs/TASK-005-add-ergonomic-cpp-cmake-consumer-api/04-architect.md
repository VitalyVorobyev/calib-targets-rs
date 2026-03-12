# Add ergonomic C++ consumer packaging and CMake API

- Task ID: `TASK-005-add-ergonomic-cpp-cmake-consumer-api`
- Backlog ID: `FFI-007`
- Role: `architect`
- Date: `2026-03-12`
- Status: `ready_for_human`

## Inputs Consulted
- `docs/handoffs/TASK-005-add-ergonomic-cpp-cmake-consumer-api/01-architect.md`
- `docs/handoffs/TASK-005-add-ergonomic-cpp-cmake-consumer-api/02-implementer.md`
- `docs/handoffs/TASK-005-add-ergonomic-cpp-cmake-consumer-api/03-reviewer.md`
- `docs/backlog.md`
- `CHANGELOG.md`
- `README.md`
- `docs/ffi/README.md`

## Summary
`FFI-007` is complete. The repo now has a post-release native packaging story that stays within the approved layering: Cargo still builds the shared library, a repo-local staging tool packages the generated headers and library into a deterministic CMake prefix, and downstream C++ consumers can integrate through exported CMake targets rather than hand-written include/link flags. Reviewer approved the implementation without findings and reproduced the high-risk packaging and native smoke checks directly.

## Decisions Made
- `FFI-007` should be closed in the backlog as complete.
- The `Unreleased` changelog entry should now include the staged CMake/package work and drop the earlier claim that no CMake packaging exists.
- Remaining native support limits should stay explicit: the package is repo-local, Linux CI coverage is the formal validation path, and published/prebuilt distribution remains future work.

## Files/Modules Affected
- `CHANGELOG.md`
- `docs/backlog.md`
- `docs/handoffs/TASK-005-add-ergonomic-cpp-cmake-consumer-api/04-architect.md`

## Validation/Tests
- Reviewed reviewer evidence: `cargo run -p calib-targets-ffi --bin stage-cmake-package -- --lib-dir target/debug --prefix /tmp/calib-targets-ffi-review-package` — passed
- Reviewed reviewer evidence: staged package layout contains headers, shared library, and CMake config files — passed
- Reviewed reviewer evidence: `cargo run -p calib-targets-ffi --bin generate-ffi-header -- --check` — passed
- Reviewed reviewer evidence: `cargo test -p calib-targets-ffi --test native_consumer_smoke -- --nocapture` — passed
- Reviewed reviewer evidence: `cargo test -p calib-targets-ffi --test cmake_consumer_smoke -- --nocapture` — passed
- Reviewed implementer evidence: `cargo fmt --all --check` — passed
- Reviewed implementer evidence: `cargo clippy --workspace --all-targets -- -D warnings` — passed
- Reviewed implementer evidence: `cargo test --workspace --all-targets` — passed
- Reviewed implementer evidence: `cargo doc --workspace --all-features --no-deps` — passed
- Reviewed implementer evidence: `mdbook build book` — passed
- Reviewed implementer evidence: `.venv/bin/python crates/calib-targets-py/tools/generate_typing_artifacts.py --check` — passed
- Reviewed implementer evidence: `.venv/bin/python -m maturin develop -m crates/calib-targets-py/Cargo.toml` — passed
- Reviewed implementer evidence: `.venv/bin/python -m pytest crates/calib-targets-py/python_tests` — passed
- Reviewed implementer evidence: `.venv/bin/python -m pyright --pythonpath .venv/bin/python crates/calib-targets-py/python_tests/typecheck_smoke.py` — passed
- Reviewed implementer evidence: `.venv/bin/python -m mypy crates/calib-targets-py/python_tests/typecheck_smoke.py` — passed

## Risks/Open Questions
- The new CMake/package flow is good enough for repo-supported native consumers, but it is still intentionally not a published install/distribution story.
- Windows/MSVC packaging remains best-effort and should stay out of stronger support claims until there is explicit validation coverage.

## Role-Specific Details

### Architect Closeout
- Delivered scope:
  Added a repo-local package staging tool, exported CMake targets for the C ABI and header-only C++ wrapper, a repo-owned `find_package(...)` consumer example, dedicated CMake smoke validation, and matching native docs/CI updates without widening the approved C ABI.
- Reviewer verdict incorporated:
  `approved`; no review findings remain.
- Human decision requested:
  Accept `FFI-007` as complete, close the native post-release packaging follow-up in the backlog, and decide whether the next release should ship with the updated repo-local CMake consumer guidance reflected in the current `Unreleased` notes.
- Suggested backlog follow-ups:
  None required immediately. If broader native distribution becomes a priority later, track it as a new task rather than extending `FFI-007`.

## Next Handoff
Human: treat `FFI-007` as complete, use the updated `Unreleased` changelog text for release planning, and decide whether any new native-distribution work should be added as a separate backlog item.
