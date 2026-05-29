//! Unit and integration tests for the C ABI surface.
//!
//! Split out of `lib.rs` to keep that file focused on the ABI helpers and
//! exported scalar entry points. `use super::*` resolves to the crate root,
//! exactly as the inline `mod tests` did before the move.

use super::*;
// Detector functions moved to `detectors` submodule; import them explicitly for tests.
use crate::detectors::{
    ct_charuco_detect_args_t, ct_charuco_detect_buffers_t, ct_charuco_detector_create,
    ct_charuco_detector_destroy, ct_charuco_detector_detect,
    ct_charuco_detector_detect_diagnostics_json, ct_chessboard_detect_args_t,
    ct_chessboard_detect_buffers_t, ct_chessboard_detector_create, ct_chessboard_detector_destroy,
    ct_chessboard_detector_detect, ct_chessboard_detector_detect_diagnostics_json,
    ct_marker_board_detect_args_t, ct_marker_board_detect_buffers_t,
    ct_marker_board_detector_create, ct_marker_board_detector_destroy,
    ct_marker_board_detector_detect, ct_marker_board_detector_detect_diagnostics_json,
    ct_puzzleboard_detect_args_t, ct_puzzleboard_detect_buffers_t, ct_puzzleboard_detector_create,
    ct_puzzleboard_detector_destroy, ct_puzzleboard_detector_detect,
    ct_puzzleboard_detector_detect_diagnostics_json,
};
use crate::error::ffi_status;
use image::ImageReader;
use std::ffi::CStr;
use std::path::{Path, PathBuf};
use std::ptr;

fn testdata_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata")
        .join(name)
}

fn load_gray(name: &str) -> image::GrayImage {
    ImageReader::open(testdata_path(name))
        .expect("open image")
        .decode()
        .expect("decode image")
        .to_luma8()
}

fn image_descriptor(image: &image::GrayImage) -> ct_gray_image_u8_t {
    ct_gray_image_u8_t {
        width: image.width(),
        height: image.height(),
        stride_bytes: image.width() as usize,
        data: image.as_raw().as_ptr(),
    }
}

fn last_error_string() -> String {
    let mut len = 0usize;
    let status = unsafe { ct_last_error_message(ptr::null_mut(), 0, &mut len) };
    assert_eq!(status, ct_status_t::CT_STATUS_OK);
    let mut buf = vec![0_i8; len + 1];
    let status = unsafe { ct_last_error_message(buf.as_mut_ptr(), buf.len(), &mut len) };
    assert_eq!(status, ct_status_t::CT_STATUS_OK);
    unsafe { CStr::from_ptr(buf.as_ptr()) }
        .to_str()
        .unwrap()
        .to_string()
}

fn default_refiner() -> ct_refiner_config_t {
    ct_refiner_config_t {
        kind: CT_REFINER_KIND_CENTER_OF_MASS,
        center_of_mass: ct_center_of_mass_config_t { radius: 2 },
        forstner: ct_forstner_config_t {
            radius: 2,
            min_trace: 25.0,
            min_det: 1e-3,
            max_condition_number: 50.0,
            max_offset: 1.5,
        },
        saddle_point: ct_saddle_point_config_t {
            radius: 2,
            det_margin: 1e-3,
            max_offset: 1.5,
            min_abs_det: 1e-4,
        },
    }
}

fn default_saddle_refiner() -> ct_refiner_config_t {
    ct_refiner_config_t {
        kind: CT_REFINER_KIND_SADDLE_POINT,
        center_of_mass: ct_center_of_mass_config_t { radius: 2 },
        forstner: ct_forstner_config_t {
            radius: 2,
            min_trace: 25.0,
            min_det: 1e-3,
            max_condition_number: 50.0,
            max_offset: 1.5,
        },
        saddle_point: ct_saddle_point_config_t {
            radius: 2,
            det_margin: 1e-3,
            max_offset: 1.5,
            min_abs_det: 1e-4,
        },
    }
}

fn default_shared_chess_config() -> ct_chess_config_t {
    ct_chess_config_t {
        params: ct_chess_params_t {
            use_radius10: CT_FALSE,
            descriptor_use_radius10: ct_optional_bool_t::none(),
            threshold_rel: 0.2,
            threshold_abs: ct_optional_f32_t::none(),
            nms_radius: 2,
            min_cluster_size: 2,
            refiner: default_refiner(),
        },
        multiscale: ct_coarse_to_fine_params_t {
            pyramid: ct_pyramid_params_t {
                num_levels: 1,
                min_size: 128,
            },
            refinement_radius: 3,
            merge_radius: 3.0,
        },
        upscale: ct_upscale_config_t {
            mode: CT_UPSCALE_MODE_DISABLED,
            factor: 2,
        },
    }
}

fn chessboard_params_with_strength(strength: f32) -> ct_chessboard_params_t {
    let mut p = chessboard_params_default_values();
    p.min_corner_strength = strength;
    p
}

fn chessboard_config_mid_png() -> ct_chessboard_detector_config_t {
    ct_chessboard_detector_config_t {
        chess: default_shared_chess_config(),
        chessboard: chessboard_params_with_strength(0.5),
    }
}

fn charuco_config_small_png() -> ct_charuco_detector_config_t {
    ct_charuco_detector_config_t {
        chess: default_shared_chess_config(),
        detector: ct_charuco_detector_params_t {
            px_per_square: 60.0,
            chessboard: chessboard_params_default_values(),
            charuco: ct_charuco_board_spec_t {
                rows: 22,
                cols: 22,
                cell_size: 5.2,
                marker_size_rel: 0.75,
                dictionary: CT_DICTIONARY_DICT_4X4_250,
                marker_layout: CT_MARKER_LAYOUT_OPENCV_CHARUCO,
            },
            scan: ct_scan_decode_config_t {
                border_bits: 1,
                inset_frac: 0.06,
                marker_size_rel: 0.75,
                min_border_score: 0.85,
                dedup_by_id: CT_TRUE,
                multi_threshold: CT_TRUE,
            },
            max_hamming: 2,
            min_marker_inliers: 12,
            grid_smoothness_threshold_rel: 0.05,
            corner_validation_threshold_rel: 0.08,
            corner_redetect_params: ct_chess_params_t {
                use_radius10: CT_FALSE,
                descriptor_use_radius10: ct_optional_bool_t::none(),
                threshold_rel: 0.05,
                threshold_abs: ct_optional_f32_t::none(),
                nms_radius: 2,
                min_cluster_size: 1,
                refiner: default_saddle_refiner(),
            },
        },
    }
}

fn marker_board_config_crop_png() -> ct_marker_board_detector_config_t {
    ct_marker_board_detector_config_t {
        chess: default_shared_chess_config(),
        detector: ct_marker_board_params_t {
            layout: ct_marker_board_layout_t {
                rows: 22,
                cols: 22,
                cell_size: ct_optional_f32_t::none(),
                circles: [
                    ct_marker_circle_spec_t {
                        cell: ct_grid_coords_t { i: 11, j: 11 },
                        polarity: CT_CIRCLE_POLARITY_WHITE,
                    },
                    ct_marker_circle_spec_t {
                        cell: ct_grid_coords_t { i: 12, j: 11 },
                        polarity: CT_CIRCLE_POLARITY_BLACK,
                    },
                    ct_marker_circle_spec_t {
                        cell: ct_grid_coords_t { i: 12, j: 12 },
                        polarity: CT_CIRCLE_POLARITY_WHITE,
                    },
                ],
            },
            chessboard: chessboard_params_with_strength(0.2),
            circle_score: ct_circle_score_params_t {
                patch_size: 64,
                diameter_frac: 0.5,
                ring_thickness_frac: 0.35,
                ring_radius_mul: 1.6,
                min_contrast: 60.0,
                samples: 48,
                center_search_px: 2,
            },
            match_params: ct_circle_match_params_t {
                max_candidates_per_polarity: 3,
                max_distance_cells: ct_optional_f32_t::none(),
                min_offset_inliers: 1,
            },
            has_roi_cells: CT_FALSE,
            roi_cells: [0; 4],
        },
    }
}

fn puzzleboard_config_small_png() -> ct_puzzleboard_detector_config_t {
    let mut chess = default_shared_chess_config();
    chess.params.threshold_rel = 0.15;
    chess.params.nms_radius = 3;
    ct_puzzleboard_detector_config_t {
        chess,
        detector: ct_puzzleboard_params_t {
            px_per_square: 60.0,
            chessboard: chessboard_params_with_strength(0.1),
            board: ct_puzzleboard_spec_t {
                rows: 10,
                cols: 10,
                cell_size: 12.0,
                origin_row: 0,
                origin_col: 0,
            },
            decode: ct_puzzleboard_decode_config_t {
                min_window: 4,
                min_bit_confidence: 0.15,
                max_bit_error_rate: 0.3,
                search_all_components: CT_TRUE,
                sample_radius_rel: 1.0 / 6.0,
                search_mode: CT_PUZZLEBOARD_SEARCH_MODE_FULL,
                scoring_mode: CT_PUZZLEBOARD_SCORING_MODE_SOFT_LOG_LIKELIHOOD,
                bit_likelihood_slope: 12.0,
                per_bit_floor: -6.0,
                alignment_min_margin: 0.02,
            },
            corner_redetect_params: ct_chess_params_t {
                use_radius10: CT_FALSE,
                descriptor_use_radius10: ct_optional_bool_t::none(),
                threshold_rel: 0.05,
                threshold_abs: ct_optional_f32_t::none(),
                nms_radius: 2,
                min_cluster_size: 1,
                refiner: default_saddle_refiner(),
            },
        },
    }
}

#[test]
fn shared_chess_config_converts_upscale_settings() {
    let mut config = default_shared_chess_config();
    config.upscale = ct_upscale_config_t {
        mode: CT_UPSCALE_MODE_FIXED,
        factor: 2,
    };

    let converted = convert::convert_chess_config(&config).unwrap();
    assert_eq!(converted.upscale, chess_corners::UpscaleConfig::fixed(2));
    assert_eq!(converted.upscale.effective_factor(), 2);
}

#[test]
fn shared_chess_config_rejects_invalid_upscale_factor() {
    let mut config = default_shared_chess_config();
    config.upscale = ct_upscale_config_t {
        mode: CT_UPSCALE_MODE_FIXED,
        factor: 1,
    };

    assert!(convert::convert_chess_config(&config).is_err());
}

#[test]
fn version_string_is_static_c_string() {
    let ptr = ct_version_string();
    assert!(!ptr.is_null());
    let version = unsafe { CStr::from_ptr(ptr) };
    assert_eq!(version.to_str().unwrap(), env!("CARGO_PKG_VERSION"));
}

#[test]
fn last_error_message_supports_query_then_copy() {
    set_last_error_message("ffi scaffold error");

    let mut len = usize::MAX;
    let status = unsafe { ct_last_error_message(ptr::null_mut(), 0, &mut len) };
    assert_eq!(status, ct_status_t::CT_STATUS_OK);
    assert_eq!(len, "ffi scaffold error".len());

    let mut short = vec![0_i8; len];
    let status = unsafe { ct_last_error_message(short.as_mut_ptr(), short.len(), &mut len) };
    assert_eq!(status, ct_status_t::CT_STATUS_BUFFER_TOO_SMALL);

    let mut exact = vec![0_i8; len + 1];
    let status = unsafe { ct_last_error_message(exact.as_mut_ptr(), exact.len(), &mut len) };
    assert_eq!(status, ct_status_t::CT_STATUS_OK);
    let copied = unsafe { CStr::from_ptr(exact.as_ptr()) };
    assert_eq!(copied.to_str().unwrap(), "ffi scaffold error");
}

#[test]
fn ffi_boundary_catches_panics_and_updates_last_error() {
    let status = ffi_status(|| -> FfiResult<()> { panic!("boom") });
    assert_eq!(status, ct_status_t::CT_STATUS_INTERNAL_ERROR);
    let last_error = last_error_bytes();
    let last_error = CStr::from_bytes_with_nul(&last_error).unwrap();
    assert!(last_error
        .to_str()
        .unwrap()
        .contains("panic across FFI boundary"));
}

#[test]
fn gray_image_validation_rejects_invalid_inputs() {
    let null_data = ct_gray_image_u8_t {
        width: 8,
        height: 8,
        stride_bytes: 8,
        data: ptr::null(),
    };
    assert_eq!(
        null_data.validate(),
        Err(ct_status_t::CT_STATUS_INVALID_ARGUMENT)
    );

    let bad_stride = ct_gray_image_u8_t {
        width: 8,
        height: 8,
        stride_bytes: 7,
        data: VERSION_CSTR.as_ptr(),
    };
    assert_eq!(
        bad_stride.validate(),
        Err(ct_status_t::CT_STATUS_INVALID_ARGUMENT)
    );
}

#[test]
fn chessboard_detect_supports_query_and_copy() {
    let config = chessboard_config_mid_png();
    let mut detector = ptr::null_mut();
    let status = unsafe { ct_chessboard_detector_create(&config, &mut detector) };
    assert_eq!(status, ct_status_t::CT_STATUS_OK);
    assert!(!detector.is_null());

    let image = load_gray("mid.png");
    let descriptor = image_descriptor(&image);
    let mut result = ct_chessboard_result_t::default();
    let mut corners_len = 0usize;
    let args = ct_chessboard_detect_args_t {
        detector,
        image: &descriptor,
    };
    let mut bufs = ct_chessboard_detect_buffers_t {
        out_result: &mut result,
        out_corners: ptr::null_mut(),
        corners_capacity: 0,
        out_corners_len: &mut corners_len,
    };
    let status = unsafe { ct_chessboard_detector_detect(&args, &mut bufs) };
    assert_eq!(status, ct_status_t::CT_STATUS_OK);
    assert_eq!(result.corners_len, 77);
    assert_eq!(corners_len, 77);
    // A real detection records the seed-derived grid pitch.
    assert_eq!(result.cell_size.has_value, CT_TRUE);
    assert!(
        result.cell_size.value > 0.0,
        "cell_size {} should be a positive pitch",
        result.cell_size.value
    );

    let mut short = vec![ct_chessboard_corner_t::default(); corners_len - 1];
    bufs.out_corners = short.as_mut_ptr();
    bufs.corners_capacity = short.len();
    let status = unsafe { ct_chessboard_detector_detect(&args, &mut bufs) };
    assert_eq!(status, ct_status_t::CT_STATUS_BUFFER_TOO_SMALL);

    let mut corners = vec![ct_chessboard_corner_t::default(); corners_len];
    bufs.out_corners = corners.as_mut_ptr();
    bufs.corners_capacity = corners.len();
    let status = unsafe { ct_chessboard_detector_detect(&args, &mut bufs) };
    assert_eq!(status, ct_status_t::CT_STATUS_OK);
    assert_eq!(corners_len, corners.len());
    // Every corner carries a distinct input-slice provenance index.
    let mut input_indices: Vec<usize> = corners.iter().map(|corner| corner.input_index).collect();
    input_indices.sort_unstable();
    input_indices.dedup();
    assert_eq!(input_indices.len(), corners.len());

    unsafe { ct_chessboard_detector_destroy(detector) };
}

#[test]
fn charuco_create_rejects_invalid_dictionary_id() {
    let mut config = charuco_config_small_png();
    config.detector.charuco.dictionary = 999;
    let mut detector = ptr::null_mut();
    let status = unsafe { ct_charuco_detector_create(&config, &mut detector) };
    assert_eq!(status, ct_status_t::CT_STATUS_CONFIG_ERROR);
    assert!(detector.is_null());
    assert!(last_error_string().contains("charuco.dictionary"));
}

#[test]
fn puzzleboard_create_rejects_invalid_board_size() {
    let mut config = puzzleboard_config_small_png();
    config.detector.board.rows = 3;
    let mut detector = ptr::null_mut();
    let status = unsafe { ct_puzzleboard_detector_create(&config, &mut detector) };
    assert_eq!(status, ct_status_t::CT_STATUS_CONFIG_ERROR);
    assert!(detector.is_null());
    assert!(last_error_string().contains("PuzzleBoard"));
}

#[test]
fn scan_decode_config_preserves_multi_threshold_flag() {
    let disabled = convert::convert_scan_decode_config(&ct_scan_decode_config_t {
        border_bits: 1,
        inset_frac: 0.06,
        marker_size_rel: 0.75,
        min_border_score: 0.85,
        dedup_by_id: CT_TRUE,
        multi_threshold: CT_FALSE,
    })
    .unwrap();
    assert!(!disabled.multi_threshold);

    let enabled = convert::convert_scan_decode_config(&ct_scan_decode_config_t {
        border_bits: 1,
        inset_frac: 0.06,
        marker_size_rel: 0.75,
        min_border_score: 0.85,
        dedup_by_id: CT_TRUE,
        multi_threshold: CT_TRUE,
    })
    .unwrap();
    assert!(enabled.multi_threshold);
}

#[test]
fn charuco_detect_supports_query_and_copy() {
    let config = charuco_config_small_png();
    let mut detector = ptr::null_mut();
    let status = unsafe { ct_charuco_detector_create(&config, &mut detector) };
    assert_eq!(status, ct_status_t::CT_STATUS_OK);
    assert!(!detector.is_null());

    let image = load_gray("small.png");
    let descriptor = image_descriptor(&image);
    let mut result = ct_charuco_result_t::default();
    let mut corners_len = 0usize;
    let mut markers_len = 0usize;
    let args = ct_charuco_detect_args_t {
        detector,
        image: &descriptor,
    };
    let mut bufs = ct_charuco_detect_buffers_t {
        out_result: &mut result,
        out_corners: ptr::null_mut(),
        corners_capacity: 0,
        out_corners_len: &mut corners_len,
        out_markers: ptr::null_mut(),
        markers_capacity: 0,
        out_markers_len: &mut markers_len,
    };
    let status = unsafe { ct_charuco_detector_detect(&args, &mut bufs) };
    assert_eq!(status, ct_status_t::CT_STATUS_OK);
    assert_eq!(result.detection.kind, CT_TARGET_KIND_CHARUCO);
    assert!(corners_len >= 60);
    assert!(markers_len >= 20);

    let mut corners = vec![ct_labeled_corner_t::default(); corners_len];
    let mut short_markers = vec![ct_marker_detection_t::default(); markers_len - 1];
    bufs.out_corners = corners.as_mut_ptr();
    bufs.corners_capacity = corners.len();
    bufs.out_markers = short_markers.as_mut_ptr();
    bufs.markers_capacity = short_markers.len();
    let status = unsafe { ct_charuco_detector_detect(&args, &mut bufs) };
    assert_eq!(status, ct_status_t::CT_STATUS_BUFFER_TOO_SMALL);

    let mut markers = vec![ct_marker_detection_t::default(); markers_len];
    bufs.out_markers = markers.as_mut_ptr();
    bufs.markers_capacity = markers.len();
    let status = unsafe { ct_charuco_detector_detect(&args, &mut bufs) };
    assert_eq!(status, ct_status_t::CT_STATUS_OK);
    assert!(corners.iter().all(|corner| {
        corner.has_grid == CT_TRUE
            && corner.id.has_value == CT_TRUE
            && corner.has_target_position == CT_TRUE
    }));
    assert!(markers
        .iter()
        .all(|marker| marker.has_corners_img == CT_TRUE));

    unsafe { ct_charuco_detector_destroy(detector) };
}

#[test]
fn marker_board_detect_supports_query_and_copy() {
    let config = marker_board_config_crop_png();
    let mut detector = ptr::null_mut();
    let status = unsafe { ct_marker_board_detector_create(&config, &mut detector) };
    assert_eq!(status, ct_status_t::CT_STATUS_OK);
    assert!(!detector.is_null());

    let image = load_gray("markerboard_crop.png");
    let descriptor = image_descriptor(&image);
    let mut result = ct_marker_board_result_t::default();
    let mut corners_len = 0usize;
    let args = ct_marker_board_detect_args_t {
        detector,
        image: &descriptor,
    };
    let mut bufs = ct_marker_board_detect_buffers_t {
        out_result: &mut result,
        out_corners: ptr::null_mut(),
        corners_capacity: 0,
        out_corners_len: &mut corners_len,
    };
    let status = unsafe { ct_marker_board_detector_detect(&args, &mut bufs) };
    assert_eq!(status, ct_status_t::CT_STATUS_OK);
    assert_eq!(result.detection.kind, CT_TARGET_KIND_CHECKERBOARD_MARKER);
    assert!(corners_len > 0);
    // Circle evidence (scored candidates, expected-to-detected matches,
    // alignment-inlier count) moved to the Rust `MarkerBoardDiagnostics`
    // channel and is intentionally not surfaced over the C ABI.

    let mut corners = vec![ct_labeled_corner_t::default(); corners_len];
    bufs.out_corners = corners.as_mut_ptr();
    bufs.corners_capacity = corners.len();
    let status = unsafe { ct_marker_board_detector_detect(&args, &mut bufs) };
    assert_eq!(status, ct_status_t::CT_STATUS_OK);

    unsafe { ct_marker_board_detector_destroy(detector) };
}

#[test]
fn puzzleboard_detect_supports_query_and_copy() {
    let config = puzzleboard_config_small_png();
    let mut detector = ptr::null_mut();
    let status = unsafe { ct_puzzleboard_detector_create(&config, &mut detector) };
    assert_eq!(status, ct_status_t::CT_STATUS_OK);
    assert!(!detector.is_null());

    let image = load_gray("puzzleboard_small.png");
    let descriptor = image_descriptor(&image);
    let mut result = ct_puzzleboard_result_t::default();
    let mut corners_len = 0usize;
    let args = ct_puzzleboard_detect_args_t {
        detector,
        image: &descriptor,
    };
    let mut bufs = ct_puzzleboard_detect_buffers_t {
        out_result: &mut result,
        out_corners: ptr::null_mut(),
        corners_capacity: 0,
        out_corners_len: &mut corners_len,
    };
    let status = unsafe { ct_puzzleboard_detector_detect(&args, &mut bufs) };
    assert_eq!(status, ct_status_t::CT_STATUS_OK);
    assert_eq!(result.detection.kind, CT_TARGET_KIND_PUZZLEBOARD);
    assert!(corners_len > 0);
    assert!(result.edges_observed > 0);
    assert!(result.mean_bit_confidence > 0.0);
    // Decode-internal evidence (score_best / score_margin / scoring_mode /
    // observed-edge count) moved to the Rust `PuzzleBoardDiagnostics`
    // channel and is intentionally not surfaced over the C ABI.

    let mut short = vec![ct_labeled_corner_t::default(); corners_len - 1];
    bufs.out_corners = short.as_mut_ptr();
    bufs.corners_capacity = short.len();
    let status = unsafe { ct_puzzleboard_detector_detect(&args, &mut bufs) };
    assert_eq!(status, ct_status_t::CT_STATUS_BUFFER_TOO_SMALL);

    let mut corners = vec![ct_labeled_corner_t::default(); corners_len];
    bufs.out_corners = corners.as_mut_ptr();
    bufs.corners_capacity = corners.len();
    let status = unsafe { ct_puzzleboard_detector_detect(&args, &mut bufs) };
    assert_eq!(status, ct_status_t::CT_STATUS_OK);
    assert!(corners.iter().all(|corner| {
        corner.has_grid == CT_TRUE
            && corner.id.has_value == CT_TRUE
            && corner.has_target_position == CT_TRUE
    }));

    unsafe { ct_puzzleboard_detector_destroy(detector) };
}

#[test]
fn puzzleboard_decode_config_converts_new_modes_and_soft_fields() {
    let converted = convert::convert_puzzleboard_decode_config(&ct_puzzleboard_decode_config_t {
        min_window: 4,
        min_bit_confidence: 0.15,
        max_bit_error_rate: 0.30,
        search_all_components: CT_TRUE,
        sample_radius_rel: 1.0 / 6.0,
        search_mode: CT_PUZZLEBOARD_SEARCH_MODE_FIXED_BOARD,
        scoring_mode: CT_PUZZLEBOARD_SCORING_MODE_HARD_WEIGHTED,
        bit_likelihood_slope: 9.0,
        per_bit_floor: -4.5,
        alignment_min_margin: 0.0,
    })
    .expect("convert puzzleboard decode config");

    assert_eq!(
        converted.search_mode,
        calib_targets::puzzleboard::PuzzleBoardSearchMode::FixedBoard
    );
    assert_eq!(
        converted.scoring_mode,
        calib_targets::puzzleboard::PuzzleBoardScoringMode::HardWeighted
    );
    assert_eq!(converted.bit_likelihood_slope, 9.0);
    assert_eq!(converted.per_bit_floor, -4.5);
    assert_eq!(converted.alignment_min_margin, 0.0);
}

#[test]
fn puzzleboard_decode_config_defaults_omitted_soft_fields_for_legacy_callers() {
    let converted = convert::convert_puzzleboard_decode_config(&ct_puzzleboard_decode_config_t {
        min_window: 4,
        min_bit_confidence: 0.15,
        max_bit_error_rate: 0.30,
        search_all_components: CT_TRUE,
        sample_radius_rel: 1.0 / 6.0,
        search_mode: 0,
        scoring_mode: 0,
        bit_likelihood_slope: 0.0,
        per_bit_floor: 0.0,
        alignment_min_margin: 0.0,
    })
    .expect("convert zeroed legacy soft fields");

    assert_eq!(
        converted.search_mode,
        calib_targets::puzzleboard::PuzzleBoardSearchMode::Full
    );
    assert_eq!(
        converted.scoring_mode,
        calib_targets::puzzleboard::PuzzleBoardScoringMode::SoftLogLikelihood
    );
    assert_eq!(converted.bit_likelihood_slope, 12.0);
    assert_eq!(converted.per_bit_floor, -6.0);
    assert_eq!(converted.alignment_min_margin, 0.02);
}

#[test]
fn puzzleboard_decode_config_allows_zero_slope_in_hard_mode() {
    let converted = convert::convert_puzzleboard_decode_config(&ct_puzzleboard_decode_config_t {
        min_window: 4,
        min_bit_confidence: 0.15,
        max_bit_error_rate: 0.30,
        search_all_components: CT_TRUE,
        sample_radius_rel: 1.0 / 6.0,
        search_mode: CT_PUZZLEBOARD_SEARCH_MODE_FIXED_BOARD,
        scoring_mode: CT_PUZZLEBOARD_SCORING_MODE_HARD_WEIGHTED,
        bit_likelihood_slope: 0.0,
        per_bit_floor: 0.0,
        alignment_min_margin: 0.0,
    })
    .expect("convert hard mode with zeroed soft slope");

    assert_eq!(
        converted.scoring_mode,
        calib_targets::puzzleboard::PuzzleBoardScoringMode::HardWeighted
    );
    assert_eq!(converted.bit_likelihood_slope, 12.0);
    assert_eq!(converted.per_bit_floor, 0.0);
    assert_eq!(converted.alignment_min_margin, 0.0);
}

#[test]
fn detectors_report_not_found_on_blank_image() {
    let blank = image::GrayImage::from_vec(32, 32, vec![0; 32 * 32]).unwrap();
    let descriptor = image_descriptor(&blank);

    let chess_config = chessboard_config_mid_png();
    let mut chess_detector = ptr::null_mut();
    let status = unsafe { ct_chessboard_detector_create(&chess_config, &mut chess_detector) };
    assert_eq!(status, ct_status_t::CT_STATUS_OK);
    let mut chess_len = usize::MAX;
    let chess_args = ct_chessboard_detect_args_t {
        detector: chess_detector,
        image: &descriptor,
    };
    let mut chess_bufs = ct_chessboard_detect_buffers_t {
        out_result: ptr::null_mut(),
        out_corners: ptr::null_mut(),
        corners_capacity: 0,
        out_corners_len: &mut chess_len,
    };
    let status = unsafe { ct_chessboard_detector_detect(&chess_args, &mut chess_bufs) };
    assert_eq!(status, ct_status_t::CT_STATUS_NOT_FOUND);
    assert_eq!(chess_len, 0);
    unsafe { ct_chessboard_detector_destroy(chess_detector) };

    let charuco_config = charuco_config_small_png();
    let mut charuco_detector = ptr::null_mut();
    let status = unsafe { ct_charuco_detector_create(&charuco_config, &mut charuco_detector) };
    assert_eq!(status, ct_status_t::CT_STATUS_OK);
    let mut charuco_corners_len = usize::MAX;
    let mut charuco_markers_len = usize::MAX;
    let charuco_args = ct_charuco_detect_args_t {
        detector: charuco_detector,
        image: &descriptor,
    };
    let mut charuco_bufs = ct_charuco_detect_buffers_t {
        out_result: ptr::null_mut(),
        out_corners: ptr::null_mut(),
        corners_capacity: 0,
        out_corners_len: &mut charuco_corners_len,
        out_markers: ptr::null_mut(),
        markers_capacity: 0,
        out_markers_len: &mut charuco_markers_len,
    };
    let status = unsafe { ct_charuco_detector_detect(&charuco_args, &mut charuco_bufs) };
    assert_eq!(status, ct_status_t::CT_STATUS_NOT_FOUND);
    assert_eq!(charuco_corners_len, 0);
    assert_eq!(charuco_markers_len, 0);
    unsafe { ct_charuco_detector_destroy(charuco_detector) };

    let marker_config = marker_board_config_crop_png();
    let mut marker_detector = ptr::null_mut();
    let status = unsafe { ct_marker_board_detector_create(&marker_config, &mut marker_detector) };
    assert_eq!(status, ct_status_t::CT_STATUS_OK);
    let mut marker_corners_len = usize::MAX;
    let marker_args = ct_marker_board_detect_args_t {
        detector: marker_detector,
        image: &descriptor,
    };
    let mut marker_bufs = ct_marker_board_detect_buffers_t {
        out_result: ptr::null_mut(),
        out_corners: ptr::null_mut(),
        corners_capacity: 0,
        out_corners_len: &mut marker_corners_len,
    };
    let status = unsafe { ct_marker_board_detector_detect(&marker_args, &mut marker_bufs) };
    assert_eq!(status, ct_status_t::CT_STATUS_NOT_FOUND);
    assert_eq!(marker_corners_len, 0);
    unsafe { ct_marker_board_detector_destroy(marker_detector) };

    let puzzle_config = puzzleboard_config_small_png();
    let mut puzzle_detector = ptr::null_mut();
    let status = unsafe { ct_puzzleboard_detector_create(&puzzle_config, &mut puzzle_detector) };
    assert_eq!(status, ct_status_t::CT_STATUS_OK);
    let mut puzzle_corners_len = usize::MAX;
    let puzzle_args = ct_puzzleboard_detect_args_t {
        detector: puzzle_detector,
        image: &descriptor,
    };
    let mut puzzle_bufs = ct_puzzleboard_detect_buffers_t {
        out_result: ptr::null_mut(),
        out_corners: ptr::null_mut(),
        corners_capacity: 0,
        out_corners_len: &mut puzzle_corners_len,
    };
    let status = unsafe { ct_puzzleboard_detector_detect(&puzzle_args, &mut puzzle_bufs) };
    assert_eq!(status, ct_status_t::CT_STATUS_NOT_FOUND);
    assert_eq!(puzzle_corners_len, 0);
    unsafe { ct_puzzleboard_detector_destroy(puzzle_detector) };
}

/// Query-then-copy a diagnostics JSON accessor and return the decoded
/// string. `accessor` runs the underlying detector; the helper exercises
/// the NULL-query → too-small → copy contract shared with
/// [`ct_last_error_message`].
///
/// Detection is not bit-for-bit deterministic across runs (HashSet
/// iteration order shifts which corners get labelled), so the helper
/// allocates a generous buffer rather than asserting an exact required
/// length, and treats a zero-capacity copy as the buffer-too-small case.
fn diagnostics_json_via(accessor: impl Fn(*mut i8, usize, *mut usize) -> ct_status_t) -> String {
    let mut len = usize::MAX;
    let status = accessor(ptr::null_mut(), 0, &mut len);
    assert_eq!(status, ct_status_t::CT_STATUS_OK);
    assert!(len > 0, "diagnostics JSON length must be non-zero");

    // A 1-byte buffer cannot hold any non-empty JSON plus its NUL.
    let mut tiny = [0_i8; 1];
    let status = accessor(tiny.as_mut_ptr(), tiny.len(), &mut len);
    assert_eq!(status, ct_status_t::CT_STATUS_BUFFER_TOO_SMALL);

    // Generous headroom absorbs any per-run length jitter.
    let mut buf = vec![0_i8; len * 2 + 64];
    let status = accessor(buf.as_mut_ptr(), buf.len(), &mut len);
    assert_eq!(status, ct_status_t::CT_STATUS_OK);
    unsafe { CStr::from_ptr(buf.as_ptr()) }
        .to_str()
        .unwrap()
        .to_string()
}

#[test]
fn chessboard_diagnostics_json_is_well_formed() {
    let config = chessboard_config_mid_png();
    let mut detector = ptr::null_mut();
    let status = unsafe { ct_chessboard_detector_create(&config, &mut detector) };
    assert_eq!(status, ct_status_t::CT_STATUS_OK);

    let image = load_gray("mid.png");
    let descriptor = image_descriptor(&image);
    let args = ct_chessboard_detect_args_t {
        detector,
        image: &descriptor,
    };
    let json = diagnostics_json_via(|out, cap, len| unsafe {
        ct_chessboard_detector_detect_diagnostics_json(&args, out, cap, len)
    });
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert!(parsed.get("schema").is_some());
    assert!(parsed.get("corners").is_some());

    unsafe { ct_chessboard_detector_destroy(detector) };
}

#[test]
fn charuco_diagnostics_json_is_well_formed() {
    let config = charuco_config_small_png();
    let mut detector = ptr::null_mut();
    let status = unsafe { ct_charuco_detector_create(&config, &mut detector) };
    assert_eq!(status, ct_status_t::CT_STATUS_OK);

    let image = load_gray("small.png");
    let descriptor = image_descriptor(&image);
    let args = ct_charuco_detect_args_t {
        detector,
        image: &descriptor,
    };
    let json = diagnostics_json_via(|out, cap, len| unsafe {
        ct_charuco_detector_detect_diagnostics_json(&args, out, cap, len)
    });
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert!(parsed.is_object());

    unsafe { ct_charuco_detector_destroy(detector) };
}

#[test]
fn marker_board_diagnostics_json_is_well_formed() {
    let config = marker_board_config_crop_png();
    let mut detector = ptr::null_mut();
    let status = unsafe { ct_marker_board_detector_create(&config, &mut detector) };
    assert_eq!(status, ct_status_t::CT_STATUS_OK);

    let image = load_gray("markerboard_crop.png");
    let descriptor = image_descriptor(&image);
    let args = ct_marker_board_detect_args_t {
        detector,
        image: &descriptor,
    };
    let json = diagnostics_json_via(|out, cap, len| unsafe {
        ct_marker_board_detector_detect_diagnostics_json(&args, out, cap, len)
    });
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert!(parsed.get("inliers").is_some());

    unsafe { ct_marker_board_detector_destroy(detector) };
}

#[test]
fn puzzleboard_diagnostics_json_is_well_formed() {
    let config = puzzleboard_config_small_png();
    let mut detector = ptr::null_mut();
    let status = unsafe { ct_puzzleboard_detector_create(&config, &mut detector) };
    assert_eq!(status, ct_status_t::CT_STATUS_OK);

    let image = load_gray("puzzleboard_small.png");
    let descriptor = image_descriptor(&image);
    let args = ct_puzzleboard_detect_args_t {
        detector,
        image: &descriptor,
    };
    let json = diagnostics_json_via(|out, cap, len| unsafe {
        ct_puzzleboard_detector_detect_diagnostics_json(&args, out, cap, len)
    });
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert!(parsed.get("observed_edges").is_some());
    assert!(parsed.get("decode").is_some());

    unsafe { ct_puzzleboard_detector_destroy(detector) };
}
