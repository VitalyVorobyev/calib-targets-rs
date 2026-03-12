# Publish C API README and concise tutorials

- Task ID: `TASK-007-publish-c-api-readme-and-tutorials`
- Backlog ID: `FFI-006`
- Role: `reviewer`
- Date: `2026-03-11`
- Status: `complete`

## Inputs Consulted
- `docs/handoffs/TASK-007-publish-c-api-readme-and-tutorials/01-architect.md`
- `docs/handoffs/TASK-007-publish-c-api-readme-and-tutorials/02-implementer.md`
- `README.md`
- `docs/ffi/README.md`
- `docs/ffi/decision-record.md`
- `docs/releases/ffi-c-api-release-draft.md`
- `crates/calib-targets-ffi/include/calib_targets_ffi.h`
- `crates/calib-targets-ffi/include/calib_targets_ffi.hpp`
- `crates/calib-targets-ffi/examples/chessboard_consumer_smoke.c`
- `crates/calib-targets-ffi/examples/chessboard_wrapper_smoke.cpp`
- `crates/calib-targets-ffi/examples/native_smoke_common.h`
- `crates/calib-targets-ffi/tests/native_consumer_smoke.rs`

## Summary
`FFI-006` converts the native docs from an internal planning document into actual release-facing entry points. The top-level README now points native consumers at the C API, and `docs/ffi/README.md` now covers the shipped support boundaries, build/link flow, ownership and error handling, the query/fill model, and short C/C++ tutorials tied to the repo-owned example programs. I reproduced the highest-risk validation path directly: the docs build, the header-drift check, and the native consumer smoke test that the guide now recommends to readers.

## Decisions Made
- Verdict: `approved`
- The guide is appropriately conservative about the current repo-local native surface and does not overpromise packaging, installation, or ABI scope.
- Keeping the tutorial snippets anchored to the shipped repo examples is acceptable because the guide explicitly distinguishes the public ABI from the repo-local helper header used by those examples.

## Files/Modules Affected
- `README.md`
- `docs/ffi/README.md`
- `docs/handoffs/TASK-007-publish-c-api-readme-and-tutorials/03-reviewer.md`

## Validation/Tests
- Reproduced: `mdbook build book` — passed
- Reproduced: `cargo run -p calib-targets-ffi --bin generate-ffi-header -- --check` — passed
- Reproduced: `cargo test -p calib-targets-ffi --test native_consumer_smoke -- --nocapture` — passed
- Reviewed implementer evidence only: `cargo fmt --all --check`
- Reviewed implementer evidence only: `cargo clippy --workspace --all-targets -- -D warnings`
- Reviewed implementer evidence only: `cargo test --workspace --all-targets`
- Reviewed implementer evidence only: `cargo doc --workspace --all-features --no-deps`
- Reviewed implementer evidence only: `.venv/bin/python crates/calib-targets-py/tools/generate_typing_artifacts.py --check`
- Reviewed implementer evidence only: `.venv/bin/python -m maturin develop -m crates/calib-targets-py/Cargo.toml`
- Reviewed implementer evidence only: `.venv/bin/python -m pytest crates/calib-targets-py/python_tests`
- Reviewed implementer evidence only: `.venv/bin/python -m pyright --pythonpath .venv/bin/python crates/calib-targets-py/python_tests/typecheck_smoke.py`
- Reviewed implementer evidence only: `.venv/bin/python -m mypy crates/calib-targets-py/python_tests/typecheck_smoke.py`

## Risks/Open Questions
- No blocking risks remain within `FFI-006` scope. The guide is explicit that the currently documented native integration path is the repo-local Cargo-built flow rather than a cross-platform packaging story.

## Role-Specific Details

### Reviewer
- Review scope:
  Architect acceptance criteria, implementer claims, the rewritten top-level README native entry point, the new C API guide, and the reproduced validation commands tied to that guide.
- Findings:
  1. No findings.
- Verdict:
  `approved`
- Required follow-up actions:
  1. Architect: write `04-architect.md`, preserve that `FFI-006` is approved, and close out the remaining human release decision around whether the C API docs set is now sufficient to ship.

## Next Handoff
Architect: write `docs/handoffs/TASK-007-publish-c-api-readme-and-tutorials/04-architect.md`, summarize the approved C API documentation scope, and ask the human to decide whether the release now has enough native-consumer documentation to proceed.
