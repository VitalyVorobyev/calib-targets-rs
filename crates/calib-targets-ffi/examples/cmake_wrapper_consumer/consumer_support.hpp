#ifndef CALIB_TARGETS_FFI_CMAKE_WRAPPER_CONSUMER_SUPPORT_HPP
#define CALIB_TARGETS_FFI_CMAKE_WRAPPER_CONSUMER_SUPPORT_HPP

#include <cctype>
#include <cerrno>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <limits>
#include <vector>

#include "calib_targets_ffi.h"

namespace consumer_support {

struct GrayImageBuffer {
  ct_gray_image_u8_t descriptor{};
  std::vector<std::uint8_t> pixels;
};

inline void reset_gray_image_buffer(GrayImageBuffer *image) {
  if (image == nullptr) {
    return;
  }

  image->descriptor = ct_gray_image_u8_t{};
  image->pixels.clear();
  image->pixels.shrink_to_fit();
}

inline bool read_pgm_token(std::FILE *file, char *out, std::size_t out_capacity) {
  int ch = 0;
  std::size_t len = 0;

  if (file == nullptr || out == nullptr || out_capacity == 0) {
    return false;
  }

  out[0] = '\0';
  for (;;) {
    ch = std::fgetc(file);
    if (ch == EOF) {
      return false;
    }
    if (std::isspace(static_cast<unsigned char>(ch)) != 0) {
      continue;
    }
    if (ch == '#') {
      while ((ch = std::fgetc(file)) != EOF && ch != '\n') {
      }
      if (ch == EOF) {
        return false;
      }
      continue;
    }
    break;
  }

  do {
    if (len + 1 >= out_capacity) {
      return false;
    }
    out[len++] = static_cast<char>(ch);
    ch = std::fgetc(file);
  } while (ch != EOF && std::isspace(static_cast<unsigned char>(ch)) == 0 && ch != '#');

  out[len] = '\0';
  if (ch == '#') {
    while ((ch = std::fgetc(file)) != EOF && ch != '\n') {
    }
  }
  return len != 0;
}

inline bool parse_u32(const char *token, std::uint32_t *out) {
  char *end = nullptr;
  unsigned long value = 0;

  if (token == nullptr || out == nullptr) {
    return false;
  }

  errno = 0;
  value = std::strtoul(token, &end, 10);
  if (errno != 0 || end == token || *end != '\0' || value > std::numeric_limits<std::uint32_t>::max()) {
    return false;
  }

  *out = static_cast<std::uint32_t>(value);
  return true;
}

inline bool load_binary_pgm(const char *path, GrayImageBuffer *out) {
  std::FILE *file = nullptr;
  char token[64];
  std::uint32_t width = 0;
  std::uint32_t height = 0;
  std::uint32_t max_value = 0;
  std::size_t pixel_bytes = 0;

  if (path == nullptr || out == nullptr) {
    return false;
  }

  reset_gray_image_buffer(out);
  file = std::fopen(path, "rb");
  if (file == nullptr) {
    return false;
  }

  if (!read_pgm_token(file, token, sizeof(token)) || std::strcmp(token, "P5") != 0) {
    std::fclose(file);
    return false;
  }
  if (!read_pgm_token(file, token, sizeof(token)) || !parse_u32(token, &width)) {
    std::fclose(file);
    return false;
  }
  if (!read_pgm_token(file, token, sizeof(token)) || !parse_u32(token, &height)) {
    std::fclose(file);
    return false;
  }
  if (!read_pgm_token(file, token, sizeof(token)) || !parse_u32(token, &max_value)) {
    std::fclose(file);
    return false;
  }
  if (max_value != 255) {
    std::fclose(file);
    return false;
  }

  pixel_bytes = static_cast<std::size_t>(width) * static_cast<std::size_t>(height);
  if (width == 0 || height == 0 || pixel_bytes / static_cast<std::size_t>(width) != static_cast<std::size_t>(height)) {
    std::fclose(file);
    return false;
  }

  out->pixels.assign(pixel_bytes, 0);
  if (std::fread(out->pixels.data(), 1, pixel_bytes, file) != pixel_bytes) {
    std::fclose(file);
    reset_gray_image_buffer(out);
    return false;
  }

  out->descriptor.width = width;
  out->descriptor.height = height;
  out->descriptor.stride_bytes = static_cast<std::size_t>(width);
  out->descriptor.data = out->pixels.data();
  std::fclose(file);
  return true;
}

inline ct_refiner_config_t default_refiner() {
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

inline ct_chess_config_t default_shared_chess_config() {
  ct_chess_config_t config{};
  config.params.use_radius10 = CT_FALSE;
  // ABI 3.0.0: single absolute response floor (was threshold_rel +
  // threshold_abs); 15.0 is the workspace production default.
  config.params.threshold = 15.0f;
  config.params.nms_radius = 2;
  config.params.min_cluster_size = 2;
  config.params.refiner = default_refiner();
  config.multiscale.pyramid.num_levels = 1;
  config.multiscale.pyramid.min_size = 128;
  config.multiscale.refinement_radius = 3;
  config.multiscale.merge_radius = 3.0f;
  config.upscale.mode = CT_UPSCALE_MODE_DISABLED;
  config.upscale.factor = 2;
  // ABI 6.0.0: estimate the pixel noise per image, propagated covariance.
  // Both are the value-initialised values; spelled out for the reader.
  config.noise_sigma.has_value = CT_FALSE;
  config.uncertainty = CT_UNCERTAINTY_PROPAGATED;
  return config;
}

inline ct_chessboard_detector_config_t default_chessboard_detector_config() {
  ct_chessboard_detector_config_t config{};
  config.chess = default_shared_chess_config();
  // C ABI: the chessboard detector's `ct_chessboard_params_t`
  // mirrors `DetectorParams`. The `init_default` helper populates a
  // valid default-configured value; callers override individual fields
  // as needed (here: raise `min_corner_strength` from 0 → 0.5).
  ct_chessboard_params_init_default(&config.chessboard);
  config.chessboard.min_corner_strength = 0.5f;
  return config;
}

// Number of corners whose `position_covariance` is set and positive definite,
// or `std::size_t(-1)` if a set covariance is not positive definite.
inline std::size_t count_valid_covariances(const std::vector<ct_chessboard_corner_t> &corners) {
  std::size_t valid = 0;
  for (const auto &corner : corners) {
    if (corner.has_position_covariance != CT_TRUE) {
      continue;
    }
    const auto &c = corner.position_covariance;
    if (!(c.xx > 0.0f && c.yy > 0.0f && c.xx * c.yy - c.xy * c.xy > 0.0f)) {
      return static_cast<std::size_t>(-1);
    }
    ++valid;
  }
  return valid;
}

// Whether a chessboard result's `quality` is internally consistent and
// populated, as it is for a lossless frame detected by the default config.
inline bool quality_is_sane(const ct_detection_quality_t &q, std::size_t corners_len) {
  return q.noise_sigma.has_value == CT_TRUE && q.noise_sigma.value >= 0.0f &&
         q.covariance_coverage >= 0.9f && q.covariance_coverage <= 1.0f &&
         q.median_sigma_px.has_value == CT_TRUE && q.median_sigma_px.value > 0.0f &&
         q.p95_sigma_px.has_value == CT_TRUE &&
         q.p95_sigma_px.value >= q.median_sigma_px.value && q.local_residual_count > 0 &&
         q.local_residual_count <= corners_len && q.local_residual_rms_px.has_value == CT_TRUE &&
         q.local_residual_rms_px.value > 0.0f && q.variance_factor_count > 0 &&
         q.variance_factor_count <= q.local_residual_count &&
         q.variance_factor.has_value == CT_TRUE && q.variance_factor.value > 0.0f;
}

}  // namespace consumer_support

#endif
