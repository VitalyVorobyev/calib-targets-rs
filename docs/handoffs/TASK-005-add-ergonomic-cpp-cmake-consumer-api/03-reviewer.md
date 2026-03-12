# Add ergonomic C++ consumer packaging and CMake API

- Task ID: `TASK-005-add-ergonomic-cpp-cmake-consumer-api`
- Backlog ID: `FFI-007`
- Role: `reviewer`
- Date: `2026-03-12`
- Status: `complete`

## Inputs Consulted
- `docs/handoffs/TASK-005-add-ergonomic-cpp-cmake-consumer-api/01-architect.md`
- `docs/handoffs/TASK-005-add-ergonomic-cpp-cmake-consumer-api/02-implementer.md`
- `crates/calib-targets-ffi/src/bin/stage-cmake-package.rs`
- `crates/calib-targets-ffi/cmake/calib_targets_ffi-config.cmake.in`
- `crates/calib-targets-ffi/cmake/calib_targets_ffi-config-version.cmake.in`
- `crates/calib-targets-ffi/examples/cmake_wrapper_consumer/CMakeLists.txt`
- `crates/calib-targets-ffi/examples/cmake_wrapper_consumer/consumer_support.hpp`
- `crates/calib-targets-ffi/examples/cmake_wrapper_consumer/main.cpp`
- `crates/calib-targets-ffi/tests/cmake_consumer_smoke.rs`
- `crates/calib-targets-ffi/tests/native_consumer_smoke.rs`
- `docs/ffi/README.md`
- `README.md`
- `.github/workflows/ci.yml`

## Summary
`FFI-007` meets the architect scope. The implementation adds a repo-local staged CMake package on top of the existing Cargo-built shared library, exports a clean two-target CMake consumer surface without widening the C ABI, adds a repo-owned `find_package(...)` consumer example, and validates that flow with a dedicated smoke test while preserving the existing raw native smoke coverage. The docs and CI changes are aligned with the actual shipped behavior and stay conservative about support boundaries.

## Decisions Made
- Treat the staged package flow and the dedicated CMake smoke test as the highest-risk parts of the task and reproduce those directly.
- Accept the local helper duplication inside the CMake consumer example as an intentional boundary choice, because it keeps the consumer project independent from repo-internal smoke headers.
- Use implementer evidence for the broader Rust/docs/Python baseline after confirming it is coherent and consistent with the reproduced native checks.

## Files/Modules Affected
- `crates/calib-targets-ffi/src/bin/stage-cmake-package.rs`
- `crates/calib-targets-ffi/cmake/calib_targets_ffi-config.cmake.in`
- `crates/calib-targets-ffi/cmake/calib_targets_ffi-config-version.cmake.in`
- `crates/calib-targets-ffi/examples/cmake_wrapper_consumer/CMakeLists.txt`
- `crates/calib-targets-ffi/examples/cmake_wrapper_consumer/consumer_support.hpp`
- `crates/calib-targets-ffi/examples/cmake_wrapper_consumer/main.cpp`
- `crates/calib-targets-ffi/tests/cmake_consumer_smoke.rs`
- `crates/calib-targets-ffi/tests/native_consumer_smoke.rs`
- `docs/ffi/README.md`
- `README.md`
- `.github/workflows/ci.yml`

## Validation/Tests
- Reproduced: `cargo run -p calib-targets-ffi --bin stage-cmake-package -- --lib-dir target/debug --prefix /tmp/calib-targets-ffi-review-package` — passed
- Reproduced: staged package layout contains headers, shared library, and CMake config files under `/tmp/calib-targets-ffi-review-package` — passed
- Reproduced: `cargo run -p calib-targets-ffi --bin generate-ffi-header -- --check` — passed
- Reproduced: `cargo test -p calib-targets-ffi --test native_consumer_smoke -- --nocapture` — passed
- Reproduced: `cargo test -p calib-targets-ffi --test cmake_consumer_smoke -- --nocapture` — passed
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
- The packaged CMake flow is explicitly repo-local and conservatively documented. That matches the architect scope, but published/broader native packaging remains future work rather than something this review treats as solved.
- Linux CI coverage is explicit; Windows/MSVC remains best-effort and should stay that way in release messaging unless dedicated validation is added later.

## Role-Specific Details

### Reviewer
- Review scope:
  Compared the architect acceptance criteria against the staged package tool, generated CMake config, repo-owned consumer example, dedicated CMake smoke test, preserved raw native smoke test, and the native docs/CI updates.
- Findings:
  No findings.
- Verdict:
  `approved`
- Required follow-up actions:
  Architect should write `04-architect.md` to close out `FFI-007` and hand the completed post-release packaging work back to the human for backlog/release-state synthesis.

## Next Handoff
Architect: write `docs/handoffs/TASK-005-add-ergonomic-cpp-cmake-consumer-api/04-architect.md` to close the task, capture the approved staged CMake/package scope, and summarize any remaining post-task native packaging limits for the human handoff.
