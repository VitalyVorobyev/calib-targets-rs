# C/C++ FFI Plan

## Repo Understanding Summary

This workspace already has a clear top-level API boundary:

- `crates/calib-targets` is the facade crate that re-exports the detector crates and provides end-to-end `detect_*` helpers from grayscale images and raw `u8` image buffers.
- `crates/calib-targets-core` contains geometry/image primitives and detection result types, but it is intentionally detector-agnostic and does not expose the full user-facing workflow.
- `crates/calib-targets-chessboard`, `crates/calib-targets-charuco`, and `crates/calib-targets-marker` hold the detector-specific params/results/detector structs.
- `crates/calib-targets-py` is the only current foreign-language boundary. It depends on the facade crate rather than on lower crates directly, and it already uses serde-backed config/result conversion to avoid binding every Rust type 1:1.

Implication for FFI design: the correct crate boundary is above the existing facade API, not inside `core` and not as separate FFI layers per detector crate.

## Recommended FFI Crate Placement

Create a dedicated crate:

- `crates/calib-targets-ffi`

Recommended dependencies and role:

- Depend on `calib-targets` with the `image` feature enabled.
- Reuse facade-level detection entry points (`detect_chessboard_from_gray_u8`, `detect_charuco_from_gray_u8`, `detect_marker_board_from_gray_u8`) where possible.
- Reuse the detector structs (`ChessboardDetector`, `CharucoDetector`, `MarkerBoardDetector`) internally for opaque handles when setup/validation should be cached across calls.
- Add `cbindgen` configuration in this crate, with header output treated as part of the crate deliverable.
- Keep all ABI-only types and panic/error containment in this crate so lower crates stay pure Rust and unconstrained by C ABI concerns.

Why not place the ABI lower?

- `calib-targets-core` is too low-level; it lacks the caller-facing detection workflow and would force the C API to reconstruct public behavior from internal pieces.
- Binding directly to `charuco`, `marker`, or `chessboard` would fracture the foreign-language surface and bypass the existing facade boundary already used by Python.
- The facade crate is the smallest layer that already reflects the repo's intended end-to-end public API.

## Confirmed Decisions

The following v1 decisions are now fixed:

- Config and result transport must use fixed C structs, not JSON.
- All config surfaces, including ChESS configuration, are exposed from day 1.
- Built-in dictionary names only in v1.
- Initial library delivery target is `cdylib`.
- The API must read as a first-class C API with a thin first-class C++ wrapper above it, not as a generic string transport.

## Recommended v1 Scope

Updated v1 recommendation:

- One FFI crate.
- One shared header.
- Grayscale `u8` image input only.
- Opaque detector handles for chessboard, ChArUco, and marker-board detectors.
- Fixed `repr(C)` config and result structs.
- Stable detection results only; Rust-only debug/report structs are not part of the v1 ABI.
- Built-in dictionary names only.
- `cdylib` first.
- Thin C++ RAII wrapper planned on top, but not part of the ABI contract itself.

## Proposed Exported API Shape

### ABI Principles

- C ABI only: `extern "C"`, `#[no_mangle]`, `#[repr(C)]`.
- FFI-safe types only.
- No panics cross the boundary; all exported functions wrap Rust internals in `catch_unwind`.
- Explicit status codes for every call.
- Explicit ownership and free rules.
- Caller-owned output buffers for serialized results and error messages.
- Opaque handles for complex/stateful Rust detector objects.

### Fixed Struct ABI Strategy

Confirmed v1 transport:

- Config in: fixed `repr(C)` structs.
- Result out: fixed `repr(C)` structs plus caller-owned arrays.

Implications:

- The FFI crate must define explicit ABI mirrors for the caller-visible configuration types.
- Optional values in Rust must become explicit presence flags or sentinel conventions in C.
- Nested result graphs must be flattened into count + pointer or count + fill-call patterns.
- The ABI should expose stable, detector-focused data, not raw serialized Rust internals.

Recommended rule:

- Mirror only the stable caller-facing config/result surface, not every Rust implementation detail.
- Exclude debug/report-only fields from v1.
- Keep all variable-length outputs caller-owned via query/fill APIs.

### Recommended C Header Sketch

```c
typedef enum ct_status {
    CT_STATUS_OK = 0,
    CT_STATUS_NOT_FOUND = 1,
    CT_STATUS_INVALID_ARGUMENT = 2,
    CT_STATUS_BUFFER_TOO_SMALL = 3,
    CT_STATUS_CONFIG_ERROR = 4,
    CT_STATUS_INTERNAL_ERROR = 255
} ct_status_t;

typedef struct ct_gray_image_u8 {
    uint32_t width;
    uint32_t height;
    size_t stride_bytes;
    const uint8_t* data;
} ct_gray_image_u8_t;

typedef struct ct_chessboard_detector ct_chessboard_detector_t;
typedef struct ct_charuco_detector ct_charuco_detector_t;
typedef struct ct_marker_board_detector ct_marker_board_detector_t;

const char* ct_version_string(void);

ct_status_t ct_last_error_message(char* out_utf8, size_t out_capacity, size_t* out_len);

typedef struct ct_presence_u32 {
    uint32_t has_value;
    uint32_t value;
} ct_presence_u32_t;

typedef struct ct_presence_f32 {
    uint32_t has_value;
    float value;
} ct_presence_f32_t;

typedef struct ct_chess_corner_params {
    uint32_t use_radius10;
    uint32_t descriptor_use_radius10_has_value;
    uint32_t descriptor_use_radius10;
    float threshold_rel;
    uint32_t threshold_abs_has_value;
    float threshold_abs;
    uint32_t nms_radius;
    uint32_t min_cluster_size;
} ct_chess_corner_params_t;

typedef struct ct_pyramid_params {
    uint32_t num_levels;
    size_t min_size;
} ct_pyramid_params_t;

typedef struct ct_coarse_to_fine_params {
    ct_pyramid_params_t pyramid;
    uint32_t refinement_radius;
    float merge_radius;
} ct_coarse_to_fine_params_t;

typedef struct ct_chess_config {
    ct_chess_corner_params_t params;
    ct_coarse_to_fine_params_t multiscale;
} ct_chess_config_t;

typedef struct ct_charuco_detector_params ct_charuco_detector_params_t;

ct_status_t ct_charuco_detector_create(
    const ct_charuco_detector_params_t* params,
    ct_charuco_detector_t** out_detector);

void ct_charuco_detector_destroy(ct_charuco_detector_t* detector);

ct_status_t ct_charuco_detector_detect(
    const ct_charuco_detector_t* detector,
    const ct_gray_image_u8_t* image,
    ct_target_detection_t* out_detection,
    ct_marker_detection_t* out_markers,
    size_t markers_capacity,
    size_t* out_markers_len,
    ct_grid_alignment_t* out_alignment);
```

Apply the same pattern to chessboard and marker-board detectors.

### Ownership Model

- `*_create` allocates a detector handle owned by the caller after success.
- `*_destroy` is always safe on non-null handles and is the only way to free them.
- Detection results use caller-owned output objects and arrays.
- Variable-length outputs use a two-call pattern:
  - first call with `capacity = 0` and output pointer `NULL` to query required count,
  - second call with a caller-allocated array to receive the values.
- Error messages still use a caller-owned UTF-8 buffer-query pattern.

### Recommended ABI Struct Families

Configuration families that should be mirrored explicitly in `repr(C)` form:

- Shared/common:
  - grayscale image descriptor
  - optional scalar helpers / presence wrappers
  - ChESS config (`ChessConfig`, `ChessCornerParams`, `PyramidParams`, `CoarseToFineParams`)
  - orientation clustering config
  - grid graph config
- Chessboard:
  - `ChessboardParams`
- ChArUco:
  - board spec
  - marker layout enum
  - scan/decode config
  - detector params
- Marker board:
  - circle polarity enum
  - marker circle spec
  - board layout
  - circle score config
  - circle match config
  - detector params

Result families that should be mirrored explicitly in `repr(C)` form:

- common corner/grid/alignment structs
- target detection header
- labeled corner output array
- chessboard result header
- marker detection array
- marker-board circle candidate / match arrays

Recommended exclusion from v1:

- Rust debug structs such as graph debug, orientation histograms, and other report-oriented payloads
- serialization-only compatibility helpers

### Recommended Rust Internal Mapping

- Chessboard handle wraps `calib_targets_chessboard::ChessboardDetector`.
- ChArUco handle wraps `calib_targets_charuco::CharucoDetector`.
- Marker-board handle wraps `calib_targets_marker::MarkerBoardDetector`.
- Shared image adapter converts `ct_gray_image_u8_t` to an internal grayscale view or owned buffer.
- Shared conversion layer maps fixed ABI structs into repo-native Rust types and maps stable Rust outputs back into fixed ABI structs and caller-owned arrays.

## Resolved Dictionary Scope

Decision:

- V1 supports built-in dictionary names only.

Rationale:

- The current Rust `Dictionary` type is built around static built-ins.
- A custom-dictionary ABI would add ownership, validation, and compatibility complexity to the very first C ABI release.
- Built-in names keep the board-spec ABI smaller and easier to stabilize.

Deferred follow-up:

- If a concrete downstream caller needs custom dictionaries, add that as a separate post-v1 backlog task with its own ABI design review.

## Implementation Plan

### Milestone 0: ABI Decision Record

Status:

- Complete.

Deliverables:

- Decision record in `docs/ffi/decision-record.md`.

### Milestone 1: Crate Scaffold and ABI Runtime Layer

Goal:

- Introduce the dedicated FFI crate without exposing detector APIs yet.

Work:

- Add `crates/calib-targets-ffi` to the workspace.
- Configure `cdylib` output and `cbindgen`.
- Define `ct_status_t`, shared image structs, version functions, and error buffer API.
- Define shared `repr(C)` config/result primitives and optional-value conventions.
- Add panic containment and internal error capture helpers.

Acceptance:

- Header generation works deterministically.
- A C smoke test can call version/error functions successfully.

### Milestone 2: Detector Handles and Detection Entry Points

Goal:

- Expose the minimal v1 functionality over the approved ABI.

Work:

- Add create/destroy APIs for chessboard, ChArUco, and marker-board detectors.
- Add fixed-struct detect calls using caller-owned output buffers and query/fill patterns.
- Map Rust `Option`/`Result` outcomes to explicit status codes.
- Expose full approved config surfaces, including ChESS config, from day 1.

Acceptance:

- Each detector can be created from approved fixed-struct config input.
- Each detector can process a grayscale image buffer and return stable fixed-struct results plus caller-owned arrays.
- No panic crosses the ABI boundary.

### Milestone 3: Header, Tests, and Packaging Hardening

Goal:

- Make the ABI reproducible and safe to consume.

Work:

- Add C integration tests.
- Add ABI smoke checks for null pointers, short buffers, invalid config, and not-found results.
- Check generated header drift in CI.

Acceptance:

- CI fails on header drift or FFI regressions.
- Ownership and error rules are documented and tested.

### Milestone 4: Thin C++ Wrapper

Goal:

- Add a convenience C++ layer without widening the C ABI.

Work:

- Implement RAII wrappers for detector handles.
- Wrap status/error handling into C++ exceptions or status objects, depending on project preference.
- Add small end-to-end examples.

Acceptance:

- The wrapper owns handles correctly and does not bypass the C ABI.
- Examples compile and run against the generated header/library.

## Recommended Backlog Breakdown

- `FFI-001` ABI decision record and scope freeze.
- `FFI-002` FFI crate scaffold and header generation.
- `FFI-003` Detector handles and detection entry points.
- `FFI-004` C examples, C++ wrapper, and CI/docs hardening.

## Current State

Major ABI decisions are now resolved.

Implementation can begin with `FFI-002`.
