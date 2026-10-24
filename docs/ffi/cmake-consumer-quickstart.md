# CMake Consumer Quickstart

The shortest path from a native release archive (or a repo checkout) to a
working CMake consumer using the header-only C++ wrapper. The full ABI
reference is the [C API guide](./README.md).

## 1. Get the package prefix

Download and unpack the archive for your platform from a tagged release
(`calib-targets-ffi-<abi-version>-<platform>.tar.gz`, or `.zip` on Windows),
or stage the same layout from a checkout:

```bash
cargo build -p calib-targets-ffi --release
cargo run -p calib-targets-ffi --bin stage-cmake-package -- \
  --lib-dir target/release --prefix target/ffi-cmake-package
```

Either way you get:

```text
<prefix>/
  include/calib_targets_ffi.h
  include/calib_targets_ffi.hpp
  lib/libcalib_targets_ffi.{so,dylib}   (calib_targets_ffi.dll on Windows)
  lib/cmake/calib_targets_ffi/calib_targets_ffi-config.cmake
  lib/cmake/calib_targets_ffi/calib_targets_ffi-config-version.cmake
```

Copy it into your project (for example `third_party/calib-targets-ffi/`) or
point at it in place.

## 2. `CMakeLists.txt`

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

A pure-C consumer links `calib_targets_ffi::c` instead, includes
`calib_targets_ffi.h`, and runs the query/fill pattern itself.

## 3. `main.cpp`

Creates a chessboard detector and runs it on a blank in-memory image, so
`CT_STATUS_NOT_FOUND` is the expected outcome. `ct_chessboard_params_init_default`
fills the detector params; the ChESS front-end has no init helper, so it is
filled explicitly with the workspace defaults.

```cpp
#include <cstdint>
#include <iostream>
#include <vector>

#include "calib_targets_ffi.hpp"

static ct_chessboard_detector_config_t default_config() {
  ct_chessboard_detector_config_t config{};

  ct_chess_params_t &chess = config.chess.params;
  chess.use_radius10 = CT_FALSE;
  chess.threshold = 15.0f;
  chess.nms_radius = 2;
  chess.min_cluster_size = 2;
  chess.refiner.kind = CT_REFINER_KIND_CENTER_OF_MASS;
  chess.refiner.center_of_mass.radius = 2;
  chess.refiner.forstner = {2, 25.0f, 1e-3f, 50.0f, 1.5f};
  chess.refiner.saddle_point = {2, 1e-3f, 1.5f, 1e-4f};

  config.chess.multiscale.pyramid.num_levels = 1;
  config.chess.multiscale.pyramid.min_size = 128;
  config.chess.multiscale.refinement_radius = 3;
  config.chess.multiscale.merge_radius = 3.0f;
  config.chess.upscale.mode = CT_UPSCALE_MODE_DISABLED;
  config.chess.upscale.factor = 2;

  ct_chessboard_params_init_default(&config.chessboard);
  return config;
}

int main() {
  std::vector<std::uint8_t> pixels(64 * 64, 0);
  ct_gray_image_u8_t image{};
  image.width = 64;
  image.height = 64;
  image.stride_bytes = 64;
  image.data = pixels.data();

  calib_targets::ffi::ChessboardDetector detector;
  auto status = detector.create(default_config());
  if (!status.ok()) {
    std::cerr << "create failed: " << status.message << "\n";
    return 1;
  }

  ct_chessboard_result_t result{};
  std::vector<ct_chessboard_corner_t> corners;
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

## 4. Build and run

```bash
cmake -S . -B build -DCMAKE_PREFIX_PATH=$PWD/third_party/calib-targets-ffi
cmake --build build
./build/calib_targets_quickstart
# detector call succeeded; no target found in blank image
```

On Windows, put `third_party/calib-targets-ffi/lib` on `PATH` or copy
`calib_targets_ffi.dll` next to the executable.

The repo's own end-to-end version of this flow (PGM input, CI-tested) is
`crates/calib-targets-ffi/examples/cmake_wrapper_consumer/`.
