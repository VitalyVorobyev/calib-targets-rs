# FFI ABI Decision Record

- Date: `2026-03-10`
- Status: `accepted`

## Decisions

- The FFI lives in a dedicated crate: `crates/calib-targets-ffi`.
- The ABI is C-only and uses `extern "C"`, `#[no_mangle]`, and `#[repr(C)]`.
- Config and result transport uses fixed C structs and caller-owned arrays, not JSON.
- All config surfaces are available from day 1, including ChESS configuration.
- V1 dictionary support is built-in dictionary names only.
- Initial packaging target is `cdylib`.
- The public surface is a first-class C API with a thin C++ RAII wrapper layered above it.

## Consequences

- The FFI crate must define stable ABI mirrors for the caller-facing config/result types.
- Optional Rust values must be represented explicitly in C via presence flags or equivalent fixed conventions.
- Debug/report payloads remain out of the v1 ABI unless promoted deliberately later.
- Custom dictionary upload/support is deferred to a follow-up task if a real caller requires it.

## Rationale

- The facade crate `calib-targets` is already the correct end-to-end Rust boundary for foreign integrations.
- Fixed structs satisfy the requirement for a first-class C/C++ API and avoid a string-transport API.
- Exposing the full config surface from day 1 avoids painting the ABI into a corner for advanced users.
- Built-in dictionary names keep the first ABI smaller and more stable while the core detector API is being established.
