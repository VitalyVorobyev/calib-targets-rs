# C API Guide

`calib-targets-ffi` exposes the detector stack of the `calib-targets` facade
crate through a stable C ABI, with a thin header-only C++17 RAII wrapper on top.
For the shortest working CMake integration see the
[CMake consumer quickstart](./cmake-consumer-quickstart.md); for the design
rationale see the [decision record](./decision-record.md).

| What | Where |
|---|---|
| Generated C header | `crates/calib-targets-ffi/include/calib_targets_ffi.h` |
| C++ wrapper (header-only) | `crates/calib-targets-ffi/include/calib_targets_ffi.hpp` |
| C example | `crates/calib-targets-ffi/examples/chessboard_consumer_smoke.c` |
| C++ example | `crates/calib-targets-ffi/examples/chessboard_wrapper_smoke.cpp` |
| CMake consumer example | `crates/calib-targets-ffi/examples/cmake_wrapper_consumer/` |
| Package staging / archive tools | `src/bin/stage-cmake-package.rs`, `src/bin/package-release-archive.rs` |

## Versioning and support boundaries

- **The crate version is the ABI version.** It is pinned in
  `crates/calib-targets-ffi/Cargo.toml` independently of the Rust workspace
  release and gets a major bump on every struct-layout or exported-symbol
  break (the manifest comment lists each break). `ct_version_string()` returns
  it, and the release-archive names and the CMake package version carry it.
  The CMake version file accepts any package version `>=` the requested one,
  so request an exact version if you need to pin an ABI major.
- `calib-targets-ffi` is `publish = false`: there is no crates.io or
  package-manager distribution. Tagged `v*` releases attach prebuilt native
  archives for Linux, macOS, and Windows.
- Image input is 8-bit grayscale only (`ct_gray_image_u8_t`).
- Marker dictionaries are the built-in `CT_DICTIONARY_*` ids only; custom
  dictionaries are not exposed.
- The C++ wrapper needs C++17; the CMake package needs CMake 3.16+.

## Exported functions

| Family | Functions |
|---|---|
| Library | `ct_version_string`, `ct_last_error_message` |
| Chessboard | `ct_chessboard_params_init_default`, `ct_chessboard_detector_{create,destroy,detect,detect_all}` |
| ChArUco | `ct_charuco_detector_{create,destroy,detect,diagnose_json}` |
| Marker board | `ct_marker_board_detector_{create,destroy,detect,diagnose_json}` |
| PuzzleBoard | `ct_puzzleboard_detector_{create,destroy,detect,diagnose_json}` |

Per-family outputs:

| Detector | Result header | Caller-owned arrays |
|---|---|---|
| Chessboard `detect` | `ct_chessboard_result_t` (`corners_len`, optional `cell_size`) | `ct_chessboard_corner_t` |
| Chessboard `detect_all` | one `ct_chessboard_result_t` per component | all components' `ct_chessboard_corner_t`, concatenated |
| ChArUco | `ct_charuco_result_t` (detection, `markers_len`, grid alignment) | `ct_labeled_corner_t`, `ct_marker_detection_t` |
| Marker board | `ct_marker_board_result_t` (detection, optional alignment) | `ct_labeled_corner_t` |
| PuzzleBoard | `ct_puzzleboard_result_t` (alignment, edge/bit statistics, master origin) | `ct_labeled_corner_t` (master-board `(I, J)` labels) |

Both corner structs end with `has_position_covariance` and
`position_covariance` (`ct_position_covariance_t { xx, xy, yy }`, px², image
frame): the covariance of `position`, valid when the flag is `CT_TRUE`. It is
`CT_FALSE` under `CT_UNCERTAINTY_OFF`, with an upscale stage, or when the
frame's noise estimate declined (typical of JPEG input — set
`ct_chess_config_t::noise_sigma`).

PuzzleBoard decode knobs live in `ct_puzzleboard_decode_config_t`:
`search_mode` (`CT_PUZZLEBOARD_SEARCH_MODE_{FULL,FIXED_BOARD}`), `scoring_mode`
(`CT_PUZZLEBOARD_SCORING_MODE_{HARD_WEIGHTED,SOFT_LOG_LIKELIHOOD}`), and
`symmetry_mode` (`CT_PUZZLEBOARD_SYMMETRY_MODE_ROTATIONS`, the default, or
`..._ROTATIONS_AND_REFLECTIONS` when the optical path mirrors the image).

## API model

**Status and errors.** Every fallible function returns `ct_status_t`:
`CT_STATUS_OK`; `CT_STATUS_NOT_FOUND` (the detector ran but found no target);
`CT_STATUS_INVALID_ARGUMENT`, `CT_STATUS_BUFFER_TOO_SMALL`,
`CT_STATUS_CONFIG_ERROR` (caller-visible failures); `CT_STATUS_INTERNAL_ERROR`.
The message for the last failure on the calling thread comes from
`ct_last_error_message(buf, capacity, &len)`, using the same query/fill pattern
as the result arrays (query with `NULL, 0`, allocate `len + 1`, call again).

**Handles.** Each family has an opaque handle (`ct_chessboard_detector_t`,
`ct_charuco_detector_t`, `ct_marker_board_detector_t`,
`ct_puzzleboard_detector_t`). `*_create` allocates it, `*_destroy` frees it,
`*_destroy(NULL)` is a no-op. Never free a handle yourself or destroy it twice.

**Args and buffers.** Each `*_detect` takes a `*_detect_args_t` (detector handle
+ image) and a `*_detect_buffers_t` (optional result header, output arrays with
capacities, and required `*_len` out-pointers). No memory crosses the ABI:

1. Call with each array pointer `NULL` and capacity `0`; the `*_len`
   out-pointers receive the required lengths.
2. Allocate the arrays yourself and call again to fill them. A short buffer
   returns `CT_STATUS_BUFFER_TOO_SMALL`.

`ct_chessboard_detector_detect_all` uses `ct_chessboard_detect_all_args_t` /
`ct_chessboard_detect_all_buffers_t`: `results_buf` receives one header per
recovered component (up to `max_components`) and `corners_buf` all corners
concatenated — slice it with each header's `corners_len`. It returns
`CT_STATUS_OK` with zero components when nothing is found.

**Configs.** `ct_chessboard_params_init_default` fills `ct_chessboard_params_t`
with the Rust defaults; the ChESS front-end (`ct_chess_config_t`) has no init
helper and must be filled explicitly (see the quickstart).

**Diagnostics JSON.** Detection results are always typed C structs. The
ChArUco, marker-board, and PuzzleBoard detectors additionally expose
`*_diagnose_json(args, buf, capacity, &len)`, which re-runs detection and writes
the Rust diagnostics struct (`CharucoDetectDiagnostics`,
`MarkerBoardDiagnostics`, `PuzzleBoardDiagnostics`) as NUL-terminated UTF-8
JSON. It returns `CT_STATUS_OK` with a well-formed payload even when detection
fails, and its schema carries a looser stability promise than the typed API.
The C++ wrapper exposes it as `detect_diagnostics_json(image, &json)`.

## C example

```c
ct_chessboard_detector_config_t config = /* fill chess + chessboard fields */;
ct_chessboard_detector_t *detector = NULL;
if (ct_chessboard_detector_create(&config, &detector) != CT_STATUS_OK) {
  /* read ct_last_error_message(...) */
}

ct_chessboard_detect_args_t args = { detector, &image };
ct_chessboard_result_t result;
size_t corners_len = 0;
ct_chessboard_detect_buffers_t bufs = { &result, NULL, 0, &corners_len };

ct_status_t status = ct_chessboard_detector_detect(&args, &bufs);  /* query */
if (status == CT_STATUS_OK) {
  ct_chessboard_corner_t *corners = calloc(corners_len, sizeof(*corners));
  bufs.out_corners = corners;
  bufs.corners_capacity = corners_len;
  status = ct_chessboard_detector_detect(&args, &bufs);            /* fill */
  /* ... use corners[k].position / corners[k].grid ... */
  free(corners);
}
ct_chessboard_detector_destroy(detector);
```

The repo example adds PGM loading and a default config through the
example-only helper `examples/native_smoke_common.h` (not public API) and
exercises the `CT_STATUS_INVALID_ARGUMENT` and `CT_STATUS_BUFFER_TOO_SMALL`
paths. Build and run it from the workspace root (macOS/Linux):

```bash
cargo build -p calib-targets-ffi
cc -std=c11 -Wall -Wextra -pedantic \
  -I crates/calib-targets-ffi/include -I crates/calib-targets-ffi/examples \
  crates/calib-targets-ffi/examples/chessboard_consumer_smoke.c \
  -L target/debug -lcalib_targets_ffi -Wl,-rpath,$PWD/target/debug \
  -o chessboard_consumer_smoke
./chessboard_consumer_smoke path/to/image.pgm
```

## C++ wrapper

`calib_targets_ffi.hpp` maps each handle to a move-only RAII class
(`calib_targets::ffi::ChessboardDetector`, `CharucoDetector`,
`MarkerBoardDetector`, `PuzzleBoardDetector`), runs the query/fill pattern into
`std::vector`s, and reports errors as `calib_targets::ffi::Status`
(`code`, `message`, `ok()`). It adds no exported symbols; the chessboard
`detect_all` entry point is C-only.

```cpp
calib_targets::ffi::ChessboardDetector detector;
auto status = detector.create(config);
ct_chessboard_result_t result{};
std::vector<ct_chessboard_corner_t> corners;
if (status.ok()) status = detector.detect(image, &result, &corners);
if (!status.ok()) { /* status.code, status.message */ }
```

Compile `examples/chessboard_wrapper_smoke.cpp` like the C example, with
`c++ -std=c++17`.

## Getting the library

**Prebuilt archives.** Tagged releases attach
`calib-targets-ffi-<abi-version>-<platform>.tar.gz` (Linux, macOS) or `.zip`
(Windows). Each unpacks to one directory holding the package prefix
(`include/`, `lib/`, `lib/cmake/calib_targets_ffi/`); point
`CMAKE_PREFIX_PATH` at it.

**From a checkout.** `cargo build -p calib-targets-ffi` produces
`libcalib_targets_ffi.{so,dylib}` / `calib_targets_ffi.dll` under
`target/<profile>/`. Stage the same prefix the archives ship, or build the
archive itself:

```bash
cargo run -p calib-targets-ffi --bin stage-cmake-package -- \
  --lib-dir target/debug --prefix target/ffi-cmake-package
cargo build -p calib-targets-ffi --release
cargo run -p calib-targets-ffi --bin package-release-archive -- \
  --lib-dir target/release --output-dir target/ffi-release-archives
```

The package exports `calib_targets_ffi::c` (the shared C library) and
`calib_targets_ffi::cpp` (the header-only wrapper on top of it); usage is in
the [quickstart](./cmake-consumer-quickstart.md). On Windows, put
`<prefix>/lib` on `PATH` or copy the DLL next to the executable.

## Validation

```bash
cargo run -p calib-targets-ffi --features generate-header --bin generate-ffi-header -- --check
cargo test -p calib-targets-ffi --test native_consumer_smoke -- --nocapture
cargo test -p calib-targets-ffi --test cmake_consumer_smoke -- --nocapture
cargo test -p calib-targets-ffi --test release_archive_smoke -- --ignored --nocapture
```

The header check fails when `calib_targets_ffi.h` is stale; regenerate it by
running the same command without `--check`.
