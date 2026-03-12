# CMake Consumer Quickstart

This is the shortest supported path today for a downstream native consumer that
wants to use `calib-targets-ffi` from a CMake project.

It assumes:

- you have either a tagged native release archive or a checkout of this repo
- you want the current ergonomic path: CMake + the header-only C++ wrapper

If you want the broader ABI/reference guide, see
[`docs/ffi/README.md`](./README.md). This quickstart is intentionally focused on
the first working integration.

## What Files You Actually Need

Your consumer project does not need the full Rust workspace at runtime. It
needs a staged native package prefix containing:

- `include/calib_targets_ffi.h`
- `include/calib_targets_ffi.hpp`
- `lib/libcalib_targets_ffi.so` on Linux, `lib/libcalib_targets_ffi.dylib` on
  macOS, or `lib/calib_targets_ffi.dll` on Windows
- `lib/cmake/calib_targets_ffi/calib_targets_ffi-config.cmake`
- `lib/cmake/calib_targets_ffi/calib_targets_ffi-config-version.cmake`

Those files are enough for a CMake consumer that uses:

- `calib_targets_ffi::c` for the raw C ABI
- `calib_targets_ffi::cpp` for the thin header-only C++ wrapper

## How To Get Those Files Today

From a supported tagged release, download and unpack the native archive for your
platform:

- Linux and macOS: `calib-targets-ffi-<version>-<platform>.tar.gz`
- Windows: `calib-targets-ffi-<version>-<platform>.zip`

After unpacking, you can either:

- point `CMAKE_PREFIX_PATH` at the unpacked top-level directory directly, or
- copy that directory into your own repo under something like
  `third_party/calib-targets-ffi/`

If you are working from a repo checkout instead, create the same layout locally:

From the workspace root:

```bash
cargo build -p calib-targets-ffi --release
cargo run -p calib-targets-ffi --bin stage-cmake-package -- \
  --lib-dir target/release \
  --prefix target/ffi-cmake-package
```

That creates a staged package prefix at:

```text
target/ffi-cmake-package/
```

Current limitation:

- there is still no crates.io/package-manager distribution or installer flow;
  the supported distribution format is the native release archive itself

If you want to rehearse that exact release-archive payload from a repo checkout,
run `package-release-archive` instead; the broader C API guide shows that
command and the expected archive layout.

## Minimal Consumer Project Layout

One workable layout is:

```text
my-app/
  CMakeLists.txt
  main.cpp
  third_party/
    calib-targets-ffi/
      include/
      lib/
```

Where `third_party/calib-targets-ffi/` is either the unpacked native release
archive directory or the staged package prefix copied from
`target/ffi-cmake-package/`.

## Minimal `CMakeLists.txt`

```cmake
cmake_minimum_required(VERSION 3.16)

project(calib_targets_quickstart LANGUAGES CXX)

find_package(calib_targets_ffi CONFIG REQUIRED)

add_executable(calib_targets_quickstart main.cpp)
target_compile_features(calib_targets_quickstart PRIVATE cxx_std_17)
target_link_libraries(calib_targets_quickstart PRIVATE calib_targets_ffi::cpp)

set_property(
  TARGET calib_targets_quickstart
  PROPERTY BUILD_RPATH "$<TARGET_FILE_DIR:calib_targets_ffi::c>"
)
```

Configure it with:

```bash
cmake -S . -B build \
  -DCMAKE_PREFIX_PATH=$PWD/third_party/calib-targets-ffi
cmake --build build
```

If you keep the staged package in the original workspace instead of copying it,
set `CMAKE_PREFIX_PATH` to that staged directory instead.

## Minimal `main.cpp`

This example does not depend on repo-internal helper headers. It builds a small
blank grayscale image in memory, creates a chessboard detector, and treats
`CT_STATUS_NOT_FOUND` as success because a blank image should contain no target.

```cpp
#include <cstdint>
#include <iostream>
#include <vector>

#include "calib_targets_ffi.hpp"

namespace {

ct_optional_bool_t none_bool() {
  ct_optional_bool_t value{};
  value.has_value = CT_FALSE;
  value.value = CT_FALSE;
  return value;
}

ct_optional_f32_t none_f32() {
  ct_optional_f32_t value{};
  value.has_value = CT_FALSE;
  value.value = 0.0f;
  return value;
}

ct_optional_u32_t some_u32(std::uint32_t value) {
  ct_optional_u32_t out{};
  out.has_value = CT_TRUE;
  out.value = value;
  return out;
}

ct_refiner_config_t default_refiner() {
  ct_refiner_config_t config{};
  config.kind = CT_REFINER_KIND_CENTER_OF_MASS;
  config.center_of_mass.radius = 2;
  config.forstner.radius = 2;
  config.forstner.min_trace = 25.0f;
  config.forstner.min_det = 1e-3f;
  config.forstner.max_condition_number = 50.0f;
  config.forstner.max_offset = 1.5f;
  config.saddle_point.radius = 2;
  config.saddle_point.det_margin = 1e-3f;
  config.saddle_point.max_offset = 1.5f;
  config.saddle_point.min_abs_det = 1e-4f;
  return config;
}

ct_orientation_clustering_params_t default_orientation_clustering() {
  ct_orientation_clustering_params_t params{};
  params.num_bins = 90;
  params.max_iters = 10;
  params.peak_min_separation_deg = 10.0f;
  params.outlier_threshold_deg = 30.0f;
  params.min_peak_weight_fraction = 0.05f;
  params.use_weights = CT_TRUE;
  return params;
}

ct_chessboard_detector_config_t default_chessboard_config() {
  ct_chessboard_detector_config_t config{};

  config.chess.params.use_radius10 = CT_FALSE;
  config.chess.params.descriptor_use_radius10 = none_bool();
  config.chess.params.threshold_rel = 0.2f;
  config.chess.params.threshold_abs = none_f32();
  config.chess.params.nms_radius = 2;
  config.chess.params.min_cluster_size = 2;
  config.chess.params.refiner = default_refiner();
  config.chess.multiscale.pyramid.num_levels = 1;
  config.chess.multiscale.pyramid.min_size = 128;
  config.chess.multiscale.refinement_radius = 3;
  config.chess.multiscale.merge_radius = 3.0f;

  config.chessboard.min_corner_strength = 0.5f;
  config.chessboard.min_corners = 20;
  config.chessboard.expected_rows = some_u32(7);
  config.chessboard.expected_cols = some_u32(11);
  config.chessboard.completeness_threshold = 0.9f;
  config.chessboard.use_orientation_clustering = CT_TRUE;
  config.chessboard.orientation_clustering_params =
      default_orientation_clustering();

  config.graph.min_spacing_pix = 10.0f;
  config.graph.max_spacing_pix = 120.0f;
  config.graph.k_neighbors = 8;
  config.graph.orientation_tolerance_deg = 22.5f;

  return config;
}

}  // namespace

int main() {
  std::vector<std::uint8_t> pixels(64 * 64, 0);
  ct_gray_image_u8_t image{};
  image.width = 64;
  image.height = 64;
  image.stride_bytes = 64;
  image.data = pixels.data();

  calib_targets::ffi::ChessboardDetector detector;
  auto status = detector.create(default_chessboard_config());
  if (!status.ok()) {
    std::cerr << "create failed: " << status.message << "\n";
    return 1;
  }

  ct_chessboard_result_t result{};
  std::vector<ct_labeled_corner_t> corners;
  status = detector.detect(image, &result, &corners);

  if (status.code == CT_STATUS_NOT_FOUND) {
    std::cout << "detector call succeeded; no target found in blank image\n";
    return 0;
  }

  if (!status.ok()) {
    std::cerr << "detect failed: " << status.message << "\n";
    return 1;
  }

  std::cout << "unexpectedly found " << corners.size() << " corners\n";
  return 1;
}
```

## Build And Run

With the staged package copied under `third_party/calib-targets-ffi/`:

```bash
cmake -S . -B build \
  -DCMAKE_PREFIX_PATH=$PWD/third_party/calib-targets-ffi
cmake --build build
./build/calib_targets_quickstart
```

Expected output:

```text
detector call succeeded; no target found in blank image
```

On Windows, make sure `third_party/calib-targets-ffi/lib` is on `PATH` before
running the executable, or copy `calib_targets_ffi.dll` next to the built
binary.

## If You Want Pure C Instead

Use the same staged package, but link the C target instead:

```cmake
target_link_libraries(your_c_app PRIVATE calib_targets_ffi::c)
```

Then include:

```c
#include "calib_targets_ffi.h"
```

You will need to manage the two-call query/fill pattern yourself for variable
length outputs. The broader C ABI details remain documented in
[`docs/ffi/README.md`](./README.md).
