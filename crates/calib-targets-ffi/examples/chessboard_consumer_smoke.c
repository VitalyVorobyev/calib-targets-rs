#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "native_smoke_common.h"

static int copy_last_error(char *out, size_t out_capacity) {
  size_t len = 0;
  ct_status_t status = ct_last_error_message(NULL, 0, &len);
  if (status != CT_STATUS_OK || out == NULL || out_capacity == 0 || len + 1 > out_capacity) {
    return 0;
  }
  status = ct_last_error_message(out, out_capacity, &len);
  return status == CT_STATUS_OK;
}

int main(int argc, char **argv) {
  ct_chessboard_detector_t *detector = NULL;
  ct_native_gray_image_buffer_t image;
  ct_chessboard_detector_config_t config;
  ct_chessboard_result_t result;
  ct_labeled_corner_t *short_corners = NULL;
  ct_labeled_corner_t *corners = NULL;
  char error_message[256];
  size_t corners_len = 0;
  ct_status_t status = CT_STATUS_INTERNAL_ERROR;
  const char *version = NULL;
  int exit_code = 1;

  memset(&image, 0, sizeof(image));
  memset(&result, 0, sizeof(result));

  if (argc != 2) {
    fprintf(stderr, "usage: %s <mid.pgm>\n", argv[0]);
    return 2;
  }

  version = ct_version_string();
  if (version == NULL || version[0] == '\0') {
    fprintf(stderr, "ct_version_string returned an empty value\n");
    return 1;
  }

  status = ct_chessboard_detector_create(NULL, &detector);
  if (status != CT_STATUS_INVALID_ARGUMENT) {
    fprintf(stderr, "expected invalid-argument status for null config, got %u\n", (unsigned)status);
    goto cleanup;
  }
  if (!copy_last_error(error_message, sizeof(error_message)) || strstr(error_message, "config") == NULL) {
    fprintf(stderr, "failed to retrieve last error after invalid create\n");
    goto cleanup;
  }

  config = ct_native_default_chessboard_detector_config();
  status = ct_chessboard_detector_create(&config, &detector);
  if (status != CT_STATUS_OK || detector == NULL) {
    fprintf(stderr, "failed to create chessboard detector: %u\n", (unsigned)status);
    goto cleanup;
  }

  if (!ct_native_load_binary_pgm(argv[1], &image)) {
    fprintf(stderr, "failed to load PGM image %s\n", argv[1]);
    goto cleanup;
  }

  status = ct_chessboard_detector_detect(detector, &image.descriptor, &result, NULL, 0, &corners_len);
  if (status != CT_STATUS_OK) {
    fprintf(stderr, "query detect failed: %u\n", (unsigned)status);
    goto cleanup;
  }
  if (corners_len != 77 || result.detection.kind != CT_TARGET_KIND_CHESSBOARD) {
    fprintf(stderr, "unexpected query result: kind=%u corners=%zu\n",
            (unsigned)result.detection.kind,
            corners_len);
    goto cleanup;
  }

  short_corners = (ct_labeled_corner_t *)calloc(corners_len - 1, sizeof(*short_corners));
  if (short_corners == NULL) {
    fprintf(stderr, "failed to allocate short corner buffer\n");
    goto cleanup;
  }

  status = ct_chessboard_detector_detect(
      detector,
      &image.descriptor,
      &result,
      short_corners,
      corners_len - 1,
      &corners_len);
  if (status != CT_STATUS_BUFFER_TOO_SMALL) {
    fprintf(stderr, "expected short-buffer status, got %u\n", (unsigned)status);
    goto cleanup;
  }
  if (!copy_last_error(error_message, sizeof(error_message)) || strstr(error_message, "out_corners") == NULL) {
    fprintf(stderr, "failed to retrieve short-buffer error message\n");
    goto cleanup;
  }

  free(short_corners);
  short_corners = NULL;

  corners = (ct_labeled_corner_t *)calloc(corners_len, sizeof(*corners));
  if (corners == NULL) {
    fprintf(stderr, "failed to allocate corner buffer\n");
    goto cleanup;
  }

  status = ct_chessboard_detector_detect(
      detector,
      &image.descriptor,
      &result,
      corners,
      corners_len,
      &corners_len);
  if (status != CT_STATUS_OK) {
    fprintf(stderr, "fill detect failed: %u\n", (unsigned)status);
    goto cleanup;
  }
  if (corners_len != 77 || result.detection.corners_len != 77 || corners[0].has_grid != CT_TRUE) {
    fprintf(stderr, "unexpected filled result: reported=%zu first.has_grid=%u\n",
            corners_len,
            (unsigned)corners[0].has_grid);
    goto cleanup;
  }

  exit_code = 0;

cleanup:
  free(corners);
  free(short_corners);
  ct_native_gray_image_buffer_reset(&image);
  ct_chessboard_detector_destroy(detector);
  return exit_code;
}
