# Backlog

## Status Values
- `todo` — not started
- `in-progress` — actively being worked
- `blocked` — waiting on something
- `done` — completed

## Priority Values
- `P0` — blocking release or correctness
- `P1` — next up
- `P2` — planned
- `P3` — someday

## ID Model
- Backlog ids (`INFRA-011`, `ALGO-014`, `DOCS-003`) are the stable planning ids used in this file.
- Workflow handoff ids (`TASK-012-...`) are execution-trace ids used under `docs/handoffs/`.
- Handoff reports should record both ids when the work came from the backlog.

---

## Active Sprint

- `FFI-003` Expose conservative chessboard / ChArUco / marker-board detector entry points over the approved ABI.

## Up Next
- `FFI-004` Add C examples, C++ RAII wrapper, ABI tests, and release/docs integration.

## Backlog

| ID | Status | Priority | Type | Title | Role | Notes |
|----|--------|----------|------|-------|------|-------|
| FFI-002 | done | P1 | infra | Scaffold `calib-targets-ffi` crate and header generation | implementer | Added the workspace FFI crate, shared ABI runtime, deterministic `cbindgen` header generation, and the initial public header. |
| FFI-003 | in-progress | P1 | infra | Add conservative detector handles and detection entry points | implementer | Implement approved v1 ABI for grayscale image input and fixed-struct config/result transport, including ChESS config. |
| FFI-004 | todo | P2 | docs | Add C examples, C++ RAII wrapper, and ABI verification | implementer | Thin C++ wrapper on top of the C ABI, plus docs and CI coverage including automated C header compile smoke checks. |

_Empty backlog is valid. Add the first work item as a new row in `Active Sprint`, `Up Next`, or `Backlog` when you want the workflow to mint a `TASK-*` handoff._

## API / Interface Tracking
- `CT-ABI-001` Config/result transport is fixed: use `repr(C)` structs and caller-owned arrays, not JSON.
- `CT-ABI-002` ChESS tuning surface is fixed: expose it from day 1.
- `CT-ABI-003` Dictionary scope is fixed: built-in dictionary names only in v1.


## Acceptance Scenarios (Attached to Tasks)
- `FFI-002` Header generation is deterministic and checked in CI; create/destroy APIs are leak-free under sanitizer/valgrind-style checks.
- `FFI-003` A C caller can detect chessboard / ChArUco / marker-board targets from an 8-bit grayscale buffer and retrieve stable fixed-struct results without Rust panics crossing the boundary.
- `FFI-004` The C++ RAII wrapper demonstrates correct ownership and error propagation without widening the underlying C ABI.

## Locked Defaults
- Dedicated `calib-targets-ffi` crate layered on top of `calib-targets`, not on individual lower crates.
- C ABI only: `extern "C"`, `#[no_mangle]`, `#[repr(C)]`, explicit status codes, and no panics across the boundary.
- Opaque detector handles own complex Rust state; callers own output buffers where practical.
- Fixed config/result structs are part of the v1 ABI; no JSON transport layer.
- Full config surface, including ChESS config, is available from day 1.
- Built-in dictionary names only in v1.
- Initial packaging target is `cdylib`.
- `cbindgen` generates the public header; the C++ wrapper lives above the C ABI and does not define the ABI.

## Done

| ID | Date | Type | Title | Notes |
|----|------|------|-------|-------|
| FFI-002 | 2026-03-11 | infra | Scaffold `calib-targets-ffi` crate and header generation | Added `calib-targets-ffi`, deterministic header generation, shared ABI status/error handling, and the initial generated header. |
| FFI-001 | 2026-03-10 | design | Freeze FFI v1 ABI scope | Fixed structs, full config surface, built-in dictionary names only, and `cdylib` first. See `docs/ffi/decision-record.md`. |
