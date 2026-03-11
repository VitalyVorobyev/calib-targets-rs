# Add C examples, C++ RAII wrapper, and ABI verification

- Task ID: `TASK-004-add-c-examples-cpp-raii-wrapper-and-abi-verification`
- Backlog ID: `FFI-004`
- Role: `reviewer`
- Date: `2026-03-11`
- Status: `complete`

## Inputs Consulted
- `docs/handoffs/TASK-004-add-c-examples-cpp-raii-wrapper-and-abi-verification/01-architect.md`
- `docs/handoffs/TASK-004-add-c-examples-cpp-raii-wrapper-and-abi-verification/02-implementer.md`
- `docs/backlog.md`
- `.github/workflows/ci.yml`
- `docs/ffi/README.md`
- `crates/calib-targets-ffi/examples/native_smoke_common.h`
- `crates/calib-targets-ffi/examples/chessboard_consumer_smoke.c`
- `crates/calib-targets-ffi/examples/chessboard_wrapper_smoke.cpp`
- `crates/calib-targets-ffi/include/calib_targets_ffi.hpp`
- `crates/calib-targets-ffi/tests/native_consumer_smoke.rs`
- `crates/calib-targets-ffi/include/calib_targets_ffi.h`
- `crates/calib-targets-ffi/src/lib.rs`

## Summary
`FFI-004` delivers the planned repo-owned native consumer path without widening the approved C ABI. The FFI crate now has a checked-in C smoke example, a thin header-only C++ RAII wrapper with a companion example, a native integration test that builds an isolated shared library and compiles/runs those examples, CI header-drift enforcement, and updated FFI usage documentation. I reproduced the highest-risk validation path directly: the generated-header check and the full `cargo test --workspace --all-targets` run, including the new external C/C++ consumer smoke test.

## Decisions Made
- Verdict: `approved_with_minor_followups`
- The integration-test-based native smoke harness is an acceptable implementation of the architect’s requested “dedicated FFI consumer smoke-validation command”; it keeps the external-consumer proof inside the normal Cargo test gate while still building and running real C and C++ callers.
- The wrapper remains layered above the generated C header and does not add new C ABI exports, which keeps `FFI-004` within the approved scope.

## Files/Modules Affected
- `.github/workflows/ci.yml`
- `docs/ffi/README.md`
- `crates/calib-targets-ffi/examples/native_smoke_common.h`
- `crates/calib-targets-ffi/examples/chessboard_consumer_smoke.c`
- `crates/calib-targets-ffi/examples/chessboard_wrapper_smoke.cpp`
- `crates/calib-targets-ffi/include/calib_targets_ffi.hpp`
- `crates/calib-targets-ffi/tests/native_consumer_smoke.rs`

## Validation/Tests
- Reproduced: `cargo run -p calib-targets-ffi --bin generate-ffi-header -- --check` — passed
- Reproduced: `cargo test -p calib-targets-ffi --test native_consumer_smoke -- --nocapture` — passed
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

## Risks/Open Questions
- The native smoke examples exercise only the chessboard path end to end. That is acceptable for this task because the wrapper header mirrors all three detector families and the underlying Rust FFI tests still cover ChArUco and marker-board ABI behavior, but broader native example coverage could still be added later if this surface grows.
- The C++ wrapper uses C++17 features such as nested namespace syntax and mutable `std::string::data()`, while the README currently documents build/link flow but not the minimum C++ language level or the Unix-like toolchain assumption exercised by the smoke harness.

## Role-Specific Details

### Reviewer
- Review scope:
  Architect acceptance criteria, implementer claims, the new native consumer examples and helper header, the header-only C++ wrapper, the Rust integration smoke harness, CI wiring, and the FFI documentation updates.
- Findings:
  1. No blocking findings.
  2. Minor follow-up: `calib_targets_ffi.hpp` currently assumes a C++17-capable compiler (`namespace calib_targets::ffi` and mutable `std::string::data()`), but the usage docs do not say that explicitly. If the wrapper is meant to be consumed beyond the repo’s current native smoke path, closeout should preserve that toolchain expectation as a documented residual requirement rather than leaving it implicit.
- Verdict:
  `approved_with_minor_followups`
- Required follow-up actions:
  1. Architect: write `04-architect.md`, record that `FFI-004` is approved, and carry forward the minor follow-up that the wrapper/toolchain expectations are still documented only implicitly by the example compile flags and header implementation.

## Next Handoff
Architect: write `docs/handoffs/TASK-004-add-c-examples-cpp-raii-wrapper-and-abi-verification/04-architect.md`, summarize the delivered native consumer coverage, and include the minor documentation/toolchain follow-up in the human-facing closeout.
