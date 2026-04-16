#ifndef CALIB_TARGETS_FFI_HPP
#define CALIB_TARGETS_FFI_HPP

#include <cstddef>
#include <string>
#include <utility>
#include <vector>

#include "calib_targets_ffi.h"

namespace calib_targets::ffi {

struct Status {
  ct_status_t code = CT_STATUS_OK;
  std::string message;

  bool ok() const noexcept {
    return code == CT_STATUS_OK;
  }
};

inline std::string last_error_message() {
  std::size_t len = 0;
  if (ct_last_error_message(nullptr, 0, &len) != CT_STATUS_OK || len == 0) {
    return {};
  }

  std::string message(len + 1, '\0');
  if (ct_last_error_message(message.data(), message.size(), &len) != CT_STATUS_OK) {
    return {};
  }
  message.resize(len);
  return message;
}

inline Status capture_status(ct_status_t code) {
  Status status;
  status.code = code;
  if (code != CT_STATUS_OK) {
    status.message = last_error_message();
  }
  return status;
}

inline Status local_status(ct_status_t code, std::string message) {
  Status status;
  status.code = code;
  status.message = std::move(message);
  return status;
}

template <typename HandleT, void (*DestroyFn)(HandleT *)>
class UniqueHandle {
 public:
  UniqueHandle() = default;

  explicit UniqueHandle(HandleT *handle) noexcept
      : handle_(handle) {}

  ~UniqueHandle() {
    reset();
  }

  UniqueHandle(const UniqueHandle &) = delete;
  UniqueHandle &operator=(const UniqueHandle &) = delete;

  UniqueHandle(UniqueHandle &&other) noexcept
      : handle_(other.release()) {}

  UniqueHandle &operator=(UniqueHandle &&other) noexcept {
    if (this != &other) {
      reset(other.release());
    }
    return *this;
  }

  void reset(HandleT *handle = nullptr) noexcept {
    if (handle_ != nullptr) {
      DestroyFn(handle_);
    }
    handle_ = handle;
  }

  [[nodiscard]] HandleT *get() const noexcept {
    return handle_;
  }

  [[nodiscard]] HandleT *release() noexcept {
    HandleT *released = handle_;
    handle_ = nullptr;
    return released;
  }

  [[nodiscard]] bool has_value() const noexcept {
    return handle_ != nullptr;
  }

 private:
  HandleT *handle_ = nullptr;
};

struct CharucoBuffers {
  std::vector<ct_labeled_corner_t> corners;
  std::vector<ct_marker_detection_t> markers;
};

struct MarkerBoardBuffers {
  std::vector<ct_labeled_corner_t> corners;
  std::vector<ct_circle_candidate_t> circle_candidates;
  std::vector<ct_circle_match_t> circle_matches;
};

struct PuzzleBoardBuffers {
  std::vector<ct_labeled_corner_t> corners;
};

class ChessboardDetector {
 public:
  ChessboardDetector() = default;
  ChessboardDetector(const ChessboardDetector &) = delete;
  ChessboardDetector &operator=(const ChessboardDetector &) = delete;
  ChessboardDetector(ChessboardDetector &&) noexcept = default;
  ChessboardDetector &operator=(ChessboardDetector &&) noexcept = default;

  [[nodiscard]] Status create(const ct_chessboard_detector_config_t &config) noexcept {
    ct_chessboard_detector_t *raw = nullptr;
    const auto code = ct_chessboard_detector_create(&config, &raw);
    if (code != CT_STATUS_OK) {
      return capture_status(code);
    }
    handle_.reset(raw);
    return {};
  }

  [[nodiscard]] Status detect(
      const ct_gray_image_u8_t &image,
      ct_chessboard_result_t *out_result,
      std::vector<ct_labeled_corner_t> *out_corners) const {
    if (!handle_.has_value()) {
      return local_status(CT_STATUS_INVALID_ARGUMENT, "chessboard detector is not initialized");
    }

    std::size_t corners_len = 0;
    ct_chessboard_result_t ignored_result{};
    auto code = ct_chessboard_detector_detect(
        handle_.get(),
        &image,
        out_result != nullptr ? out_result : &ignored_result,
        nullptr,
        0,
        &corners_len);
    if (code != CT_STATUS_OK) {
      return capture_status(code);
    }
    if (out_corners == nullptr) {
      return {};
    }

    out_corners->assign(corners_len, ct_labeled_corner_t{});
    code = ct_chessboard_detector_detect(
        handle_.get(),
        &image,
        out_result != nullptr ? out_result : &ignored_result,
        out_corners->empty() ? nullptr : out_corners->data(),
        out_corners->size(),
        &corners_len);
    if (code != CT_STATUS_OK) {
      out_corners->clear();
      return capture_status(code);
    }
    out_corners->resize(corners_len);
    return {};
  }

  [[nodiscard]] bool initialized() const noexcept {
    return handle_.has_value();
  }

 private:
  UniqueHandle<ct_chessboard_detector_t, ct_chessboard_detector_destroy> handle_;
};

class CharucoDetector {
 public:
  CharucoDetector() = default;
  CharucoDetector(const CharucoDetector &) = delete;
  CharucoDetector &operator=(const CharucoDetector &) = delete;
  CharucoDetector(CharucoDetector &&) noexcept = default;
  CharucoDetector &operator=(CharucoDetector &&) noexcept = default;

  [[nodiscard]] Status create(const ct_charuco_detector_config_t &config) noexcept {
    ct_charuco_detector_t *raw = nullptr;
    const auto code = ct_charuco_detector_create(&config, &raw);
    if (code != CT_STATUS_OK) {
      return capture_status(code);
    }
    handle_.reset(raw);
    return {};
  }

  [[nodiscard]] Status detect(
      const ct_gray_image_u8_t &image,
      ct_charuco_result_t *out_result,
      CharucoBuffers *out_buffers) const {
    if (!handle_.has_value()) {
      return local_status(CT_STATUS_INVALID_ARGUMENT, "charuco detector is not initialized");
    }

    std::size_t corners_len = 0;
    std::size_t markers_len = 0;
    ct_charuco_result_t ignored_result{};
    auto code = ct_charuco_detector_detect(
        handle_.get(),
        &image,
        out_result != nullptr ? out_result : &ignored_result,
        nullptr,
        0,
        &corners_len,
        nullptr,
        0,
        &markers_len);
    if (code != CT_STATUS_OK) {
      return capture_status(code);
    }
    if (out_buffers == nullptr) {
      return {};
    }

    out_buffers->corners.assign(corners_len, ct_labeled_corner_t{});
    out_buffers->markers.assign(markers_len, ct_marker_detection_t{});
    code = ct_charuco_detector_detect(
        handle_.get(),
        &image,
        out_result != nullptr ? out_result : &ignored_result,
        out_buffers->corners.empty() ? nullptr : out_buffers->corners.data(),
        out_buffers->corners.size(),
        &corners_len,
        out_buffers->markers.empty() ? nullptr : out_buffers->markers.data(),
        out_buffers->markers.size(),
        &markers_len);
    if (code != CT_STATUS_OK) {
      out_buffers->corners.clear();
      out_buffers->markers.clear();
      return capture_status(code);
    }
    out_buffers->corners.resize(corners_len);
    out_buffers->markers.resize(markers_len);
    return {};
  }

  [[nodiscard]] bool initialized() const noexcept {
    return handle_.has_value();
  }

 private:
  UniqueHandle<ct_charuco_detector_t, ct_charuco_detector_destroy> handle_;
};

class MarkerBoardDetector {
 public:
  MarkerBoardDetector() = default;
  MarkerBoardDetector(const MarkerBoardDetector &) = delete;
  MarkerBoardDetector &operator=(const MarkerBoardDetector &) = delete;
  MarkerBoardDetector(MarkerBoardDetector &&) noexcept = default;
  MarkerBoardDetector &operator=(MarkerBoardDetector &&) noexcept = default;

  [[nodiscard]] Status create(const ct_marker_board_detector_config_t &config) noexcept {
    ct_marker_board_detector_t *raw = nullptr;
    const auto code = ct_marker_board_detector_create(&config, &raw);
    if (code != CT_STATUS_OK) {
      return capture_status(code);
    }
    handle_.reset(raw);
    return {};
  }

  [[nodiscard]] Status detect(
      const ct_gray_image_u8_t &image,
      ct_marker_board_result_t *out_result,
      MarkerBoardBuffers *out_buffers) const {
    if (!handle_.has_value()) {
      return local_status(CT_STATUS_INVALID_ARGUMENT, "marker-board detector is not initialized");
    }

    std::size_t corners_len = 0;
    std::size_t candidates_len = 0;
    std::size_t matches_len = 0;
    ct_marker_board_result_t ignored_result{};
    auto code = ct_marker_board_detector_detect(
        handle_.get(),
        &image,
        out_result != nullptr ? out_result : &ignored_result,
        nullptr,
        0,
        &corners_len,
        nullptr,
        0,
        &candidates_len,
        nullptr,
        0,
        &matches_len);
    if (code != CT_STATUS_OK) {
      return capture_status(code);
    }
    if (out_buffers == nullptr) {
      return {};
    }

    out_buffers->corners.assign(corners_len, ct_labeled_corner_t{});
    out_buffers->circle_candidates.assign(candidates_len, ct_circle_candidate_t{});
    out_buffers->circle_matches.assign(matches_len, ct_circle_match_t{});
    code = ct_marker_board_detector_detect(
        handle_.get(),
        &image,
        out_result != nullptr ? out_result : &ignored_result,
        out_buffers->corners.empty() ? nullptr : out_buffers->corners.data(),
        out_buffers->corners.size(),
        &corners_len,
        out_buffers->circle_candidates.empty() ? nullptr : out_buffers->circle_candidates.data(),
        out_buffers->circle_candidates.size(),
        &candidates_len,
        out_buffers->circle_matches.empty() ? nullptr : out_buffers->circle_matches.data(),
        out_buffers->circle_matches.size(),
        &matches_len);
    if (code != CT_STATUS_OK) {
      out_buffers->corners.clear();
      out_buffers->circle_candidates.clear();
      out_buffers->circle_matches.clear();
      return capture_status(code);
    }
    out_buffers->corners.resize(corners_len);
    out_buffers->circle_candidates.resize(candidates_len);
    out_buffers->circle_matches.resize(matches_len);
    return {};
  }

  [[nodiscard]] bool initialized() const noexcept {
    return handle_.has_value();
  }

 private:
  UniqueHandle<ct_marker_board_detector_t, ct_marker_board_detector_destroy> handle_;
};

class PuzzleBoardDetector {
 public:
  PuzzleBoardDetector() = default;
  PuzzleBoardDetector(const PuzzleBoardDetector &) = delete;
  PuzzleBoardDetector &operator=(const PuzzleBoardDetector &) = delete;
  PuzzleBoardDetector(PuzzleBoardDetector &&) noexcept = default;
  PuzzleBoardDetector &operator=(PuzzleBoardDetector &&) noexcept = default;

  [[nodiscard]] Status create(const ct_puzzleboard_detector_config_t &config) noexcept {
    ct_puzzleboard_detector_t *raw = nullptr;
    const auto code = ct_puzzleboard_detector_create(&config, &raw);
    if (code != CT_STATUS_OK) {
      return capture_status(code);
    }
    handle_.reset(raw);
    return {};
  }

  [[nodiscard]] Status detect(
      const ct_gray_image_u8_t &image,
      ct_puzzleboard_result_t *out_result,
      PuzzleBoardBuffers *out_buffers) const {
    if (!handle_.has_value()) {
      return local_status(CT_STATUS_INVALID_ARGUMENT, "puzzleboard detector is not initialized");
    }

    std::size_t corners_len = 0;
    ct_puzzleboard_result_t ignored_result{};
    auto code = ct_puzzleboard_detector_detect(
        handle_.get(),
        &image,
        out_result != nullptr ? out_result : &ignored_result,
        nullptr,
        0,
        &corners_len);
    if (code != CT_STATUS_OK) {
      return capture_status(code);
    }
    if (out_buffers == nullptr) {
      return {};
    }

    out_buffers->corners.assign(corners_len, ct_labeled_corner_t{});
    code = ct_puzzleboard_detector_detect(
        handle_.get(),
        &image,
        out_result != nullptr ? out_result : &ignored_result,
        out_buffers->corners.empty() ? nullptr : out_buffers->corners.data(),
        out_buffers->corners.size(),
        &corners_len);
    if (code != CT_STATUS_OK) {
      out_buffers->corners.clear();
      return capture_status(code);
    }
    out_buffers->corners.resize(corners_len);
    return {};
  }

  [[nodiscard]] bool initialized() const noexcept {
    return handle_.has_value();
  }

 private:
  UniqueHandle<ct_puzzleboard_detector_t, ct_puzzleboard_detector_destroy> handle_;
};

}  // namespace calib_targets::ffi

#endif
