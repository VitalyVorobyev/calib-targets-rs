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

- _None currently._

## Up Next
- `PRINT-004` (`P1`, infra, implementer) Productize the printable target CLI workflow.

## Backlog

| ID | Status | Priority | Type | Title | Role | Notes |
|----|--------|----------|------|-------|------|-------|
| FFI-002 | done | P1 | infra | Scaffold `calib-targets-ffi` crate and header generation | implementer | Added the workspace FFI crate, shared ABI runtime, deterministic `cbindgen` header generation, and the initial public header. |
| FFI-003 | done | P1 | infra | Add conservative detector handles and detection entry points | implementer | Delivered the approved v1 detector ABI for grayscale image input and fixed-struct config/result transport, including ChESS config. |
| FFI-004 | done | P2 | docs | Add C examples, C++ RAII wrapper, and ABI verification | implementer | Added repo-owned C/C++ examples, a thin RAII wrapper, automated external compile/smoke coverage, and usage docs without widening the approved C ABI. |
| FFI-005 | done | P0 | docs | Add C API changelog entries and release notes | implementer | Added an `Unreleased` changelog entry plus a checked-in release-note draft for the C API launch, with current support boundaries and deferred C++/CMake follow-up called out explicitly. |
| FFI-006 | done | P0 | docs | Publish a release-ready C API README and concise tutorials | implementer | Added a top-level native entry point plus a release-facing C API guide with build/link instructions, ownership/error rules, query/fill coverage, and concise C/C++ tutorials aligned to the shipped examples. |
| FFI-007 | done | P1 | infra | Add ergonomic C++ consumer packaging and CMake API | implementer | Added a repo-local staged CMake package, exported C/C++ consumer targets, a `find_package(...)` example, dedicated smoke validation, and matching docs without widening the underlying C ABI. |
| FFI-008 | done | P1 | infra | Publish native C/C++ release artifacts for supported platforms | implementer | Delivered per-platform native archive packaging, staged-prefix-based smoke validation, a tag-driven release workflow, and release-facing docs for GitHub-hosted native artifacts. |
| PRINT-001 | done | P0 | infra | Prepare `calib-targets-print` for crates.io publish and align workspace release metadata | implementer | Added release-workflow support, publish-order handling, and publication-facing README/metadata/doc alignment so the crate is ready for a later live crates.io publish. |
| PRINT-002 | done | P1 | api | Add ergonomic detector-spec to printable-target conversions without moving rendering into detector crates | implementer | Added explicit millimeter-aware conversions from `CharucoBoardSpec` and `MarkerBoardLayout` into printable specs/documents, with checked marker-board layout failures and no detector-to-print dependency changes. |
| PRINT-003 | done | P1 | docs | Publish a release-ready printable-targets README and workspace entry points | implementer | Added a canonical printable-target guide plus aligned root/facade/print/CLI/Python entry points, concrete JSON and Rust/CLI/Python examples, output expectations, and print-at-100%-scale guidance. |
| PRINT-004 | todo | P1 | infra | Productize the printable target CLI workflow | implementer | Promote the existing `calib-targets-cli` init/generate flow as the official target-generation app, add any minimal missing affordances such as validation or dictionary discovery, and document whether the CLI remains repo-local or becomes a published companion package. |
| PRINT-005 | blocked | P0 | release | Perform the live crates.io publish for `calib-targets-print` and final docs-state sync | implementer | Execute the actual crates.io publish after the remaining printable backlog work lands, then switch any remaining workspace-local wording to live published-crate wording where appropriate. |

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
- `FFI-008` Supported releases attach native archives for the chosen platform matrix, each containing the staged include/lib/cmake prefix, and the docs explain how downstream C/C++ consumers use those artifacts without a Rust toolchain.
- `PRINT-001` `calib-targets-print` is fully prepared for crates.io publication: the workspace release workflow includes it in the correct order, dry-run publish succeeds, and current docs/metadata accurately describe the pre-publish state while pointing users to the canonical printable-target workflow.
- `PRINT-002` Rust users can construct printable target documents directly from detector-owned board/layout specs where appropriate, without duplicating rendering code across detector crates or introducing cyclic crate dependencies.
- `PRINT-003` The repo contains a concise but release-facing printable-target guide with one canonical JSON example, Rust/CLI/Python quickstarts, output-file expectations, and explicit physical-print validation guidance.
- `PRINT-004` A user can discover, initialize, validate, and generate a printable target through the documented CLI flow, and the scope/distribution model of that CLI is explicit in repo docs.
- `PRINT-005` The dedicated `calib-targets-print` crate is publicly available on crates.io, the workspace/facade docs match that live state, and users can depend on it directly without contradictory repo wording.

## Locked Defaults
- Dedicated `calib-targets-ffi` crate layered on top of `calib-targets`, not on individual lower crates.
- C ABI only: `extern "C"`, `#[no_mangle]`, `#[repr(C)]`, explicit status codes, and no panics across the boundary.
- Opaque detector handles own complex Rust state; callers own output buffers where practical.
- Fixed config/result structs are part of the v1 ABI; no JSON transport layer.
- Full config surface, including ChESS config, is available from day 1.
- Built-in dictionary names only in v1.
- Initial packaging target is `cdylib`.
- `cbindgen` generates the public header; the C++ wrapper lives above the C ABI and does not define the ABI.
- Dedicated `calib-targets-print` crate layered on top of detector crates stays as the printable-target backend; do not move SVG/PNG/page rendering into `calib-targets-charuco`, `calib-targets-marker`, or other detector crates.
- Detector crates own board semantics and reusable board/layout specs; `calib-targets-print` owns page specification, layout resolution, rendering, and output bundles, with facade re-exports used only for ergonomics.

## Done

| ID | Date | Type | Title | Notes |
|----|------|------|-------|-------|
| PRINT-002 | 2026-03-12 | api | Add ergonomic detector-spec to printable-target conversions without moving rendering into detector crates | Added explicit ChArUco and marker-board conversion helpers in `calib-targets-print`, preserved centralized rendering ownership, and rejected marker-board layouts that cannot be converted deterministically for printing. |
| PRINT-001 | 2026-03-12 | infra | Prepare `calib-targets-print` for crates.io publish and align workspace release metadata | Added release-workflow support, correct publish ordering, dry-run publish validation, and release-facing doc alignment while intentionally deferring the live crates.io publish until the remaining printable backlog tasks are complete. |
| PRINT-003 | 2026-03-12 | docs | Publish a release-ready printable-targets README and workspace entry points | Added the canonical printable-target guide, aligned workspace and crate README entry points, documented Rust/CLI/Python flows, and made the current published-vs-repo-local distribution boundaries explicit. |
| FFI-008 | 2026-03-12 | infra | Publish native C/C++ release artifacts for supported platforms | Added deterministic native release-archive packaging, multi-config-safe smoke validation, a tag-driven GitHub workflow, and updated docs/release notes for GitHub-hosted native artifacts. |
| FFI-007 | 2026-03-12 | infra | Add ergonomic C++ consumer packaging and CMake API | Added a repo-local staged CMake package, exported `calib_targets_ffi::c` and `calib_targets_ffi::cpp` targets, a `find_package(...)` consumer example, dedicated smoke validation, and updated native docs/CI. |
| FFI-006 | 2026-03-11 | docs | Publish a release-ready C API README and concise tutorials | Added a top-level native entry point and rewrote the FFI README into a release-facing C API guide with support boundaries, build/link flow, ownership rules, and concise example-backed tutorials. |
| FFI-005 | 2026-03-11 | docs | Add C API changelog entries and release notes | Added an `Unreleased` changelog entry and a checked-in release-note draft for the C API launch, with explicit support boundaries and deferred post-release work. |
| FFI-004 | 2026-03-11 | docs | Add C examples, C++ RAII wrapper, and ABI verification | Added repo-owned C/C++ smoke consumers, a header-only RAII wrapper, native compile/run smoke validation, CI header checks, and FFI usage docs. |
| FFI-003 | 2026-03-11 | infra | Add conservative detector handles and detection entry points | Added the v1 detector ABI for chessboard, ChArUco, and marker-board detection with fixed structs, query/fill arrays, and stable status/error mapping. |
| FFI-002 | 2026-03-11 | infra | Scaffold `calib-targets-ffi` crate and header generation | Added `calib-targets-ffi`, deterministic header generation, shared ABI status/error handling, and the initial generated header. |
| FFI-001 | 2026-03-10 | design | Freeze FFI v1 ABI scope | Fixed structs, full config surface, built-in dictionary names only, and `cdylib` first. See `docs/ffi/decision-record.md`. |
