# C API Guide

`calib-targets-rs` ships a repo-local native FFI crate, `calib-targets-ffi`, for
C and C++ consumers that want the same detector stack exposed by the Rust
facade crate.

This guide covers the current release-facing native surface:

- generated public header: `crates/calib-targets-ffi/include/calib_targets_ffi.h`
- header-only C++ helper wrapper: `crates/calib-targets-ffi/include/calib_targets_ffi.hpp`
- repo-local staging helper: `crates/calib-targets-ffi/src/bin/stage-cmake-package.rs`
- repo-local release-archive helper: `crates/calib-targets-ffi/src/bin/package-release-archive.rs`
- repo-owned C example: `crates/calib-targets-ffi/examples/chessboard_consumer_smoke.c`
- repo-owned C++ example: `crates/calib-targets-ffi/examples/chessboard_wrapper_smoke.cpp`
- repo-owned CMake consumer example:
  `crates/calib-targets-ffi/examples/cmake_wrapper_consumer/`

For the architectural rationale behind the ABI shape, see the
[decision record](./decision-record.md). This document focuses on consumption,
not design history.

If you want a direct “what files do I need and what does the smallest CMake
consumer look like?” walkthrough, start with
[`docs/ffi/cmake-consumer-quickstart.md`](./cmake-consumer-quickstart.md).

## Current Support Boundaries

- `calib-targets-ffi` is repo-local and remains `publish = false`.
- Supported tagged GitHub releases attach native archives for Linux, macOS, and
  Windows. Each archive contains the staged `include/`, `lib/`, and
  `lib/cmake/` prefix, so downstream consumers can integrate without building
  Rust from source.
- The crate is still not distributed on crates.io, and there is still no
  package-manager metadata, installer flow, or signed native package.
- Image input is limited to 8-bit grayscale buffers via `ct_gray_image_u8_t`.
- The v1 ABI supports built-in dictionary ids only.
- The C++ helper wrapper assumes a C++17-capable compiler.
- The staged CMake consumer flow assumes CMake 3.16 or newer and is currently
  validated on Linux, macOS, and Windows release runners.

## What Ships

The native ABI currently exposes:

- chessboard detection via `ct_chessboard_detector_*`
- ChArUco detection via `ct_charuco_detector_*`
- PuzzleBoard detection via `ct_puzzleboard_detector_*`
- checkerboard marker-board detection via `ct_marker_board_detector_*`
- a staged CMake config package exporting `calib_targets_ffi::c` and
  `calib_targets_ffi::cpp`
- shared status/error retrieval through `ct_status_t` and
  `ct_last_error_message`
- caller-owned result arrays with query/fill patterns instead of heap ownership
  crossing the ABI boundary

## Download Native Release Archives

Supported tagged releases attach one native archive per platform. Archive names
follow this pattern:

- Linux and macOS: `calib-targets-ffi-<version>-<platform>.tar.gz`
- Windows: `calib-targets-ffi-<version>-<platform>.zip`

Each archive unpacks into a single top-level directory named the same way,
containing the staged package prefix directly:

```text
calib-targets-ffi-<version>-<platform>/
  include/
  lib/
  lib/cmake/calib_targets_ffi/
```

Point `CMAKE_PREFIX_PATH` at that unpacked top-level directory.

If you are working from a repo checkout instead of a tagged release asset, the
rest of this guide still shows how to build and stage the same layout locally.

## Build And Link

Build the shared library from the workspace root:

```bash
cargo build -p calib-targets-ffi
```

The public headers live in:

```text
crates/calib-targets-ffi/include/
```

The shared library is produced in the usual Cargo target directory, for example:

- macOS: `target/debug/libcalib_targets_ffi.dylib`
- Linux: `target/debug/libcalib_targets_ffi.so`
- Windows: `target/debug/calib_targets_ffi.dll`

Typical include and link flags from the repo root look like this:

```bash
cc -std=c11 -Wall -Wextra -pedantic \
  -I crates/calib-targets-ffi/include \
  your_app.c \
  -L target/debug \
  -lcalib_targets_ffi \
  -o your_app
```

On Unix-like platforms you usually also want either an rpath or a runtime
library search path:

- macOS: `DYLD_LIBRARY_PATH=target/debug`
- Linux: `LD_LIBRARY_PATH=target/debug`

If you want to verify the checked-in header before integrating, run:

```bash
cargo run -p calib-targets-ffi --bin generate-ffi-header -- --check
```

## Stage A CMake Package

If you are working from a repo checkout, build the shared library first, then
stage the same package prefix that is shipped inside the native release
archives:

```bash
cargo build -p calib-targets-ffi
cargo run -p calib-targets-ffi --bin stage-cmake-package -- \
  --lib-dir target/debug \
  --prefix target/ffi-cmake-package
```

That produces a prefix with the shape:

```text
target/ffi-cmake-package/
  include/calib_targets_ffi.h
  include/calib_targets_ffi.hpp
  lib/libcalib_targets_ffi.{so,dylib}
  lib/cmake/calib_targets_ffi/calib_targets_ffi-config.cmake
  lib/cmake/calib_targets_ffi/calib_targets_ffi-config-version.cmake
```

The generated CMake package exports two targets:

- `calib_targets_ffi::c` for the shared C ABI library
- `calib_targets_ffi::cpp` for the header-only C++ wrapper layered on top of it

The local stage command is useful when you want to test packaging changes from a
checkout. Supported tagged releases ship the same layout as downloadable native
archives, but there is still no system package or package-manager integration.

## Build A Matching Release Archive Locally

If you want to rehearse the exact tagged-release payload from a repo checkout,
build the release library and package the staged prefix as a platform archive:

```bash
cargo build -p calib-targets-ffi --release
cargo run -p calib-targets-ffi --bin package-release-archive -- \
  --lib-dir target/release \
  --output-dir target/ffi-release-archives
```

The command prints the produced archive path. Linux and macOS emit `.tar.gz`;
Windows emits `.zip`. Unpacking that archive yields the same top-level
`calib-targets-ffi-<version>-<platform>/` prefix that the tagged GitHub
releases publish.

## API Model

### Status Codes And Error Text

Every exported function returns `ct_status_t`.

- `CT_STATUS_OK` means success.
- `CT_STATUS_NOT_FOUND` means the detector ran successfully but found no target.
- `CT_STATUS_INVALID_ARGUMENT`, `CT_STATUS_BUFFER_TOO_SMALL`, and
  `CT_STATUS_CONFIG_ERROR` indicate caller-visible failures.
- `CT_STATUS_INTERNAL_ERROR` is reserved for unexpected internal failures.

Failure details are retrieved with `ct_last_error_message(...)`. Use the same
query/copy pattern as the result arrays:

```c
size_t len = 0;
ct_status_t status = ct_last_error_message(NULL, 0, &len);
```

Then allocate a buffer of at least `len + 1` bytes and call it again.

### Handle Ownership

Each detector family uses an opaque handle:

- `ct_chessboard_detector_t`
- `ct_charuco_detector_t`
- `ct_puzzleboard_detector_t`
- `ct_marker_board_detector_t`

Ownership rules are simple:

- `*_create(...)` allocates a handle on success.
- `*_destroy(...)` frees it.
- `*_destroy(NULL)` is allowed.
- Do not free a handle yourself or destroy it twice.

### Caller-Owned Arrays

Variable-length outputs never allocate memory for you. Instead:

1. Call the detect function with the output array pointer set to `NULL` and its
   capacity set to `0`.
2. Read the required length from the corresponding `*_len` out-parameter.
3. Allocate that many entries in your own memory.
4. Call the detect function again to fill the array.

For chessboard detection that pattern is:

```c
size_t corners_len = 0;
ct_chessboard_detector_detect(detector, &image, &result, NULL, 0, &corners_len);
```

Then allocate `corners_len` `ct_labeled_corner_t` entries and call
`ct_chessboard_detector_detect(...)` again.

## Plain C Tutorial: Chessboard Detection

The complete runnable example in this repo is:

- `crates/calib-targets-ffi/examples/chessboard_consumer_smoke.c`

That example uses the repo-local helper header
`crates/calib-targets-ffi/examples/native_smoke_common.h` for two things:

- loading a binary PGM fixture into `ct_gray_image_u8_t`
- constructing a known-good `ct_chessboard_detector_config_t`

`native_smoke_common.h` is example scaffolding, not part of the public API.
Downstream applications should build their own config/image-loading helpers and
fill the public ABI structs from `calib_targets_ffi.h`.

The end-to-end detection flow itself looks like this:

```c
ct_chessboard_detector_t *detector = NULL;
ct_chessboard_detector_config_t config = ct_native_default_chessboard_detector_config();
ct_chessboard_result_t result;
ct_labeled_corner_t *corners = NULL;
size_t corners_len = 0;

memset(&result, 0, sizeof(result));

ct_status_t status = ct_chessboard_detector_create(&config, &detector);
if (status != CT_STATUS_OK) {
  /* read ct_last_error_message(...) */
}

status = ct_chessboard_detector_detect(detector, &image, &result, NULL, 0, &corners_len);
if (status != CT_STATUS_OK) {
  /* handle failure */
}

corners = (ct_labeled_corner_t *)calloc(corners_len, sizeof(*corners));
status = ct_chessboard_detector_detect(
    detector,
    &image,
    &result,
    corners,
    corners_len,
    &corners_len);
if (status != CT_STATUS_OK) {
  /* handle failure */
}

ct_chessboard_detector_destroy(detector);
free(corners);
```

The checked-in example also demonstrates two important failure paths worth
preserving in downstream code:

- invalid create arguments return `CT_STATUS_INVALID_ARGUMENT`
- short output buffers return `CT_STATUS_BUFFER_TOO_SMALL`

To compile and run the repo-owned example from the workspace root on the same
macOS/Linux-style path covered by the repo smoke test:

```bash
cargo build -p calib-targets-ffi

cc -std=c11 -Wall -Wextra -pedantic \
  -I crates/calib-targets-ffi/include \
  -I crates/calib-targets-ffi/examples \
  crates/calib-targets-ffi/examples/chessboard_consumer_smoke.c \
  -L target/debug \
  -lcalib_targets_ffi \
  -Wl,-rpath,$PWD/target/debug \
  -o chessboard_consumer_smoke
```

Then run it against a grayscale PGM fixture:

```bash
./chessboard_consumer_smoke path/to/image.pgm
```

If you want a repo-owned end-to-end proof, use the native smoke test instead:

```bash
cargo test -p calib-targets-ffi --test native_consumer_smoke -- --nocapture
```

## C++ Tutorial: Thin Wrapper Layer

The helper wrapper lives in:

- `crates/calib-targets-ffi/include/calib_targets_ffi.hpp`

It is header-only and stays strictly above the C ABI:

- detector ownership is mapped into RAII objects
- status handling stays explicit through `calib_targets::ffi::Status`
- the wrapper does not introduce new exported symbols

The current wrapper example is:

- `crates/calib-targets-ffi/examples/chessboard_wrapper_smoke.cpp`

Minimal usage looks like this:

```cpp
ct_chessboard_detector_config_t config = ct_native_default_chessboard_detector_config();
calib_targets::ffi::ChessboardDetector detector;
ct_chessboard_result_t result{};
std::vector<ct_labeled_corner_t> corners;

auto status = detector.create(config);
if (!status.ok()) {
  /* inspect status.message */
}

status = detector.detect(image.descriptor, &result, &corners);
if (!status.ok()) {
  /* inspect status.message */
}
```

The wrapper example uses the same repo-local helper header as the C example for
fixture loading and default config construction. The public contract is still
the C ABI types from `calib_targets_ffi.h`.

Compile it from the workspace root on the same macOS/Linux-style path covered by
the repo smoke test, using a C++17-capable compiler:

```bash
cargo build -p calib-targets-ffi

c++ -std=c++17 -Wall -Wextra -pedantic \
  -I crates/calib-targets-ffi/include \
  -I crates/calib-targets-ffi/examples \
  crates/calib-targets-ffi/examples/chessboard_wrapper_smoke.cpp \
  -L target/debug \
  -lcalib_targets_ffi \
  -Wl,-rpath,$PWD/target/debug \
  -o chessboard_wrapper_smoke
```

## CMake Consumer Flow

The staged package is intended to remove handwritten include directories and
linker flags from downstream CMake consumers. The repo-owned example is:

- `crates/calib-targets-ffi/examples/cmake_wrapper_consumer/`

Its `CMakeLists.txt` uses the staged package like this:

```cmake
cmake_minimum_required(VERSION 3.16)
project(chessboard_cmake_consumer LANGUAGES CXX)

find_package(calib_targets_ffi CONFIG REQUIRED)

add_executable(chessboard_cmake_consumer main.cpp)
target_compile_features(chessboard_cmake_consumer PRIVATE cxx_std_17)
target_link_libraries(chessboard_cmake_consumer PRIVATE calib_targets_ffi::cpp)

set_property(
  TARGET chessboard_cmake_consumer
  PROPERTY BUILD_RPATH "$<TARGET_FILE_DIR:calib_targets_ffi::c>"
)
```

To build and run that example from the workspace root:

```bash
cargo build -p calib-targets-ffi
cargo run -p calib-targets-ffi --bin stage-cmake-package -- \
  --lib-dir target/debug \
  --prefix target/ffi-cmake-package

cmake -S crates/calib-targets-ffi/examples/cmake_wrapper_consumer \
  -B target/cmake-wrapper-consumer \
  -DCMAKE_PREFIX_PATH=$PWD/target/ffi-cmake-package
cmake --build target/cmake-wrapper-consumer
target/cmake-wrapper-consumer/chessboard_cmake_consumer path/to/image.pgm
```

The example keeps the public boundary clean:

- it includes `calib_targets_ffi.hpp` from the staged package
- it links against the exported `calib_targets_ffi::cpp` target
- its local helper header handles PGM loading and config construction inside the
  consumer project rather than depending on repo-internal smoke helpers

On Windows, make sure `<prefix>/lib` is on `PATH` when you run a consumer
binary, or copy `calib_targets_ffi.dll` next to the executable. The repo smoke
tests set `PATH` automatically for that case.

## Other Detector Families

The other detector families follow the same broad model:

- ChArUco:
  `ct_charuco_detector_create`, `ct_charuco_detector_detect`,
  `ct_charuco_detector_destroy`
- Marker board:
  `ct_marker_board_detector_create`, `ct_marker_board_detector_detect`,
  `ct_marker_board_detector_destroy`

The main differences are the output arrays:

- ChArUco fills both `ct_labeled_corner_t` and `ct_marker_detection_t`
- marker-board detection fills labeled corners, circle candidates, and circle
  matches

The same ownership and query/fill rules apply.

## Validation Commands

These commands exercise the native surface the repo currently documents:

```bash
cargo run -p calib-targets-ffi --bin generate-ffi-header -- --check
cargo test -p calib-targets-ffi --test native_consumer_smoke -- --nocapture
cargo test -p calib-targets-ffi --test cmake_consumer_smoke -- --nocapture
cargo test -p calib-targets-ffi --test release_archive_smoke -- --nocapture
```

## Design History

If you need the design rationale rather than the consumer workflow, start with:

- `docs/ffi/decision-record.md`
- `docs/handoffs/TASK-001-plan-c-ffi-crate/01-architect.md`
- `docs/handoffs/TASK-004-add-c-examples-cpp-raii-wrapper-and-abi-verification/01-architect.md`
