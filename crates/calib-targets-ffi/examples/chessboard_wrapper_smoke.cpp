#include <cstring>
#include <iostream>
#include <utility>
#include <vector>

#include "calib_targets_ffi.hpp"
#include "native_smoke_common.h"

int main(int argc, char **argv) {
  ct_native_gray_image_buffer_t image{};
  auto config = ct_native_default_chessboard_detector_config();
  calib_targets::ffi::ChessboardDetector detector;
  ct_chessboard_result_t result{};
  std::vector<ct_labeled_corner_t> corners;

  if (argc != 2) {
    std::cerr << "usage: " << argv[0] << " <mid.pgm>\n";
    return 2;
  }

  if (!ct_native_load_binary_pgm(argv[1], &image)) {
    std::cerr << "failed to load PGM image " << argv[1] << "\n";
    return 1;
  }

  auto status = detector.create(config);
  if (!status.ok()) {
    std::cerr << "failed to create chessboard detector: " << status.message << "\n";
    ct_native_gray_image_buffer_reset(&image);
    return 1;
  }

  status = detector.detect(image.descriptor, &result, &corners);
  if (!status.ok()) {
    std::cerr << "wrapper detect failed: " << status.message << "\n";
    ct_native_gray_image_buffer_reset(&image);
    return 1;
  }
  if (result.detection.kind != CT_TARGET_KIND_CHESSBOARD || corners.size() != 77 || corners.front().has_grid != CT_TRUE) {
    std::cerr << "unexpected wrapper result: kind=" << result.detection.kind
              << " corners=" << corners.size() << "\n";
    ct_native_gray_image_buffer_reset(&image);
    return 1;
  }

  calib_targets::ffi::ChessboardDetector moved = std::move(detector);
  corners.clear();
  std::memset(&result, 0, sizeof(result));
  status = moved.detect(image.descriptor, &result, &corners);
  if (!status.ok() || corners.empty()) {
    std::cerr << "moved wrapper detect failed: " << status.message << "\n";
    ct_native_gray_image_buffer_reset(&image);
    return 1;
  }

  ct_native_gray_image_buffer_reset(&image);
  return 0;
}
