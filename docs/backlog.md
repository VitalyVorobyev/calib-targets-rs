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

- `FFI-006` (`P0`, docs, implementer) Publish a release-ready C API README and concise tutorials.

## Up Next
- `FFI-007` (`P1`, infra, implementer) Add ergonomic C++ consumer packaging and CMake API after the C API release.

## Backlog

| ID | Status | Priority | Type | Title | Role | Notes |
|----|--------|----------|------|-------|------|-------|
| FFI-002 | done | P1 | infra | Scaffold `calib-targets-ffi` crate and header generation | implementer | Added the workspace FFI crate, shared ABI runtime, deterministic `cbindgen` header generation, and the initial public header. |
| FFI-003 | done | P1 | infra | Add conservative detector handles and detection entry points | implementer | Delivered the approved v1 detector ABI for grayscale image input and fixed-struct config/result transport, including ChESS config. |
| FFI-004 | done | P2 | docs | Add C examples, C++ RAII wrapper, and ABI verification | implementer | Added repo-owned C/C++ examples, a thin RAII wrapper, automated external compile/smoke coverage, and usage docs without widening the approved C ABI. |
| FFI-005 | done | P0 | docs | Add C API changelog entries and release notes | implementer | Added an `Unreleased` changelog entry plus a checked-in release-note draft for the C API launch, with current support boundaries and deferred C++/CMake follow-up called out explicitly. |
| FFI-006 | todo | P0 | docs | Publish a release-ready C API README and concise tutorials | implementer | Block the next release until the repo has a concise user-facing C API guide with build/link instructions, ownership/error rules, minimal detect tutorials, and top-level README navigation for C consumers. |
| FFI-007 | todo | P1 | infra | Add ergonomic C++ consumer packaging and CMake API | implementer | Planned now for post-release execution; package the existing thin C++ wrapper as a clean CMake consumer surface without widening the underlying C ABI. |

_Empty backlog is valid. Add the first work item as a new row in `Active Sprint`, `Up Next`, or `Backlog` when you want the workflow to mint a `TASK-*` handoff._

## API / Interface Tracking
- `CT-ABI-001` Config/result transport is fixed: use `repr(C)` structs and caller-owned arrays, not JSON.
- `CT-ABI-002` ChESS tuning surface is fixed: expose it from day 1.
- `CT-ABI-003` Dictionary scope is fixed: built-in dictionary names only in v1.


## Acceptance Scenarios (Attached to Tasks)
- `FFI-002` Header generation is deterministic and checked in CI; create/destroy APIs are leak-free under sanitizer/valgrind-style checks.
- `FFI-003` A C caller can detect chessboard / ChArUco / marker-board targets from an 8-bit grayscale buffer and retrieve stable fixed-struct results without Rust panics crossing the boundary.
- `FFI-004` Repo-owned C and C++ consumers compile and exercise the generated header/library in CI, and the thin C++ RAII wrapper preserves ownership and explicit error propagation without widening the underlying C ABI.
- `FFI-005` The next release entry in `CHANGELOG.md` and release notes clearly describe the shipped C API surface, native smoke validation, documented toolchain/language assumptions, and explicitly deferred post-release work.
- `FFI-006` The repo contains a concise but complete C API guide with build/link instructions, ownership and last-error conventions, at least one end-to-end tutorial, and clear top-level navigation from the workspace README.
- `FFI-007` Post-release C++ consumers can integrate through a documented CMake target/package and a repo-owned example without widening or bypassing the approved C ABI.

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
| FFI-005 | 2026-03-11 | docs | Add C API changelog entries and release notes | Added an `Unreleased` changelog entry and a checked-in release-note draft for the C API launch, with explicit support boundaries and deferred post-release work. |
| FFI-004 | 2026-03-11 | docs | Add C examples, C++ RAII wrapper, and ABI verification | Added repo-owned C/C++ smoke consumers, a header-only RAII wrapper, native compile/run smoke validation, CI header checks, and FFI usage docs. |
| FFI-003 | 2026-03-11 | infra | Add conservative detector handles and detection entry points | Added the v1 detector ABI for chessboard, ChArUco, and marker-board detection with fixed structs, query/fill arrays, and stable status/error mapping. |
| FFI-002 | 2026-03-11 | infra | Scaffold `calib-targets-ffi` crate and header generation | Added `calib-targets-ffi`, deterministic header generation, shared ABI status/error handling, and the initial generated header. |
| FFI-001 | 2026-03-10 | design | Freeze FFI v1 ABI scope | Fixed structs, full config surface, built-in dictionary names only, and `cdylib` first. See `docs/ffi/decision-record.md`. |
