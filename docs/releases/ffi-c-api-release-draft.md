# Draft Release Notes: C API Launch

This release adds the first native C API for `calib-targets-rs`.

## Highlights

- New `calib-targets-ffi` crate with a generated public header, `cdylib` output, and explicit status/error handling.
- New C detector APIs for chessboard, ChArUco, and checkerboard marker-board detection from 8-bit grayscale images.
- Fixed `repr(C)` config/result structs, opaque detector handles, caller-owned query/fill buffers, and no JSON transport layer in the ABI.
- Repo-owned native validation now compiles and runs checked-in C and C++ consumers against the generated header and built shared library.
- A thin header-only C++ wrapper is included as a helper layer above the C ABI; it does not define a second ABI surface.

## Current Support Boundaries

- `calib-targets-ffi` is currently repo-local and remains `publish = false`; native consumers build it from the workspace rather than installing it from crates.io.
- Image input is limited to 8-bit grayscale buffers in this release.
- ChArUco support uses built-in dictionary names only.
- The checked-in C++ wrapper currently assumes a C++17-capable compiler and follows the same Unix-like toolchain path covered by the repo smoke test.
- This release does not yet provide ergonomic CMake packaging or imported CMake targets for downstream consumers.

## Validation Included In Repo

- `cargo run -p calib-targets-ffi --bin generate-ffi-header -- --check`
- `cargo test -p calib-targets-ffi --test native_consumer_smoke -- --nocapture`

The native smoke path builds the shared library, compiles the checked-in C and C++ examples against the generated header, and runs them on a deterministic chessboard fixture.

## Deferred Follow-Up Work

- `FFI-006`: publish a broader release-ready C API README and concise tutorials.
- `FFI-007`: add ergonomic C++ consumer packaging and a supported CMake integration flow.
