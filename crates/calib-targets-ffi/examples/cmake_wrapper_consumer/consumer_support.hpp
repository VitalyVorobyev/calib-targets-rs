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

inline ct_optional_u32_t some_u32(std::uint32_t value) {
  ct_optional_u32_t out{};
  out.has_value = CT_TRUE;
  out.value = value;
  return out;
}

inline ct_optional_bool_t none_bool() {
  ct_optional_bool_t out{};
  out.has_value = CT_FALSE;
  out.value = CT_FALSE;
  return out;
}

inline ct_optional_f32_t none_f32() {
  ct_optional_f32_t out{};
  out.has_value = CT_FALSE;
  out.value = 0.0f;
  return out;
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

inline ct_orientation_clustering_params_t default_orientation_clustering() {
  ct_orientation_clustering_params_t params{};
  params.num_bins = 90;
  params.max_iters = 10;
  params.peak_min_separation_deg = 10.0f;
  params.outlier_threshold_deg = 30.0f;
  params.min_peak_weight_fraction = 0.05f;
  params.use_weights = CT_TRUE;
  return params;
}

inline ct_chess_config_t default_shared_chess_config() {
  ct_chess_config_t config{};
  config.params.use_radius10 = CT_FALSE;
  config.params.descriptor_use_radius10 = none_bool();
  config.params.threshold_rel = 0.2f;
  config.params.threshold_abs = none_f32();
  config.params.nms_radius = 2;
  config.params.min_cluster_size = 2;
  config.params.refiner = default_refiner();
  config.multiscale.pyramid.num_levels = 1;
  config.multiscale.pyramid.min_size = 128;
  config.multiscale.refinement_radius = 3;
  config.multiscale.merge_radius = 3.0f;
  return config;
}

inline ct_chessboard_detector_config_t default_chessboard_detector_config() {
  ct_chessboard_detector_config_t config{};
  config.chess = default_shared_chess_config();
  config.chessboard.min_corner_strength = 0.5f;
  config.chessboard.min_corners = 20;
  config.chessboard.expected_rows = some_u32(7);
  config.chessboard.expected_cols = some_u32(11);
  config.chessboard.completeness_threshold = 0.9f;
  config.chessboard.use_orientation_clustering = CT_TRUE;
  config.chessboard.orientation_clustering_params = default_orientation_clustering();
  config.graph.min_spacing_pix = 10.0f;
  config.graph.max_spacing_pix = 120.0f;
  config.graph.k_neighbors = 8;
  config.graph.orientation_tolerance_deg = 22.5f;
  return config;
}

}  // namespace consumer_support

#endif
