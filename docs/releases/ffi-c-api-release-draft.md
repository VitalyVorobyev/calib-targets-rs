# Draft Release Notes: C API Launch

This release adds the first native C API for `calib-targets-rs`.

## Highlights

- New `calib-targets-ffi` crate with a generated public header, `cdylib` output, and explicit status/error handling.
- New C detector APIs for chessboard, ChArUco, and checkerboard marker-board detection from 8-bit grayscale images.
- Fixed `repr(C)` config/result structs, opaque detector handles, caller-owned query/fill buffers, and no JSON transport layer in the ABI.
- Marker scan config in the C ABI includes the `multi_threshold` toggle, matching the Rust `ScanDecodeConfig` surface for ChArUco callers.
- Repo-owned native validation now compiles and runs checked-in C and C++ consumers against the generated header and built shared library.
- A thin header-only C++ wrapper is included as a helper layer above the C ABI; it does not define a second ABI surface.
- Tagged GitHub releases now attach per-platform native archives that ship the staged `include/`, `lib/`, and `lib/cmake/` prefix for downstream C/C++ consumers.

## Current Support Boundaries

- `calib-targets-ffi` is still `publish = false`; native consumers use tagged GitHub release archives for supported Linux, macOS, and Windows platforms rather than installing from crates.io.
- Image input is limited to 8-bit grayscale buffers in this release.
- ChArUco support uses built-in dictionary names only.
- The checked-in C++ wrapper assumes a C++17-capable compiler, and the staged package/consumer flow targets CMake 3.16 or newer.
- There is still no package-manager metadata, installer flow, or signed native package.

## Validation Included In Repo

- `cargo run -p calib-targets-ffi --bin generate-ffi-header -- --check`
- `cargo test -p calib-targets-ffi --test native_consumer_smoke -- --nocapture`
- `cargo test -p calib-targets-ffi --test cmake_consumer_smoke -- --nocapture`
- `cargo test -p calib-targets-ffi --test release_archive_smoke -- --nocapture`

The native smoke path builds the shared library, compiles the checked-in C and
C++ examples against the generated header, exercises the staged CMake package,
and proves that an unpacked release archive can build and run the repo-owned
CMake consumer example on the supported release runners.

## Deferred Follow-Up Work

- Code signing, notarization, or other OS-level trust/distribution hardening for the native archives.
- Package-manager or installer-based native distribution beyond the GitHub release assets.
