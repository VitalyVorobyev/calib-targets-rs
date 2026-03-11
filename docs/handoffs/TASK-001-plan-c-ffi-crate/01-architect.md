# Plan C FFI Crate

- Task ID: `TASK-001-plan-c-ffi-crate`
- Backlog ID: `FFI-001`
- Role: `architect`
- Date: `2026-03-10`
- Status: `ready_for_implementer`

## Inputs Consulted
- `docs/backlog.md`
- `Cargo.toml`
- `crates/calib-targets/Cargo.toml`
- `crates/calib-targets/src/lib.rs`
- `crates/calib-targets/src/detect.rs`
- `crates/calib-targets-core/src/corner.rs`
- `crates/calib-targets-core/src/image.rs`
- `crates/calib-targets-charuco/src/board.rs`
- `crates/calib-targets-charuco/src/detector/params.rs`
- `crates/calib-targets-charuco/src/detector/pipeline.rs`
- `crates/calib-targets-marker/src/types.rs`
- `crates/calib-targets-marker/src/detector.rs`
- `crates/calib-targets-aruco/src/dictionary.rs`
- `crates/calib-targets-py/Cargo.toml`
- `crates/calib-targets-py/README.md`
- `crates/calib-targets-py/python/calib_targets/config.py`
- `crates/calib-targets-py/python/calib_targets/results.py`
- `docs/ffi/README.md`
- `docs/ffi/decision-record.md`

## Summary
The workspace already has a strong public boundary in the `calib-targets` facade crate and an existing foreign-language integration pattern in `calib-targets-py`. The FFI direction is now fully resolved for v1: use a dedicated `calib-targets-ffi` crate, a first-class fixed-struct C ABI, full config exposure from day 1 including ChESS, built-in dictionary names only, and `cdylib` packaging first. The planning gate is complete and implementation can now begin with the scaffold task.

## Decisions Made
- The recommended crate boundary is `crates/calib-targets-ffi`, not `core` and not per-detector lower-crate FFI layers.
- The v1 ABI uses fixed `repr(C)` config/result structs, not JSON transport.
- The v1 ABI exposes all approved config surfaces from day 1, including ChESS configuration.
- The recommended v1 ABI remains conservative in other respects: C ABI only, opaque detector handles, explicit status codes, no panic crossing the boundary, caller-owned output buffers, `cbindgen`-generated headers, and no Rust debug/report payloads in v1.
- Built-in dictionary names only are supported in v1.
- `cdylib` is the initial packaging target.

## Files/Modules Affected
- `docs/ffi/README.md`
- `docs/backlog.md`
- `docs/handoffs/TASK-001-plan-c-ffi-crate/01-architect.md`
- Expected later: `Cargo.toml`
- Expected later: `crates/calib-targets-ffi/`

## Validation/Tests
- Repo structure and public API inspection completed.
- No code/tests run because this handoff stops at design and pending human decisions.

## Risks/Open Questions
- The fixed-struct requirement makes ABI review more important than usual, especially around optional-value conventions and variable-length outputs.
- Custom dictionary support is intentionally deferred; if it becomes necessary, treat it as a post-v1 ABI extension rather than widening the initial scaffold task.

## Role-Specific Details

### Architect Planning
- Problem statement:
  Add a repo-specific C/C++ FFI layer without freezing the wrong Rust boundary or overcommitting the ABI before the public contract is understood.
- Scope:
  Study the workspace, choose the FFI crate boundary, propose a conservative exported API shape, identify blocking design decisions, and break the work into backlog milestones.
- Out of scope:
  Any FFI code implementation, header generation, C examples, or C++ wrapper code before the major ABI decisions are answered.
- Constraints:
  Dedicated `*-ffi` crate preferred; C ABI only; FFI-safe types only; opaque handles for complex/stateful types; explicit status codes; no panics across the boundary; explicit ownership/free rules; caller-owned output buffers where practical; `cbindgen`; thin C++ RAII wrapper on top; conservative ABI surface.
- Assumptions:
  The facade crate remains the correct end-to-end public Rust API boundary and the FFI should not force lower crates to become ABI-aware.
- Implementation plan:
  1. Scaffold `calib-targets-ffi` and the shared ABI/runtime layer.
  2. Add fixed-struct config mirrors, conservative detector handles, and detect entry points.
  3. Add C examples, C++ wrapper, and ABI validation.
- Acceptance criteria:
  1. Backlog tasks `FFI-002` through `FFI-004` are actionable without revisiting the whole repo study.
  2. The recommended crate boundary and API shape are documented in `docs/ffi/README.md`.
  3. The resolved ABI decisions are recorded in `docs/ffi/decision-record.md`.
- Test plan:
  1. During implementation, require deterministic header generation and C integration smoke tests.
  2. Validate null-pointer, short-buffer, invalid-config, and not-found paths over the C ABI.

## Next Handoff
Implementer: start `FFI-002` by creating the `calib-targets-ffi` scaffold, shared ABI runtime layer, and deterministic header-generation flow using `docs/ffi/README.md` and `docs/ffi/decision-record.md` as the contract source.
