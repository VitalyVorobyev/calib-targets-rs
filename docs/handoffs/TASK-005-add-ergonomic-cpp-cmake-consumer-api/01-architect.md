# Add ergonomic C++ consumer packaging and CMake API

- Task ID: `TASK-005-add-ergonomic-cpp-cmake-consumer-api`
- Backlog ID: `FFI-007`
- Role: `architect`
- Date: `2026-03-11`
- Status: `ready_for_implementer`

## Inputs Consulted
- `docs/backlog.md`
- `CHANGELOG.md`
- `README.md`
- `docs/ffi/README.md`
- `crates/calib-targets-ffi/Cargo.toml`
- `crates/calib-targets-ffi/include/calib_targets_ffi.hpp`
- `crates/calib-targets-ffi/examples/chessboard_consumer_smoke.c`
- `crates/calib-targets-ffi/examples/chessboard_wrapper_smoke.cpp`
- `crates/calib-targets-ffi/tests/native_consumer_smoke.rs`

## Summary
The repo now has a usable C ABI and a thin header-only C++ wrapper, but downstream C++ consumers still need repo-specific manual include and link flags. That is acceptable for the next release because the immediate release goal is a good C API, not full native packaging ergonomics. `FFI-007` should land immediately after that release and package the existing wrapper plus shared library as a first-class CMake consumer surface without widening or bypassing the approved C ABI.

## Decisions Made
- Treat the existing `calib_targets_ffi.hpp` wrapper as the semantic baseline; improve ergonomics around packaging and consumption rather than redesigning the wrapper around exceptions or alternate ownership rules.
- Use Cargo as the source of truth for building the Rust shared library, then expose the built artifacts to CMake through generated package/config targets instead of teaching CMake to rebuild the Rust crate independently.
- Keep the C++ layer above the C ABI as a header-only wrapper plus imported shared-library target; do not add C++-only exported symbols or a second ABI surface.

## Files/Modules Affected
- `crates/calib-targets-ffi/include/calib_targets_ffi.hpp`
- `crates/calib-targets-ffi/include/calib_targets_ffi.h`
- New staging/package metadata under `crates/calib-targets-ffi/` for CMake config generation
- New repo-owned CMake consumer example under `crates/calib-targets-ffi/`
- `crates/calib-targets-ffi/tests/`
- `.github/workflows/ci.yml`
- `docs/ffi/README.md`
- `README.md` if top-level C++/CMake navigation is added

## Validation/Tests
- No implementation yet.
- Required validation for implementation is listed below.

## Risks/Open Questions
- The repo does not yet define a native packaging layout for release artifacts. This task should introduce one that works for repo validation first and can later back published release assets if needed.
- The existing wrapper and smoke test already imply a C++17 baseline. The implementation should document the exact minimum C++ standard and minimum supported CMake version instead of leaving them implicit.
- Windows/MSVC packaging may need separate follow-up if the first implementation focuses on the same Unix-like toolchain path currently covered by native smoke tests.

## Role-Specific Details

### Architect Planning
- Problem statement:
  The current C++ wrapper is only ergonomic for repo-local development. Consumers still have to know where Cargo puts the shared library, which include directories to add, and how to link the wrapper manually. That is too much friction for a supported native consumer path, but it is post-release work because the current release is explicitly focused on the C API itself.
- Scope:
  Add a deterministic package/staging layout for the built FFI library and headers, generate a CMake config/package that exposes clean imported targets for the C library and header-only C++ wrapper, add a repo-owned CMake consumer example plus smoke validation, and document the workflow.
- Out of scope:
  Any new C ABI exports, exception-only wrapper redesign, static-library support, custom dictionary support, prebuilt binary distribution, Windows-specific packaging guarantees beyond what can be validated in CI today, and the current-release docs/release-note blockers in `FFI-005` and `FFI-006`.
- Constraints:
  Preserve the approved C ABI and current ownership/error model; keep Cargo as the authoritative build path for the shared library; keep the C++ layer thin and header-only above the generated C header; do not delay the current C API release with this work.
- Assumptions:
  A `find_package(...)` CMake consumer flow is the right ergonomic target for post-release support.
  A staged package layout produced from `cargo build -p calib-targets-ffi` is sufficient for the first version.
  The wrapper remains status-oriented rather than exception-only unless a later human decision explicitly changes that direction.
- Implementation plan:
  1. Define the package layout and exported CMake targets.
     Add a deterministic staging/install layout that contains the shared library, generated C header, C++ wrapper header, and generated CMake package files. Expose at least one imported target for the C library and one interface target for the C++ wrapper so downstream code does not need manual include/link flags.
  2. Add a repo-owned CMake consumer path.
     Create a small CMake example that uses `find_package(...)` and the exported targets to build and run a wrapper-based detector flow against the staged artifacts. Keep the example focused on the already-shipped chessboard happy path plus one checked error/status path where practical.
  3. Add validation and documentation.
     Wire a local/CI smoke command that stages the package, configures the CMake example, builds it, and runs it. Document the package generation flow, minimum C++/CMake requirements, and the exact consumer steps in `docs/ffi/README.md`, with a short top-level pointer if the workspace README gains a native section.
- Acceptance criteria:
  1. A deterministic package/staging layout can be produced from the repo and contains the FFI shared library, generated C header, C++ wrapper header, and CMake package files.
  2. A repo-owned CMake example builds through `find_package(...)` and exported targets without handwritten include directories or linker flags in the consumer project.
  3. Local/CI validation configures, builds, and runs the CMake consumer example against the staged package.
  4. The underlying C ABI and generated C header remain backward-compatible aside from non-breaking comments or docs polish.
  5. Documentation states the minimum supported C++ language level and CMake version for the package flow.
- Test plan:
  1. `cargo fmt`
  2. `cargo clippy --workspace --all-targets -- -D warnings`
  3. `cargo test --workspace`
  4. `cargo run -p calib-targets-ffi --bin generate-ffi-header -- --check`
  5. Add a dedicated package/CMake smoke-validation command and record it in CI; a reasonable shape is a Rust integration test or scripted check that stages artifacts, runs `cmake -S ... -B ...`, `cmake --build ...`, and then executes the produced example binary.

## Next Handoff
Implementer: start this only after the current C API release blockers (`FFI-005` and `FFI-006`) are complete, then add the staged CMake package flow, repo-owned consumer example, validation wiring, and docs without widening the C ABI.
