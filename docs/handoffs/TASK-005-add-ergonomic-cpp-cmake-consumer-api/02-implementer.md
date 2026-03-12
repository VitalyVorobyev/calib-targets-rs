# Add ergonomic C++ consumer packaging and CMake API

- Task ID: `TASK-005-add-ergonomic-cpp-cmake-consumer-api`
- Backlog ID: `FFI-007`
- Role: `implementer`
- Date: `2026-03-12`
- Status: `ready_for_review`

## Inputs Consulted
- `docs/handoffs/TASK-005-add-ergonomic-cpp-cmake-consumer-api/01-architect.md`
- `README.md`
- `docs/ffi/README.md`
- `crates/calib-targets-ffi/Cargo.toml`
- `crates/calib-targets-ffi/include/calib_targets_ffi.hpp`
- `crates/calib-targets-ffi/examples/chessboard_wrapper_smoke.cpp`
- `crates/calib-targets-ffi/tests/native_consumer_smoke.rs`
- `.github/workflows/ci.yml`

## Summary
Implemented the post-release CMake consumer path for `calib-targets-ffi` without changing the approved C ABI. The FFI crate now includes a small staging binary that packages the generated headers, shared library, and CMake config files into a deterministic repo-local prefix; a repo-owned `find_package(...)` CMake example that consumes that package through exported targets; and a dedicated `cmake_consumer_smoke` integration test that configures, builds, and runs the example against staged artifacts. The existing raw compiler smoke coverage remains in place, and the native docs/README now describe both the direct include/link path and the new staged CMake flow.

## Decisions Made
- Implemented the package flow as a small repo-local Rust binary, `stage-cmake-package`, so Cargo remains the build authority and CMake only consumes staged artifacts.
- Exported a two-target CMake surface: `calib_targets_ffi::c` for the shared C ABI library and `calib_targets_ffi::cpp` for the header-only C++ wrapper.
- Kept the repo-owned CMake example independent from repo-internal smoke headers by giving it its own local helper header for PGM loading and config construction.
- Preserved the existing direct compiler smoke test and factored only the common Rust test harness helpers into shared support modules.

## Files/Modules Affected
- `crates/calib-targets-ffi/src/bin/stage-cmake-package.rs`
- `crates/calib-targets-ffi/cmake/calib_targets_ffi-config.cmake.in`
- `crates/calib-targets-ffi/cmake/calib_targets_ffi-config-version.cmake.in`
- `crates/calib-targets-ffi/examples/cmake_wrapper_consumer/CMakeLists.txt`
- `crates/calib-targets-ffi/examples/cmake_wrapper_consumer/consumer_support.hpp`
- `crates/calib-targets-ffi/examples/cmake_wrapper_consumer/main.cpp`
- `crates/calib-targets-ffi/tests/cmake_consumer_smoke.rs`
- `crates/calib-targets-ffi/tests/support/mod.rs`
- `crates/calib-targets-ffi/tests/support/native.rs`
- `crates/calib-targets-ffi/tests/native_consumer_smoke.rs`
- `docs/ffi/README.md`
- `README.md`
- `.github/workflows/ci.yml`

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
- `cargo test -p calib-targets-ffi --test cmake_consumer_smoke -- --nocapture` — passed

## Risks/Open Questions
- The staged CMake package is intentionally repo-local and conservative. It improves downstream ergonomics, but it does not yet imply system packaging, crates.io publication, or prebuilt binaries.
- CI coverage for the packaged CMake path is explicit on Linux. The implementation also passed locally on macOS during this turn, but the docs still describe Linux CI as the formally validated path.
- Windows/MSVC-specific package/import-library behavior was kept best-effort and is not claimed as fully validated here.

## Role-Specific Details

### Implementer
- Checklist executed:
  Added the staging binary and CMake templates, added the repo-owned CMake consumer example, added dedicated CMake smoke coverage and CI wiring, updated the native docs/README, and ran the full repo validation baseline plus the explicit FFI header/native smoke commands.
- Code/tests changed:
  `stage-cmake-package` stages a deterministic prefix with headers, shared library, and CMake config files. The new `cmake_wrapper_consumer` example consumes that prefix through `find_package(calib_targets_ffi CONFIG REQUIRED)` and exported targets, using only local consumer-side helpers plus public FFI headers. `cmake_consumer_smoke.rs` validates the staged package end to end, while `native_consumer_smoke.rs` remains the raw compiler/link coverage and now shares common harness utilities from `tests/support/`.
- Deviations from plan:
  No architectural scope deviations. The only notable implementation choice was to keep the CMake example self-contained by duplicating small consumer-side helper logic locally instead of depending on `examples/native_smoke_common.h`, which better matches the intended downstream package boundary.
- Remaining follow-ups:
  None required for review beyond normal scrutiny of the staged package layout, target naming, and docs wording.

## Next Handoff
Reviewer: verify that the staged CMake package and exported targets stay strictly above the existing C ABI, that the new `cmake_consumer_smoke` path meaningfully exercises the documented package flow, and that the docs/CI updates do not overstate current native packaging support.
