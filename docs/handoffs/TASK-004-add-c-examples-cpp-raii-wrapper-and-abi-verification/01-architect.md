# Add C examples, C++ RAII wrapper, and ABI verification

- Task ID: `TASK-004-add-c-examples-cpp-raii-wrapper-and-abi-verification`
- Backlog ID: `FFI-004`
- Role: `architect`
- Date: `2026-03-11`
- Status: `ready_for_implementer`

## Inputs Consulted
- `docs/backlog.md`
- `docs/ffi/README.md`
- `docs/ffi/decision-record.md`
- `docs/handoffs/TASK-003-add-conservative-detector-handles-and-detection-entry-points/01-architect.md`
- `docs/handoffs/TASK-003-add-conservative-detector-handles-and-detection-entry-points/02-implementer.md`
- `docs/handoffs/TASK-003-add-conservative-detector-handles-and-detection-entry-points/03-reviewer.md`
- `.github/workflows/ci.yml`
- `crates/calib-targets-ffi/Cargo.toml`
- `crates/calib-targets-ffi/src/lib.rs`
- `crates/calib-targets-ffi/include/calib_targets_ffi.h`

## Summary
`FFI-003` established the first usable detector ABI, but the repo still proves that surface only from Rust-side tests and header drift checks. `FFI-004` should harden the FFI for real consumers by adding repo-owned C and C++ usage paths, a thin RAII wrapper layered strictly on top of the generated C header, and automated external compile/smoke coverage in CI. The task should validate consumption and documentation without widening or redesigning the approved C ABI.

## Decisions Made
- `FFI-004` is a consumer-hardening task, not an ABI-expansion task: no new detector families, transport changes, or convenience exports should be added unless they are purely non-breaking documentation polish.
- The repo should own at least one C example and one thin C++ wrapper/example that exercise the generated header and built library directly rather than relying only on Rust FFI tests.
- The C++ layer should preserve explicit status-based error propagation on top of the C ABI instead of introducing exception-only behavior.

## Files/Modules Affected
- `crates/calib-targets-ffi/Cargo.toml`
- `crates/calib-targets-ffi/include/calib_targets_ffi.h`
- `crates/calib-targets-ffi/src/lib.rs` only if comments or small non-breaking support hooks are required; no new ABI surface is planned
- New example and/or wrapper files under `crates/calib-targets-ffi/`
- `.github/workflows/ci.yml`
- `docs/ffi/README.md`

## Validation/Tests
- No implementation yet.
- Required validation for implementation is listed below.

## Risks/Open Questions
- External compile/run smoke coverage on Linux will materially improve confidence, but it still will not prove every downstream toolchain; keep examples and wrapper code portable and compiler-agnostic.
- `calib-targets-ffi` is still `publish = false`, so "release/docs integration" should be interpreted as repo documentation and CI readiness, not crates.io publishing or installer packaging.
- Example execution should prefer deterministic lifecycle and error-path coverage plus a stable fixture-driven happy path, rather than brittle image-dependent behavior that may vary across environments.

## Role-Specific Details

### Architect Planning
- Problem statement:
  The C ABI is now large enough that Rust-only tests are no longer sufficient to catch consumer-facing regressions. The repo does not yet prove that the generated header and built library can be consumed cleanly from plain C or from the planned thin C++ layer, and ownership/error rules are still validated only indirectly.
- Scope:
  Add repo-owned C example coverage, a thin C++ RAII wrapper and example layered on the existing C ABI, automated native compile/smoke checks wired into repo validation, and the corresponding FFI usage/build documentation.
- Out of scope:
  New C ABI exports or contract redesign, non-grayscale inputs, custom dictionaries, Windows/macOS-specific packaging work, crates.io publishing changes, prebuilt binary distribution, and any release automation beyond CI/docs hardening for the existing unpublished FFI crate.
- Constraints:
  Keep the underlying C ABI fixed and compatible with `FFI-003`; wrapper code must consume only the generated public header; preserve explicit ownership and last-error rules; prefer deterministic smoke coverage that can run in CI; keep diffs localized to the FFI crate, docs, and CI wiring.
- Assumptions:
  Ubuntu CI runners can compile both C and C++ smoke consumers with standard system toolchains.
  Existing fixtures or deterministic no-detection flows are sufficient for smoke coverage without inventing new fragile fixture-generation logic.
  A status-oriented C++ wrapper is acceptable for v1 and better aligned with the repo's correctness-first posture than exception-only behavior.
- Implementation plan:
  1. Add consumer-facing sources and define the wrapper shape.
     Introduce at least one checked-in C example that exercises version/error retrieval plus one detector create/query/fill/destroy flow against the generated header. Add a thin C++ RAII wrapper plus a small example on top of the same C ABI, keeping ownership one-to-one with the C handles and avoiding alternate transport layers or new exports.
  2. Add automated external compile and smoke execution.
     Add a repo-owned smoke harness that regenerates/checks the header, builds the FFI library, compiles the C and C++ examples against that header/library, and runs them in CI. Include external-caller checks for at least one failure path such as not-found, short-buffer, or error-message retrieval so the consumer contract is exercised outside Rust unit tests.
  3. Document the supported consumer workflow.
     Update the FFI docs with build/link instructions, wrapper usage, ownership/error conventions, and the smoke-validation entry point that future ABI changes must keep passing.
- Acceptance criteria:
  1. The repo contains a checked-in C example that compiles against `calib_targets_ffi.h`, links against the built library, and exercises version/error handling plus one detector lifecycle/query-fill path.
  2. The repo contains a thin C++ RAII wrapper and example that manage detector handles safely, preserve explicit status-based error propagation, and do not require any new C ABI exports.
  3. CI runs automated external C/C++ compile/smoke validation for the generated header/library and fails on header drift or consumer build/runtime regressions.
  4. `docs/ffi/README.md` explains how to build, link, and use the C API and wrapper, including ownership and last-error retrieval rules.
  5. The approved `FFI-003` C ABI remains backward-compatible from the perspective of the checked-in header and exported symbol surface.
- Test plan:
  1. `cargo fmt`
  2. `cargo clippy --workspace --all-targets -- -D warnings`
  3. `cargo test --workspace`
  4. `cargo run -p calib-targets-ffi --bin generate-ffi-header -- --check`
  5. Add a dedicated FFI consumer smoke-validation command and wire it into CI; a reasonable target is `cargo test -p calib-targets-ffi --test native_consumer_smoke -- --nocapture` if the harness is implemented as an integration test.

## Next Handoff
Implementer: add the C example, thin status-oriented C++ wrapper/example, and automated native consumer smoke coverage without widening the approved C ABI, then document the exact local/CI validation entry point in the implementation handoff.
