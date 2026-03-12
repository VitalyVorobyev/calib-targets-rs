# Scaffold calib-targets-ffi

- Task ID: `TASK-002-scaffold-calib-targets-ffi`
- Backlog ID: `FFI-002`
- Role: `implementer`
- Date: `2026-03-10`
- Status: `ready_for_review`

## Inputs Consulted
- `docs/handoffs/TASK-002-scaffold-calib-targets-ffi/01-architect.md`
- `docs/ffi/README.md`
- `docs/ffi/decision-record.md`
- `Cargo.toml`
- `crates/calib-targets/Cargo.toml`
- `crates/calib-targets-py/Cargo.toml`

## Summary
Added the new `calib-targets-ffi` workspace crate as the ABI scaffold above `calib-targets`, without introducing any detector-specific entry points yet. The crate now owns the shared status/error runtime, fixed optional-value conventions, the grayscale image descriptor, panic containment helpers, deterministic `cbindgen` configuration, an in-repo header generation/check binary, and the initial generated public header. The scaffold stays inside `FFI-002` scope and leaves detector handles/create/detect APIs for `FFI-003`.

## Decisions Made
- The checked-in header path is `crates/calib-targets-ffi/include/calib_targets_ffi.h`.
- Header generation/checking is self-contained in the workspace via:
  - `cargo run -p calib-targets-ffi --bin generate-ffi-header`
  - `cargo run -p calib-targets-ffi --bin generate-ffi-header -- --check`
- `ct_last_error_message` preserves the stored thread-local error message even when the retrieval call itself fails, so the query/copy pattern is stable.
- The scaffold includes `rlib` alongside `cdylib` to support Rust tests and the in-package header generator while keeping `cdylib` as the public packaging target.

## Files/Modules Affected
- `Cargo.toml`
- `Cargo.lock`
- `crates/calib-targets-ffi/Cargo.toml`
- `crates/calib-targets-ffi/cbindgen.toml`
- `crates/calib-targets-ffi/src/lib.rs`
- `crates/calib-targets-ffi/src/bin/generate-ffi-header.rs`
- `crates/calib-targets-ffi/include/calib_targets_ffi.h`

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
- `cargo run -p calib-targets-ffi --bin generate-ffi-header` — passed
- `cargo run -p calib-targets-ffi --bin generate-ffi-header -- --check` — passed

## Risks/Open Questions
- The scaffold intentionally stops short of detector-specific config/result structs and entry points; reviewer should verify no hidden detector ABI crept in.
- No dedicated C compiler smoke test was added at this stage. Rust-side ABI/runtime tests and header determinism are in place; C consumption should be exercised once detector APIs exist.
- `ct_gray_image_u8_t` validation exists now, but it is not yet exercised from exported detect calls until `FFI-003`.

## Role-Specific Details

### Implementer
- Checklist executed:
  - Added `crates/calib-targets-ffi` to the workspace.
  - Added shared ABI types/constants/status codes and error/runtime helpers.
  - Added the first exported functions: `ct_version_string` and `ct_last_error_message`.
  - Added unit tests for version/error/runtime behavior and image validation.
  - Added `cbindgen.toml`, in-repo header generator/check binary, and generated the checked-in header.
- Code/tests changed:
  - Shared scaffold only; no chessboard/ChArUco/marker-board detector APIs yet.
  - Added tests for last-error query/copy flow, panic containment, static version string, and image descriptor validation.
- Deviations from plan:
  - Used an in-package Rust binary with the `cbindgen` crate instead of assuming a globally installed `cbindgen` executable.
  - Bootstrapped `pip` in `.venv` and installed `pytest`, `pyright`, and `mypy` there so the required Python validation could run.
  - `pyright` required `--pythonpath .venv/bin/python` to resolve the editable package and venv-installed dependencies.
- Remaining follow-ups:
  - `FFI-003` should add detector handles/create/detect calls on top of these shared ABI primitives.
  - Add dedicated C/C++ smoke coverage once real detector entry points exist.

## Next Handoff
Reviewer: verify that the scaffold stays within `FFI-002` scope, that the header-generation/check workflow is deterministic and repo-local, and that the shared ABI runtime surface is a sound base for `FFI-003`.
