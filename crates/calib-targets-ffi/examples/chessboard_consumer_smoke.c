#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "native_smoke_common.h"

/* Expected labelled-corner count for testdata/mid.pgm under the default
 * chessboard detector config (min_corner_strength overridden to 0.5). */
#define CT_SMOKE_EXPECTED_CORNERS 77

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
  ct_chessboard_detect_args_t args;
  ct_chessboard_corner_t *short_corners = NULL;
  ct_chessboard_corner_t *corners = NULL;
  char error_message[256];
  size_t corners_len = 0;
  ct_status_t status = CT_STATUS_INTERNAL_ERROR;
  const char *version = NULL;
  int exit_code = 1;

  memset(&image, 0, sizeof(image));
  memset(&result, 0, sizeof(result));
  memset(&args, 0, sizeof(args));

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

  /* The C ABI groups the detector handle and image into a `_detect_args_t`
   * struct and the output buffers into a `_detect_buffers_t` struct. A NULL
   * corner buffer with `0` capacity queries the required length. */
  args.detector = detector;
  args.image = &image.descriptor;

  {
    ct_chessboard_detect_buffers_t bufs;
    memset(&bufs, 0, sizeof(bufs));
    bufs.out_result = &result;
    bufs.out_corners = NULL;
    bufs.corners_capacity = 0;
    bufs.out_corners_len = &corners_len;

    status = ct_chessboard_detector_detect(&args, &bufs);
  }
  if (status != CT_STATUS_OK) {
    fprintf(stderr, "query detect failed: %u\n", (unsigned)status);
    goto cleanup;
  }
  if (corners_len != CT_SMOKE_EXPECTED_CORNERS || result.corners_len != CT_SMOKE_EXPECTED_CORNERS) {
    fprintf(stderr, "unexpected query result: result.corners_len=%zu corners=%zu\n",
            result.corners_len,
            corners_len);
    goto cleanup;
  }

  /* A buffer one entry short must fail with BUFFER_TOO_SMALL while still
   * writing the required length back into `out_corners_len`. */
  short_corners = (ct_chessboard_corner_t *)calloc(corners_len - 1, sizeof(*short_corners));
  if (short_corners == NULL) {
    fprintf(stderr, "failed to allocate short corner buffer\n");
    goto cleanup;
  }

  {
    ct_chessboard_detect_buffers_t bufs;
    memset(&bufs, 0, sizeof(bufs));
    bufs.out_result = &result;
    bufs.out_corners = short_corners;
    bufs.corners_capacity = corners_len - 1;
    bufs.out_corners_len = &corners_len;

    status = ct_chessboard_detector_detect(&args, &bufs);
  }
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

  corners = (ct_chessboard_corner_t *)calloc(corners_len, sizeof(*corners));
  if (corners == NULL) {
    fprintf(stderr, "failed to allocate corner buffer\n");
    goto cleanup;
  }

  {
    ct_chessboard_detect_buffers_t bufs;
    memset(&bufs, 0, sizeof(bufs));
    bufs.out_result = &result;
    bufs.out_corners = corners;
    bufs.corners_capacity = corners_len;
    bufs.out_corners_len = &corners_len;

    status = ct_chessboard_detector_detect(&args, &bufs);
  }
  if (status != CT_STATUS_OK) {
    fprintf(stderr, "fill detect failed: %u\n", (unsigned)status);
    goto cleanup;
  }
  if (corners_len != CT_SMOKE_EXPECTED_CORNERS || result.corners_len != CT_SMOKE_EXPECTED_CORNERS) {
    fprintf(stderr, "unexpected filled result: reported=%zu result.corners_len=%zu\n",
            corners_len,
            result.corners_len);
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
