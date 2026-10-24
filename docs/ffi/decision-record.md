# FFI ABI Decision Record

- Date: `2026-03-10` (amended for the diagnostics channel and ABI versioning)
- Status: `accepted`

## Decisions

- The FFI lives in a dedicated crate, `crates/calib-targets-ffi`, layered on the
  `calib-targets` facade.
- The ABI is C-only: `extern "C"`, `#[no_mangle]`, `#[repr(C)]`, and a
  cbindgen-generated header. A thin header-only C++ RAII wrapper sits above it
  and adds no exported symbols.
- Configs and detection results are fixed C structs plus caller-owned arrays
  (query/fill), never JSON. Optional values use explicit presence flags.
- The full config surface is exposed, including the ChESS corner front-end.
- Opt-in diagnostics are the one JSON channel: `*_diagnose_json` writes the
  Rust diagnostics struct as a UTF-8 string. Its schema is looser than the
  typed result API and nothing in the typed API depends on it.
- Marker dictionaries are the built-in ids only.
- The shipped artifact is a `cdylib`.
- The crate version is the ABI version, pinned independently of the Rust
  workspace and bumped major on every struct-layout or symbol break.

## Consequences

- Every caller-facing config/result type needs a stable `#[repr(C)]` mirror,
  and any layout change is a major ABI bump.
- Debug payloads stay out of the typed ABI; anything a caller needs for
  production use must be promoted into a typed struct, not read from JSON.
- Custom dictionary support is deferred until a real caller needs it.

## Rationale

- The facade crate is already the end-to-end Rust boundary for foreign
  integrations.
- Fixed structs give C/C++ callers a first-class typed API instead of a
  string-transport API; diagnostics are inspection data whose shape changes
  with the algorithms, so serializing them avoids freezing that churn into
  struct layouts.
- Exposing the full config surface up front avoids painting the ABI into a
  corner for advanced users.
- Built-in dictionaries keep the ABI small and stable.
