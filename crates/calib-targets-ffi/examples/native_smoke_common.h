#ifndef CALIB_TARGETS_FFI_NATIVE_SMOKE_COMMON_H
#define CALIB_TARGETS_FFI_NATIVE_SMOKE_COMMON_H

#include <ctype.h>
#include <errno.h>
#include <limits.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "calib_targets_ffi.h"

typedef struct ct_native_gray_image_buffer_t {
  ct_gray_image_u8_t descriptor;
  uint8_t *pixels;
} ct_native_gray_image_buffer_t;

static void ct_native_gray_image_buffer_reset(ct_native_gray_image_buffer_t *image) {
  if (image == NULL) {
    return;
  }
  free(image->pixels);
  memset(image, 0, sizeof(*image));
}

static int ct_native_read_pgm_token(FILE *file, char *out, size_t out_capacity) {
  int ch = 0;
  size_t len = 0;

  if (file == NULL || out == NULL || out_capacity == 0) {
    return 0;
  }

  out[0] = '\0';
  for (;;) {
    ch = fgetc(file);
    if (ch == EOF) {
      return 0;
    }
    if (isspace((unsigned char)ch)) {
      continue;
    }
    if (ch == '#') {
      while ((ch = fgetc(file)) != EOF && ch != '\n') {
      }
      if (ch == EOF) {
        return 0;
      }
      continue;
    }
    break;
  }

  do {
    if (len + 1 >= out_capacity) {
      return 0;
    }
    out[len++] = (char)ch;
    ch = fgetc(file);
  } while (ch != EOF && !isspace((unsigned char)ch) && ch != '#');

  out[len] = '\0';
  if (ch == '#') {
    while ((ch = fgetc(file)) != EOF && ch != '\n') {
    }
  }
  return len != 0;
}

static int ct_native_parse_u32(const char *token, uint32_t *out) {
  char *end = NULL;
  unsigned long value = 0;

  if (token == NULL || out == NULL) {
    return 0;
  }

  errno = 0;
  value = strtoul(token, &end, 10);
  if (errno != 0 || end == token || *end != '\0' || value > UINT32_MAX) {
    return 0;
  }
  *out = (uint32_t)value;
  return 1;
}

static int ct_native_load_binary_pgm(const char *path, ct_native_gray_image_buffer_t *out) {
  FILE *file = NULL;
  char token[64];
  uint32_t width = 0;
  uint32_t height = 0;
  uint32_t max_value = 0;
  size_t pixel_bytes = 0;

  if (path == NULL || out == NULL) {
    return 0;
  }

  memset(out, 0, sizeof(*out));
  file = fopen(path, "rb");
  if (file == NULL) {
    return 0;
  }

  if (!ct_native_read_pgm_token(file, token, sizeof(token)) || strcmp(token, "P5") != 0) {
    fclose(file);
    return 0;
  }
  if (!ct_native_read_pgm_token(file, token, sizeof(token)) || !ct_native_parse_u32(token, &width)) {
    fclose(file);
    return 0;
  }
  if (!ct_native_read_pgm_token(file, token, sizeof(token)) || !ct_native_parse_u32(token, &height)) {
    fclose(file);
    return 0;
  }
  if (!ct_native_read_pgm_token(file, token, sizeof(token)) || !ct_native_parse_u32(token, &max_value)) {
    fclose(file);
    return 0;
  }
  if (max_value != 255) {
    fclose(file);
    return 0;
  }

  pixel_bytes = (size_t)width * (size_t)height;
  if (width == 0 || height == 0 || pixel_bytes / (size_t)width != (size_t)height) {
    fclose(file);
    return 0;
  }

  out->pixels = (uint8_t *)malloc(pixel_bytes);
  if (out->pixels == NULL) {
    fclose(file);
    return 0;
  }
  if (fread(out->pixels, 1, pixel_bytes, file) != pixel_bytes) {
    fclose(file);
    ct_native_gray_image_buffer_reset(out);
    return 0;
  }

  out->descriptor.width = width;
  out->descriptor.height = height;
  out->descriptor.stride_bytes = (size_t)width;
  out->descriptor.data = out->pixels;
  fclose(file);
  return 1;
}

static ct_optional_u32_t ct_native_some_u32(uint32_t value) {
  ct_optional_u32_t out;
  out.has_value = CT_TRUE;
  out.value = value;
  return out;
}

static ct_optional_bool_t ct_native_none_bool(void) {
  ct_optional_bool_t out;
  out.has_value = CT_FALSE;
  out.value = CT_FALSE;
  return out;
}

static ct_optional_f32_t ct_native_none_f32(void) {
  ct_optional_f32_t out;
  out.has_value = CT_FALSE;
  out.value = 0.0f;
  return out;
}

static ct_refiner_config_t ct_native_default_refiner(void) {
  ct_refiner_config_t config;
  memset(&config, 0, sizeof(config));
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

static ct_orientation_clustering_params_t ct_native_default_orientation_clustering(void) {
  ct_orientation_clustering_params_t params;
  memset(&params, 0, sizeof(params));
  params.num_bins = 90;
  params.max_iters = 10;
  params.peak_min_separation_deg = 10.0f;
  params.outlier_threshold_deg = 30.0f;
  params.min_peak_weight_fraction = 0.05f;
  params.use_weights = CT_TRUE;
  return params;
}

static ct_chess_config_t ct_native_default_shared_chess_config(void) {
  ct_chess_config_t config;
  memset(&config, 0, sizeof(config));
  config.params.use_radius10 = CT_FALSE;
  config.params.descriptor_use_radius10 = ct_native_none_bool();
  config.params.threshold_rel = 0.2f;
  config.params.threshold_abs = ct_native_none_f32();
  config.params.nms_radius = 2;
  config.params.min_cluster_size = 2;
  config.params.refiner = ct_native_default_refiner();
  config.multiscale.pyramid.num_levels = 1;
  config.multiscale.pyramid.min_size = 128;
  config.multiscale.refinement_radius = 3;
  config.multiscale.merge_radius = 3.0f;
  return config;
}

static ct_chessboard_detector_config_t ct_native_default_chessboard_detector_config(void) {
  ct_chessboard_detector_config_t config;
  memset(&config, 0, sizeof(config));
  config.chess = ct_native_default_shared_chess_config();
  config.chessboard.min_corner_strength = 0.5f;
  config.chessboard.min_corners = 20;
  config.chessboard.expected_rows = ct_native_some_u32(7);
  config.chessboard.expected_cols = ct_native_some_u32(11);
  config.chessboard.completeness_threshold = 0.9f;
  config.chessboard.use_orientation_clustering = CT_TRUE;
  config.chessboard.orientation_clustering_params = ct_native_default_orientation_clustering();
  config.chessboard.graph.min_spacing_pix = 10.0f;
  config.chessboard.graph.max_spacing_pix = 120.0f;
  config.chessboard.graph.k_neighbors = 8;
  config.chessboard.graph.orientation_tolerance_deg = 22.5f;
  return config;
}

#endif
