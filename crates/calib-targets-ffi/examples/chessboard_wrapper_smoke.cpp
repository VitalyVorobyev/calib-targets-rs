#include <cstring>
#include <iostream>
#include <string>
#include <utility>
#include <vector>

#include "calib_targets_ffi.hpp"
#include "native_smoke_common.h"

// Expected labelled-corner count for testdata/mid.pgm under the default
// chessboard detector config (min_corner_strength overridden to 0.5).
namespace {
constexpr std::size_t kExpectedCorners = 77;
}  // namespace

int main(int argc, char **argv) {
  ct_native_gray_image_buffer_t image{};
  auto config = ct_native_default_chessboard_detector_config();
  calib_targets::ffi::ChessboardDetector detector;
  ct_chessboard_result_t result{};
  std::vector<ct_chessboard_corner_t> corners;

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
  if (result.corners_len != kExpectedCorners || corners.size() != kExpectedCorners) {
    std::cerr << "unexpected wrapper result: result.corners_len=" << result.corners_len
              << " corners=" << corners.size() << "\n";
    ct_native_gray_image_buffer_reset(&image);
    return 1;
  }
  const std::size_t with_cov = ct_native_count_valid_covariances(corners.data(), corners.size());
  if (with_cov == static_cast<std::size_t>(-1) || with_cov * 10 < corners.size() * 9) {
    std::cerr << "unexpected position covariances: valid=" << with_cov << " of "
              << corners.size() << "\n";
    ct_native_gray_image_buffer_reset(&image);
    return 1;
  }
  if (!ct_native_quality_is_sane(&result.quality, corners.size())) {
    std::cerr << "unexpected detection quality: coverage=" << result.quality.covariance_coverage
              << " local=" << result.quality.local_residual_count << "\n";
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
