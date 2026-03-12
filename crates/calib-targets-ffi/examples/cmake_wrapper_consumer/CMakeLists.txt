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
