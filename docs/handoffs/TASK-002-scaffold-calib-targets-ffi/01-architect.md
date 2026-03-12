# Scaffold calib-targets-ffi

- Task ID: `TASK-002-scaffold-calib-targets-ffi`
- Backlog ID: `FFI-002`
- Role: `architect`
- Date: `2026-03-10`
- Status: `ready_for_implementer`

## Inputs Consulted
- `docs/backlog.md`
- `docs/ffi/README.md`
- `docs/ffi/decision-record.md`
- `docs/handoffs/TASK-001-plan-c-ffi-crate/01-architect.md`
- `Cargo.toml`
- `crates/calib-targets/Cargo.toml`
- `crates/calib-targets-py/Cargo.toml`

## Summary
The first implementation task is to establish the `calib-targets-ffi` crate as a stable ABI shell above the existing facade crate. This task should build the shared runtime and header-generation foundation without yet exposing the full detector API surface. The output must be small, deterministic, and safe enough for later detector-specific ABI work to build on directly.

## Decisions Made
- `crates/calib-targets-ffi` is a dedicated workspace crate layered on top of `calib-targets`.
- Initial output format is `cdylib`.
- This task owns shared ABI/runtime primitives only: status codes, version/error functions, image descriptor, optional-value conventions, crate layout, and `cbindgen` integration.
- Detector-specific create/detect APIs are deferred to `FFI-003`.

## Files/Modules Affected
- `Cargo.toml`
- `crates/calib-targets-ffi/Cargo.toml`
- `crates/calib-targets-ffi/src/lib.rs`
- `crates/calib-targets-ffi/cbindgen.toml`
- Expected later: generated header path and header-check script/docs

## Validation/Tests
- No implementation yet.
- Required validation for implementation is listed below.

## Risks/Open Questions
- The main risk is overbuilding the scaffold task and smuggling detector-specific ABI shape into it. Keep `FFI-002` focused on shared infrastructure.
- Header generation must be deterministic from the beginning; otherwise later FFI tasks will accumulate churn.

## Role-Specific Details

### Architect Planning
- Problem statement:
  Introduce a new FFI crate and ABI runtime layer without locking in detector-specific mistakes or mixing ABI concerns into the existing Rust crates.
- Scope:
  Add the workspace crate, `cdylib` configuration, `cbindgen`, status/error plumbing, shared image/input primitives, and the first stable header skeleton.
- Out of scope:
  Chessboard/ChArUco/marker-board detector handles and detect calls; detector-specific config/result structs beyond what is strictly required as shared primitives.
- Constraints:
  C ABI only; no panics across FFI; explicit ownership rules; caller-owned buffers where practical; deterministic header generation; keep the scaffold small and reusable.
- Assumptions:
  `calib-targets` remains the only Rust dependency surface needed for the FFI crate.
- Implementation plan:
  1. Add `crates/calib-targets-ffi` to the workspace with `cdylib` crate type and minimal dependencies.
  2. Define the shared ABI runtime surface:
     `ct_status_t`, version function, last-error function, grayscale image descriptor, and optional-value helper conventions.
  3. Add panic containment and internal error capture helpers used by all exported functions.
  4. Add `cbindgen` config and one deterministic header-generation/check workflow.
  5. Document ownership/error rules close to the ABI definitions.
- Acceptance criteria:
  1. Workspace builds with the new FFI crate present.
  2. `cbindgen` emits a deterministic public header for the scaffolded ABI types/functions.
  3. Exported functions never panic across the FFI boundary.
  4. Shared ABI primitives are sufficient for `FFI-003` to add detector APIs without redesigning the scaffold.
- Test plan:
  1. `cargo fmt --all --check`
  2. `cargo clippy --workspace --all-targets -- -D warnings`
  3. `cargo test --workspace --all-targets`
  4. Run header generation and verify no drift from the checked-in/generated contract path chosen in implementation.
  5. Add at least one Rust-side smoke test for last-error/version behavior and one C-facing smoke path if practical at this stage.

## Next Handoff
Implementer: create the `calib-targets-ffi` scaffold exactly within this scope, record the chosen generated-header path and check strategy, and stop before detector-specific ABI entry points.
