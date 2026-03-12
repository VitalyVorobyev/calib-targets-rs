# Publish native C/C++ release artifacts

- Task ID: `TASK-008-publish-native-c-cpp-release-artifacts`
- Backlog ID: `FFI-008`
- Role: `reviewer`
- Date: `2026-03-12`
- Status: `complete`

## Inputs Consulted
- `docs/handoffs/TASK-008-publish-native-c-cpp-release-artifacts/01-architect.md`
- `docs/handoffs/TASK-008-publish-native-c-cpp-release-artifacts/02-implementer.md`
- `crates/calib-targets-ffi/src/package_support.rs`
- `crates/calib-targets-ffi/src/bin/package-release-archive.rs`
- `crates/calib-targets-ffi/src/bin/stage-cmake-package.rs`
- `crates/calib-targets-ffi/tests/support/mod.rs`
- `crates/calib-targets-ffi/tests/support/cmake.rs`
- `crates/calib-targets-ffi/tests/cmake_consumer_smoke.rs`
- `crates/calib-targets-ffi/tests/release_archive_smoke.rs`
- `crates/calib-targets-ffi/examples/cmake_wrapper_consumer/CMakeLists.txt`
- `.github/workflows/release-native-ffi.yml`
- `README.md`
- `docs/ffi/README.md`
- `docs/ffi/cmake-consumer-quickstart.md`
- `docs/releases/ffi-c-api-release-draft.md`

## Summary
The reviewer-requested rework is correct and addresses the only blocking issue from the previous pass. The release-archive implementation still matches the architected staged-prefix contract, and the CMake-based smoke coverage now handles both single-config and multi-config generators by configuring an explicit build configuration, building with `--config`, and resolving the consumer executable from either the config subdirectory or the flat build directory. I reviewed the implementer’s full baseline evidence and reproduced the previously failing high-risk path directly with `CMAKE_GENERATOR=Xcode` for both CMake-based smoke tests.

## Decisions Made
- Clear the prior `changes_requested` verdict because the multi-config generator path now behaves correctly in both the staged-package and release-archive smoke tests.
- Treat the remaining unrun live GitHub-tag publish path as non-blocking because the architect explicitly allowed an equivalent safe release rehearsal, and the implementer/reviewer evidence now includes direct multi-config reproductions plus the full local baseline.

## Files/Modules Affected
- `crates/calib-targets-ffi/tests/support/cmake.rs`
- `crates/calib-targets-ffi/tests/cmake_consumer_smoke.rs`
- `crates/calib-targets-ffi/tests/release_archive_smoke.rs`
- `docs/handoffs/TASK-008-publish-native-c-cpp-release-artifacts/03-reviewer.md`

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
- Reproduced the previously blocking high-risk checks locally:
  - `CMAKE_GENERATOR=Xcode cargo test -p calib-targets-ffi --test cmake_consumer_smoke -- --nocapture` — passed
  - `CMAKE_GENERATOR=Xcode cargo test -p calib-targets-ffi --test release_archive_smoke -- --nocapture` — passed
- Reviewed implementer evidence for the explicit FFI commands:
  - `cargo run -p calib-targets-ffi --bin generate-ffi-header -- --check`
  - `cargo test -p calib-targets-ffi --test native_consumer_smoke -- --nocapture`
  - `cargo test -p calib-targets-ffi --test cmake_consumer_smoke -- --nocapture`
  - `cargo test -p calib-targets-ffi --test release_archive_smoke -- --nocapture`
- I did not reproduce a live GitHub tag workflow or Windows runner execution in this environment.

## Risks/Open Questions
- A live disposable tag run of `.github/workflows/release-native-ffi.yml` would still be useful release-process rehearsal, but it is no longer a review blocker because the multi-config failure mode was reproduced and cleared locally.
- The existing non-failing `cargo doc` filename-collision warning between `calib-targets` and `calib-targets-cli` remains outside `FFI-008` scope.

## Role-Specific Details

### Reviewer
- Review scope:
  Re-reviewed the implementation after the `changes_requested` handoff, focused on the CMake multi-config generator path, the new shared CMake smoke helper module, and the coherence of the updated validation evidence against the architect acceptance criteria.
- Findings:
  No blocking findings remain.
- Verdict:
  `approved`
- Required follow-up actions:
  1. Architect: write `docs/handoffs/TASK-008-publish-native-c-cpp-release-artifacts/04-architect.md`, summarize the approved native release-archive scope, and hand the task back to the human for final release/process acceptance.

## Next Handoff
Architect: close out `TASK-008-publish-native-c-cpp-release-artifacts`, record that `FFI-008` is approved, summarize the shipped native archive/release workflow scope, and ask the human to decide whether to run an actual disposable tag rehearsal before relying on the new release workflow for the next tagged release.
