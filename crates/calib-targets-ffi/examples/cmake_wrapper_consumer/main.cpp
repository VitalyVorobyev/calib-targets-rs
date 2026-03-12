#include <iostream>
#include <string>
#include <vector>

#include "calib_targets_ffi.hpp"
#include "consumer_support.hpp"

int main(int argc, char **argv) {
  consumer_support::GrayImageBuffer image{};
  auto config = consumer_support::default_chessboard_detector_config();
  calib_targets::ffi::ChessboardDetector detector;
  ct_chessboard_result_t result{};
  std::vector<ct_labeled_corner_t> corners;

  if (argc != 2) {
    std::cerr << "usage: " << argv[0] << " <mid.pgm>\n";
    return 2;
  }

  if (ct_version_string() == nullptr || std::string(ct_version_string()).empty()) {
    std::cerr << "ct_version_string returned an empty value\n";
    return 1;
  }

  if (!consumer_support::load_binary_pgm(argv[1], &image)) {
    std::cerr << "failed to load PGM image " << argv[1] << "\n";
    return 1;
  }

  auto status = detector.detect(image.descriptor, &result, &corners);
  if (status.code != CT_STATUS_INVALID_ARGUMENT ||
      status.message.find("not initialized") == std::string::npos) {
    std::cerr << "expected uninitialized wrapper failure, got code=" << status.code
              << " message=" << status.message << "\n";
    consumer_support::reset_gray_image_buffer(&image);
    return 1;
  }

  status = detector.create(config);
  if (!status.ok()) {
    std::cerr << "failed to create chessboard detector: " << status.message << "\n";
    consumer_support::reset_gray_image_buffer(&image);
    return 1;
  }

  status = detector.detect(image.descriptor, &result, &corners);
  if (!status.ok()) {
    std::cerr << "wrapper detect failed: " << status.message << "\n";
    consumer_support::reset_gray_image_buffer(&image);
    return 1;
  }
  if (result.detection.kind != CT_TARGET_KIND_CHESSBOARD || corners.size() != 77 ||
      corners.front().has_grid != CT_TRUE) {
    std::cerr << "unexpected wrapper result: kind=" << result.detection.kind
              << " corners=" << corners.size() << "\n";
    consumer_support::reset_gray_image_buffer(&image);
    return 1;
  }

  consumer_support::reset_gray_image_buffer(&image);
  return 0;
}
