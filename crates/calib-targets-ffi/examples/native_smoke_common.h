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
  config.saddle_point.radius = 2;
  config.saddle_point.det_margin = 1e-3f;
  config.saddle_point.max_offset = 1.5f;
  config.saddle_point.min_abs_det = 1e-4f;
  return config;
}

static ct_chess_config_t ct_native_default_shared_chess_config(void) {
  ct_chess_config_t config;
  memset(&config, 0, sizeof(config));
  config.params.use_radius10 = CT_FALSE;
  /* ABI 3.0.0: single absolute response floor (was threshold_rel +
     threshold_abs); 15.0 is the workspace production default. */
  config.params.threshold = 15.0f;
  config.params.nms_radius = 2;
  config.params.min_cluster_size = 2;
  config.params.refiner = ct_native_default_refiner();
  config.multiscale.pyramid.num_levels = 1;
  config.multiscale.pyramid.min_size = 128;
  config.multiscale.refinement_radius = 3;
  config.multiscale.merge_radius = 3.0f;
  config.upscale.mode = CT_UPSCALE_MODE_DISABLED;
  config.upscale.factor = 2;
  /* ABI 6.0.0: opt into the position covariance (zero-initialised: off),
     with the pixel noise estimated per image (noise_sigma unset). */
  config.noise_sigma.has_value = CT_FALSE;
  config.uncertainty = CT_UNCERTAINTY_PROPAGATED;
  return config;
}

static ct_chessboard_detector_config_t ct_native_default_chessboard_detector_config(void) {
  ct_chessboard_detector_config_t config;
  memset(&config, 0, sizeof(config));
  config.chess = ct_native_default_shared_chess_config();
  // C ABI: the chessboard detector's 30-field `ct_chessboard_params_t`
  // mirrors `DetectorParams`. The `init_default` helper populates a
  // valid default-configured value; callers override individual fields
  // as needed (here: override `min_corner_strength`; the library default
  // is 33.0, which trims weak ChESS responses on defocused edges).
  ct_chessboard_params_init_default(&config.chessboard);
  config.chessboard.min_corner_strength = 0.5f;
  return config;
}

/* Number of corners whose `position_covariance` is set and positive
 * definite. Any set covariance that is not positive definite counts as a
 * failure: the function returns `(size_t)-1`. */
static size_t ct_native_count_valid_covariances(const ct_chessboard_corner_t *corners,
                                                size_t corners_len) {
  size_t valid = 0;
  size_t i = 0;
  for (i = 0; i < corners_len; ++i) {
    const ct_position_covariance_t *c = &corners[i].position_covariance;
    if (corners[i].has_position_covariance != CT_TRUE) {
      continue;
    }
    if (!(c->xx > 0.0f && c->yy > 0.0f && c->xx * c->yy - c->xy * c->xy > 0.0f)) {
      return (size_t)-1;
    }
    ++valid;
  }
  return valid;
}

/* Whether a chessboard result's `quality` is internally consistent and
 * populated, as it is for a lossless frame detected with the covariance on:
 * noise known, covariances on (nearly) every corner, a local check over at
 * least one corner. */
static int ct_native_quality_is_sane(const ct_detection_quality_t *q, size_t corners_len) {
  if (q->noise_sigma.has_value != CT_TRUE || !(q->noise_sigma.value >= 0.0f)) {
    return 0;
  }
  if (!(q->covariance_coverage >= 0.9f && q->covariance_coverage <= 1.0f)) {
    return 0;
  }
  if (q->median_sigma_px.has_value != CT_TRUE || q->p95_sigma_px.has_value != CT_TRUE ||
      !(q->median_sigma_px.value > 0.0f) || q->p95_sigma_px.value < q->median_sigma_px.value) {
    return 0;
  }
  if (q->local_residual_count == 0 || q->local_residual_count > corners_len ||
      q->local_residual_rms_px.has_value != CT_TRUE || !(q->local_residual_rms_px.value > 0.0f)) {
    return 0;
  }
  if (q->variance_factor_count == 0 || q->variance_factor_count > q->local_residual_count ||
      q->variance_factor.has_value != CT_TRUE || !(q->variance_factor.value > 0.0f)) {
    return 0;
  }
  return 1;
}

#endif
