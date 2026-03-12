# Add C examples, C++ RAII wrapper, and ABI verification

- Task ID: `TASK-004-add-c-examples-cpp-raii-wrapper-and-abi-verification`
- Backlog ID: `FFI-004`
- Role: `implementer`
- Date: `2026-03-11`
- Status: `ready_for_review`

## Inputs Consulted
- `docs/handoffs/TASK-004-add-c-examples-cpp-raii-wrapper-and-abi-verification/01-architect.md`
- `docs/ffi/README.md`
- `.github/workflows/ci.yml`
- `crates/calib-targets-ffi/Cargo.toml`
- `crates/calib-targets-ffi/include/calib_targets_ffi.h`
- `crates/calib-targets-ffi/src/lib.rs`

## Summary
Implemented repo-owned native consumer coverage for the approved FFI surface without widening the C ABI. The FFI crate now includes checked-in C and C++ smoke examples, a header-only C++ RAII wrapper layered over the generated C header, a Rust integration test that builds an isolated shared library and compiles/runs the native examples, a CI header-drift check, and updated FFI docs describing the local consumer-validation entry points. No Rust ABI exports or fixed-layout C structs changed for this task.

## Decisions Made
- The native smoke path is implemented as a Rust integration test (`native_consumer_smoke`) that builds a private `calib-targets-ffi` cdylib in an isolated target directory, then compiles and runs the checked-in C and C++ examples against that artifact.
- The C++ layer is header-only and status-oriented: it wraps the existing handle lifecycle and query/fill calls, returns explicit `Status` values with `ct_last_error_message` text, and does not introduce exception-only control flow or new C ABI exports.
- The plain C and C++ smoke examples both exercise the chessboard detector path, while the wrapper header itself mirrors all three detector-handle families.

## Files/Modules Affected
- `.github/workflows/ci.yml`
- `crates/calib-targets-ffi/examples/native_smoke_common.h`
- `crates/calib-targets-ffi/examples/chessboard_consumer_smoke.c`
- `crates/calib-targets-ffi/examples/chessboard_wrapper_smoke.cpp`
- `crates/calib-targets-ffi/include/calib_targets_ffi.hpp`
- `crates/calib-targets-ffi/tests/native_consumer_smoke.rs`
- `docs/ffi/README.md`

## Validation/Tests
- `cargo fmt --all --check` — passed
- `cargo clippy --workspace --all-targets -- -D warnings` — passed
- `cargo test --workspace --all-targets` — passed
- `cargo doc --workspace --all-features --no-deps` — passed
- `mdbook build book` — passed
- `.venv/bin/python crates/calib-targets-py/tools/generate_typing_artifacts.py --check` — passed
- `.venv/bin/python -m maturin develop -m crates/calib-targets-py/Cargo.toml` — passed
- `.venv/bin/python -m pytest crates/calib-targets-py/python_tests` — passed
- `.venv/bin/python -m pyright --pythonpath .venv/bin/python crates/calib-targets-py/python_tests/typecheck_smoke.py` — passed
- `.venv/bin/python -m mypy crates/calib-targets-py/python_tests/typecheck_smoke.py` — passed
- `cargo run -p calib-targets-ffi --bin generate-ffi-header -- --check` — passed
- `cargo test -p calib-targets-ffi --test native_consumer_smoke -- --nocapture` — passed

## Risks/Open Questions
- The repo-owned native smoke path currently proves one end-to-end external consumer flow through the chessboard detector. ChArUco and marker-board external compile/runtime behavior are still covered indirectly via Rust FFI tests and the C++ wrapper surface, not by separate native example executables.
- The integration test assumes a standard native toolchain (`cc` and `c++` or compatible fallbacks) is available. That matches current local and Ubuntu CI assumptions, but broader downstream toolchain coverage remains out of scope for this task.

## Role-Specific Details

### Implementer
- Checklist executed:
  - Added a shared native helper header for PGM loading and conservative chessboard config construction used by the C and C++ smoke examples.
  - Added a plain C smoke example that exercises version retrieval, invalid-call error capture, chessboard query/fill detection, and short-buffer handling.
  - Added a header-only C++ wrapper with RAII handle ownership and status-based detect APIs for chessboard, ChArUco, and marker-board detector handles.
  - Added a C++ smoke example that uses the wrapper on the chessboard path and verifies safe move semantics.
  - Added a Rust integration test that builds an isolated `calib-targets-ffi` shared library, converts `testdata/mid.png` into a temporary binary PGM fixture, compiles the native examples against the generated header, and runs them.
  - Added CI header-drift checking and documented the native validation workflow in `docs/ffi/README.md`.
- Code/tests changed:
  - The FFI crate now owns repo-checked native consumer artifacts under `examples/`, `include/`, and `tests/`.
  - CI now fails fast on generated-header drift before the workspace test run.
  - No changes were required in `crates/calib-targets-ffi/src/lib.rs`; the approved `FFI-003` ABI surface remained unchanged.
- Deviations from plan:
  - The dedicated native smoke command was implemented as `cargo test -p calib-targets-ffi --test native_consumer_smoke -- --nocapture` instead of a standalone script or bin. This keeps the external consumer validation inside normal Cargo test workflows and the existing CI `cargo test --workspace --all-targets` gate.
- Remaining follow-ups:
  - Reviewer should confirm that the chessboard-focused native smoke examples are sufficient coverage for `FFI-004` given that the wrapper header exposes all three detector families.
  - Broader native packaging and additional downstream toolchain coverage remain future work if/when the FFI crate moves beyond repo-local consumption.

## Next Handoff
Reviewer: verify that the native smoke examples and integration harness genuinely prove external consumption of the checked-in header/shared library, that the C++ wrapper preserves explicit status/error semantics without widening the ABI, and that the docs/CI changes are sufficient to keep this path from regressing.
