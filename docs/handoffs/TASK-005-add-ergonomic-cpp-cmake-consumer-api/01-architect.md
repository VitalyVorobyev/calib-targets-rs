# Add ergonomic C++ consumer packaging and CMake API

- Task ID: `TASK-005-add-ergonomic-cpp-cmake-consumer-api`
- Backlog ID: `FFI-007`
- Role: `architect`
- Date: `2026-03-12`
- Status: `ready_for_implementer`

## Inputs Consulted
- `docs/backlog.md`
- `docs/handoffs/TASK-005-add-ergonomic-cpp-cmake-consumer-api/01-architect.md` (previous revision)
- `docs/handoffs/TASK-007-publish-c-api-readme-and-tutorials/04-architect.md`
- `README.md`
- `docs/ffi/README.md`
- `docs/releases/ffi-c-api-release-draft.md`
- `crates/calib-targets-ffi/Cargo.toml`
- `crates/calib-targets-ffi/include/calib_targets_ffi.hpp`
- `crates/calib-targets-ffi/tests/native_consumer_smoke.rs`
- `.github/workflows/ci.yml`

## Summary
The C API release blockers are now complete, so `FFI-007` is the next native-consumer task. The repo already ships a usable header-only C++17 wrapper and validates repo-local C/C++ consumers, but downstream CMake users still have to know Cargo’s output layout and hand-write include/link flags. This task should package the existing shared library and wrapper as a deterministic repo-produced CMake config package, add a repo-owned `find_package(...)` consumer example, and validate that flow in CI without widening or bypassing the approved C ABI.

## Decisions Made
- Reuse the existing `TASK-005-add-ergonomic-cpp-cmake-consumer-api` handoff directory and revise the plan now that the C API release/docs work is complete.
- Keep Cargo as the build authority for `calib-targets-ffi`; the new work should stage/package already-built artifacts for CMake consumption rather than teaching CMake to build the Rust crate itself.
- Preserve the current layering: one imported/shared-library C target for the C ABI and one interface/header-only C++ target above it. Do not add a second ABI surface or C++-only exported symbols.
- Treat Linux on the existing `ubuntu-latest` CI path as the required validation target for this task. Other native toolchains may remain best-effort until they have explicit CI coverage.

## Files/Modules Affected
- `crates/calib-targets-ffi/src/bin/` for a package-staging helper
- New CMake package templates under `crates/calib-targets-ffi/`
- `crates/calib-targets-ffi/include/calib_targets_ffi.hpp`
- New repo-owned CMake consumer example under `crates/calib-targets-ffi/`
- `crates/calib-targets-ffi/tests/`
- `.github/workflows/ci.yml`
- `docs/ffi/README.md`
- `README.md` if the native entry point needs a short CMake/package note

## Validation/Tests
- No implementation yet.
- Required validation for implementation is listed below.

## Risks/Open Questions
- The repo does not yet define a staged native package layout. This task must introduce one that is deterministic and relocatable enough for repo validation without implying crates.io publication or prebuilt binary distribution.
- The current docs say “no CMake package yet”; implementation must update that messaging carefully so it reflects the new repo-local packaged flow without overstating platform support.
- The minimum supported CMake version is not yet fixed. The implementation should choose the lowest version needed by the actual package/config features used and document it explicitly.
- If Windows/MSVC-specific packaging details diverge materially from the Linux CI path, keep them out of scope for this task and leave a concrete follow-up instead of guessing.

## Role-Specific Details

### Architect Planning
- Problem statement:
  The existing native story is good enough for a C API release but still poor for supported C++ consumers. A downstream CMake project currently has to know where Cargo wrote the shared library, which headers to include, and which linker/runtime-path flags to pass manually. That is avoidable friction now that the release-critical C API work is done.
- Scope:
  Add a deterministic staged package layout for the built `calib-targets-ffi` artifacts, generate a repo-local CMake config package with clean imported targets for the C library and header-only C++ wrapper, add a repo-owned `find_package(...)` consumer example, validate that flow in CI, and document the supported usage path.
- Out of scope:
  Any new C ABI exports, redesign of the C++ wrapper around exceptions, static-library support, package-manager metadata, prebuilt binaries, crates.io publication of `calib-targets-ffi`, Windows/MSVC guarantees beyond documented caveats, and any new detector capabilities unrelated to consumer packaging.
- Constraints:
  Preserve the approved C ABI and current ownership/error model; keep the C++ layer thin and header-only above `calib_targets_ffi.h`; keep Cargo as the source of truth for building the shared library; maintain the existing direct-compiler native smoke test instead of replacing it; and keep the implementation reviewable as one focused post-release change set.
- Assumptions:
  A repo-local `find_package(... CONFIG REQUIRED)` flow is the right ergonomic target for the first supported CMake integration.
  A staged prefix containing `include/`, `lib/`, and `lib/cmake/<package>/` is sufficient for the first implementation.
  The current C++17 wrapper semantics remain acceptable; this task improves packaging and consumption, not wrapper behavior.
- Implementation plan:
  1. Add a deterministic native package-staging flow.
     Introduce a small Rust helper under `crates/calib-targets-ffi/src/bin/` or an equivalently local mechanism that stages the already-built shared library, generated C header, C++ wrapper header, and CMake config files into a deterministic prefix. The generated package should be prefix-relative/relocatable enough for temp-dir smoke tests and CI, and should expose one imported C target plus one interface C++ wrapper target so consumers do not set include or link flags by hand.
  2. Add a repo-owned CMake consumer example and smoke test.
     Create a small CMake project under `crates/calib-targets-ffi/` that uses `find_package(...)` and the exported targets to build a wrapper-based consumer. Keep the example close to the existing chessboard happy path and one explicit error/status check. Add a dedicated integration test that builds `calib-targets-ffi`, stages the package, runs `cmake -S ... -B ...`, builds the example, and executes it against a known fixture. Keep `native_consumer_smoke.rs` as the direct compiler-level coverage for the raw headers/link flags path.
  3. Update docs and CI around the packaged flow.
     Update `docs/ffi/README.md` to document the package-generation command, prefix layout, minimum supported C++ standard, minimum supported CMake version, `find_package(...)` consumer steps, and current validation scope. Add a short README pointer if needed. Wire the new smoke path into CI on `ubuntu-latest`.
- Acceptance criteria:
  1. The repo can produce a deterministic staged native package containing the FFI shared library, generated C header, C++ wrapper header, and CMake package/config files.
  2. A repo-owned CMake consumer project builds through `find_package(... CONFIG REQUIRED)` and exported targets without handwritten include directories or linker flags in the consumer project.
  3. CI runs a dedicated package/CMake smoke validation on the existing Linux path and executes the produced consumer binary successfully.
  4. The existing raw C/C++ native smoke path continues to pass, and the underlying C ABI/header surface remains unchanged apart from non-breaking comments or documentation-related polish.
  5. The docs explicitly state the supported C++ language level, minimum CMake version, package-generation workflow, and any remaining platform limitations.
- Test plan:
  1. `cargo fmt`
  2. `cargo clippy --workspace --all-targets -- -D warnings`
  3. `cargo test --workspace --all-targets`
  4. `cargo run -p calib-targets-ffi --bin generate-ffi-header -- --check`
  5. `cargo test -p calib-targets-ffi --test native_consumer_smoke -- --nocapture`
  6. Add and run a dedicated CMake/package smoke command; the preferred shape is `cargo test -p calib-targets-ffi --test cmake_consumer_smoke -- --nocapture`

## Next Handoff
Implementer: treat `FFI-007` as the active post-release native task. Add the staged CMake package flow, repo-owned `find_package(...)` consumer example, dedicated smoke validation, and matching docs without widening the approved C ABI or replacing the existing raw native smoke coverage.
