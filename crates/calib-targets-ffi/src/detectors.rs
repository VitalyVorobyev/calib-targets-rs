//! Per-detector create/destroy/detect implementations and exported C symbols.
//!
//! All functions here call into the private helpers defined in `lib.rs`
//! (accessible because this is a child module):
//! `require_ref`, `require_mut_ref`, `write_required_len`, `write_optional_result`,
//! `validate_output_buffer`, `copy_output_slice`, and `PreparedGrayImage`.

// Handle types (ct_*_detector_t) are defined in the parent lib.rs and accessed
// via `super::`. Config, result, and data types come from the types module.
use super::{
    alignment_to_ffi, build_detection_header, chessboard_params_default_values,
    circle_candidate_to_ffi, circle_match_to_ffi, convert_charuco_detector_params,
    convert_chess_config, convert_chessboard_params, convert_marker_board_params,
    convert_puzzleboard_params, copy_output_slice, ct_charuco_detector_t, ct_chessboard_detector_t,
    ct_marker_board_detector_t, ct_puzzleboard_detector_t, labeled_corner_to_ffi,
    map_charuco_create_error, map_charuco_detect_error, map_puzzleboard_create_error,
    map_puzzleboard_detect_error, marker_detection_to_ffi, panic_message, require_mut_ref,
    require_ref, set_last_error_message, validate_output_buffer, write_optional_result,
    write_required_len, CharucoDetectCall, CharucoDetector, ChessboardDetector, FfiError,
    FfiResult, MarkerBoardDetectCall, MarkerBoardDetector, PreparedGrayImage,
    PuzzleBoardDetectCall, PuzzleBoardDetector,
};
use crate::convert::puzzleboard_scoring_mode_to_ffi;
use crate::error::ffi_status;
use crate::types::{
    ct_charuco_detector_config_t, ct_charuco_result_t, ct_chessboard_detector_config_t,
    ct_chessboard_params_t, ct_chessboard_result_t, ct_circle_candidate_t, ct_circle_match_t,
    ct_gray_image_u8_t, ct_labeled_corner_t, ct_marker_board_detector_config_t,
    ct_marker_board_result_t, ct_marker_detection_t, ct_puzzleboard_detector_config_t,
    ct_puzzleboard_result_t, ct_status_t, CT_FALSE, CT_TRUE,
};
use std::panic::{catch_unwind, AssertUnwindSafe};

// ─── Detector create/destroy/detect impls ────────────────────────────────────

pub(super) unsafe fn chessboard_detector_create_impl(
    config: *const ct_chessboard_detector_config_t,
    out_detector: *mut *mut ct_chessboard_detector_t,
) -> FfiResult<()> {
    // SAFETY: caller contract (from `ct_chessboard_detector_create`): `config` is a
    // valid, aligned, non-null pointer for the duration of the call.
    let config = unsafe { require_ref(config, "config")? };
    // SAFETY: `out_detector` is a valid writable pointer for the duration of the call.
    let out_detector = unsafe { require_mut_ref(out_detector, "out_detector")? };
    let chess = convert_chess_config(&config.chess)?;
    let detector = ChessboardDetector::new(convert_chessboard_params(&config.chessboard)?);
    let handle = Box::new(ct_chessboard_detector_t { chess, detector });
    *out_detector = Box::into_raw(handle);
    Ok(())
}

pub(super) unsafe fn charuco_detector_create_impl(
    config: *const ct_charuco_detector_config_t,
    out_detector: *mut *mut ct_charuco_detector_t,
) -> FfiResult<()> {
    // SAFETY: caller contract (from `ct_charuco_detector_create`): `config` is a
    // valid, aligned, non-null pointer for the duration of the call.
    let config = unsafe { require_ref(config, "config")? };
    // SAFETY: `out_detector` is a valid writable pointer for the duration of the call.
    let out_detector = unsafe { require_mut_ref(out_detector, "out_detector")? };
    let chess = convert_chess_config(&config.chess)?;
    let detector = CharucoDetector::new(convert_charuco_detector_params(&config.detector)?)
        .map_err(map_charuco_create_error)?;
    let handle = Box::new(ct_charuco_detector_t { chess, detector });
    *out_detector = Box::into_raw(handle);
    Ok(())
}

pub(super) unsafe fn marker_board_detector_create_impl(
    config: *const ct_marker_board_detector_config_t,
    out_detector: *mut *mut ct_marker_board_detector_t,
) -> FfiResult<()> {
    // SAFETY: caller contract (from `ct_marker_board_detector_create`): `config` is a
    // valid, aligned, non-null pointer for the duration of the call.
    let config = unsafe { require_ref(config, "config")? };
    // SAFETY: `out_detector` is a valid writable pointer for the duration of the call.
    let out_detector = unsafe { require_mut_ref(out_detector, "out_detector")? };
    let chess = convert_chess_config(&config.chess)?;
    let detector = MarkerBoardDetector::new(convert_marker_board_params(&config.detector)?);
    let handle = Box::new(ct_marker_board_detector_t { chess, detector });
    *out_detector = Box::into_raw(handle);
    Ok(())
}

pub(super) unsafe fn puzzleboard_detector_create_impl(
    config: *const ct_puzzleboard_detector_config_t,
    out_detector: *mut *mut ct_puzzleboard_detector_t,
) -> FfiResult<()> {
    // SAFETY: caller contract (from `ct_puzzleboard_detector_create`): `config` is a
    // valid, aligned, non-null pointer for the duration of the call.
    let config = unsafe { require_ref(config, "config")? };
    // SAFETY: `out_detector` is a valid writable pointer for the duration of the call.
    let out_detector = unsafe { require_mut_ref(out_detector, "out_detector")? };
    let chess = convert_chess_config(&config.chess)?;
    let detector = PuzzleBoardDetector::new(convert_puzzleboard_params(&config.detector)?)
        .map_err(map_puzzleboard_create_error)?;
    let handle = Box::new(ct_puzzleboard_detector_t { chess, detector });
    *out_detector = Box::into_raw(handle);
    Ok(())
}

pub(super) unsafe fn chessboard_detector_detect_impl(
    detector: *const ct_chessboard_detector_t,
    image: *const ct_gray_image_u8_t,
    out_result: *mut ct_chessboard_result_t,
    out_corners: *mut ct_labeled_corner_t,
    corners_capacity: usize,
    out_corners_len: *mut usize,
) -> FfiResult<()> {
    // SAFETY: caller contract (from `ct_chessboard_detector_detect`): `detector` is a
    // valid handle returned by `ct_chessboard_detector_create`, alive for this call.
    let detector = unsafe { require_ref(detector, "detector")? };
    // SAFETY: caller contract: `image` points to a valid `ct_gray_image_u8_t` struct.
    let image = unsafe { require_ref(image, "image")? };
    let prepared = PreparedGrayImage::from_descriptor(image)?;
    let corners = prepared.detect_corners(&detector.chess)?;

    let Some(detection) = detector.detector.detect(&corners) else {
        // SAFETY: `out_corners_len` and `out_result` are valid writable pointers per
        // the caller contract; null is handled inside the helpers.
        unsafe {
            write_required_len(out_corners_len, 0, "out_corners_len")?;
            write_optional_result(out_result, ct_chessboard_result_t::default());
        }
        return Err(FfiError::not_found("chessboard not detected"));
    };

    let corners_out: Vec<ct_labeled_corner_t> = detection
        .target
        .corners
        .iter()
        .map(labeled_corner_to_ffi)
        .collect();
    let result = ct_chessboard_result_t {
        detection: build_detection_header(&detection.target),
        grid_direction_0_rad: detection.grid_directions[0],
        grid_direction_1_rad: detection.grid_directions[1],
        cell_size: detection.cell_size,
    };

    // SAFETY: `out_corners_len` and `out_result` are valid writable pointers per
    // the caller contract; null is handled inside the helpers.
    unsafe {
        write_required_len(out_corners_len, corners_out.len(), "out_corners_len")?;
        write_optional_result(out_result, result);
    }
    let copy_corners = validate_output_buffer(
        out_corners,
        corners_capacity,
        corners_out.len(),
        "out_corners",
    )?;
    if copy_corners {
        // SAFETY: `out_corners` has been validated to be non-null with sufficient
        // capacity by `validate_output_buffer` above.
        unsafe { copy_output_slice(out_corners, &corners_out) };
    }
    Ok(())
}

pub(super) unsafe fn charuco_detector_detect_impl(call: CharucoDetectCall) -> FfiResult<()> {
    // SAFETY: caller contract: `detector` is a valid `ct_charuco_detector_t` handle.
    let detector = unsafe { require_ref(call.detector, "detector")? };
    // SAFETY: caller contract: `image` points to a valid `ct_gray_image_u8_t` struct.
    let image = unsafe { require_ref(call.image, "image")? };
    let prepared = PreparedGrayImage::from_descriptor(image)?;
    let corners = prepared.detect_corners(&detector.chess)?;
    let view = prepared.view();

    let detection = detector
        .detector
        .detect(&view, &corners)
        .map_err(map_charuco_detect_error);

    let detection = match detection {
        Ok(detection) => detection,
        Err(err) => {
            // SAFETY: output pointers are valid per caller contract; null is handled inside helpers.
            unsafe {
                write_required_len(call.out_corners_len, 0, "out_corners_len")?;
                write_required_len(call.out_markers_len, 0, "out_markers_len")?;
                write_optional_result(call.out_result, ct_charuco_result_t::default());
            }
            return Err(err);
        }
    };

    let corners_out: Vec<ct_labeled_corner_t> = detection
        .detection
        .corners
        .iter()
        .map(labeled_corner_to_ffi)
        .collect();
    let markers_out: Vec<ct_marker_detection_t> = detection
        .markers
        .iter()
        .map(marker_detection_to_ffi)
        .collect();
    let result = ct_charuco_result_t {
        detection: build_detection_header(&detection.detection),
        markers_len: markers_out.len(),
        alignment: alignment_to_ffi(detection.alignment),
    };

    // SAFETY: output pointers are valid per caller contract; null is handled inside helpers.
    unsafe {
        write_required_len(call.out_corners_len, corners_out.len(), "out_corners_len")?;
        write_required_len(call.out_markers_len, markers_out.len(), "out_markers_len")?;
        write_optional_result(call.out_result, result);
    }

    let copy_corners = validate_output_buffer(
        call.out_corners,
        call.corners_capacity,
        corners_out.len(),
        "out_corners",
    )?;
    let copy_markers = validate_output_buffer(
        call.out_markers,
        call.markers_capacity,
        markers_out.len(),
        "out_markers",
    )?;

    if copy_corners {
        // SAFETY: `out_corners` capacity validated by `validate_output_buffer` above.
        unsafe { copy_output_slice(call.out_corners, &corners_out) };
    }
    if copy_markers {
        // SAFETY: `out_markers` capacity validated by `validate_output_buffer` above.
        unsafe { copy_output_slice(call.out_markers, &markers_out) };
    }
    Ok(())
}

pub(super) unsafe fn marker_board_detector_detect_impl(
    call: MarkerBoardDetectCall,
) -> FfiResult<()> {
    // SAFETY: caller contract: `detector` is a valid `ct_marker_board_detector_t` handle.
    let detector = unsafe { require_ref(call.detector, "detector")? };
    // SAFETY: caller contract: `image` points to a valid `ct_gray_image_u8_t` struct.
    let image = unsafe { require_ref(call.image, "image")? };
    let prepared = PreparedGrayImage::from_descriptor(image)?;
    let corners = prepared.detect_corners(&detector.chess)?;
    let view = prepared.view();

    let Some(detection) = detector
        .detector
        .detect_from_image_and_corners(&view, &corners)
    else {
        // SAFETY: output pointers are valid per caller contract; null is handled inside helpers.
        unsafe {
            write_required_len(call.out_corners_len, 0, "out_corners_len")?;
            write_required_len(
                call.out_circle_candidates_len,
                0,
                "out_circle_candidates_len",
            )?;
            write_required_len(call.out_circle_matches_len, 0, "out_circle_matches_len")?;
            write_optional_result(call.out_result, ct_marker_board_result_t::default());
        }
        return Err(FfiError::not_found("marker board not detected"));
    };

    let corners_out: Vec<ct_labeled_corner_t> = detection
        .detection
        .corners
        .iter()
        .map(labeled_corner_to_ffi)
        .collect();
    let circle_candidates_out: Vec<ct_circle_candidate_t> = detection
        .circle_candidates
        .iter()
        .map(circle_candidate_to_ffi)
        .collect();
    let circle_matches_out: Vec<ct_circle_match_t> = detection
        .circle_matches
        .iter()
        .map(circle_match_to_ffi)
        .collect();
    let result = ct_marker_board_result_t {
        detection: build_detection_header(&detection.detection),
        circle_candidates_len: circle_candidates_out.len(),
        circle_matches_len: circle_matches_out.len(),
        has_alignment: if detection.alignment.is_some() {
            CT_TRUE
        } else {
            CT_FALSE
        },
        alignment: detection
            .alignment
            .map(alignment_to_ffi)
            .unwrap_or_default(),
        alignment_inliers: detection.alignment_inliers,
    };

    // SAFETY: output pointers are valid per caller contract; null is handled inside helpers.
    unsafe {
        write_required_len(call.out_corners_len, corners_out.len(), "out_corners_len")?;
        write_required_len(
            call.out_circle_candidates_len,
            circle_candidates_out.len(),
            "out_circle_candidates_len",
        )?;
        write_required_len(
            call.out_circle_matches_len,
            circle_matches_out.len(),
            "out_circle_matches_len",
        )?;
        write_optional_result(call.out_result, result);
    }

    let copy_corners = validate_output_buffer(
        call.out_corners,
        call.corners_capacity,
        corners_out.len(),
        "out_corners",
    )?;
    let copy_circle_candidates = validate_output_buffer(
        call.out_circle_candidates,
        call.circle_candidates_capacity,
        circle_candidates_out.len(),
        "out_circle_candidates",
    )?;
    let copy_circle_matches = validate_output_buffer(
        call.out_circle_matches,
        call.circle_matches_capacity,
        circle_matches_out.len(),
        "out_circle_matches",
    )?;

    if copy_corners {
        // SAFETY: `out_corners` capacity validated by `validate_output_buffer` above.
        unsafe { copy_output_slice(call.out_corners, &corners_out) };
    }
    if copy_circle_candidates {
        // SAFETY: `out_circle_candidates` capacity validated by `validate_output_buffer` above.
        unsafe { copy_output_slice(call.out_circle_candidates, &circle_candidates_out) };
    }
    if copy_circle_matches {
        // SAFETY: `out_circle_matches` capacity validated by `validate_output_buffer` above.
        unsafe { copy_output_slice(call.out_circle_matches, &circle_matches_out) };
    }
    Ok(())
}

pub(super) unsafe fn puzzleboard_detector_detect_impl(
    call: PuzzleBoardDetectCall,
) -> FfiResult<()> {
    // SAFETY: caller contract: `detector` is a valid `ct_puzzleboard_detector_t` handle.
    let detector = unsafe { require_ref(call.detector, "detector")? };
    // SAFETY: caller contract: `image` points to a valid `ct_gray_image_u8_t` struct.
    let image = unsafe { require_ref(call.image, "image")? };
    let prepared = PreparedGrayImage::from_descriptor(image)?;
    let corners = prepared.detect_corners(&detector.chess)?;
    let view = prepared.view();

    let detection = detector
        .detector
        .detect(&view, &corners)
        .map_err(map_puzzleboard_detect_error);

    let detection = match detection {
        Ok(detection) => detection,
        Err(err) => {
            // SAFETY: output pointers are valid per caller contract; null is handled inside helpers.
            unsafe {
                write_required_len(call.out_corners_len, 0, "out_corners_len")?;
                write_optional_result(call.out_result, ct_puzzleboard_result_t::default());
            }
            return Err(err);
        }
    };

    let corners_out: Vec<ct_labeled_corner_t> = detection
        .detection
        .corners
        .iter()
        .map(labeled_corner_to_ffi)
        .collect();
    let result = ct_puzzleboard_result_t {
        detection: build_detection_header(&detection.detection),
        alignment: alignment_to_ffi(detection.alignment),
        edges_observed: detection.decode.edges_observed,
        edges_matched: detection.decode.edges_matched,
        mean_bit_confidence: detection.decode.mean_confidence,
        bit_error_rate: detection.decode.bit_error_rate,
        master_origin_row: detection.decode.master_origin_row,
        master_origin_col: detection.decode.master_origin_col,
        score_best: detection
            .decode
            .score_best
            .map(crate::types::ct_optional_f32_t::some)
            .unwrap_or_default(),
        score_runner_up: detection
            .decode
            .score_runner_up
            .map(crate::types::ct_optional_f32_t::some)
            .unwrap_or_default(),
        score_margin: detection
            .decode
            .score_margin
            .map(crate::types::ct_optional_f32_t::some)
            .unwrap_or_default(),
        scoring_mode: detection
            .decode
            .scoring_mode
            .map(puzzleboard_scoring_mode_to_ffi)
            .unwrap_or_default(),
        has_runner_up_alignment: if detection.decode.runner_up_origin_row.is_some()
            && detection.decode.runner_up_origin_col.is_some()
            && detection.decode.runner_up_transform.is_some()
        {
            CT_TRUE
        } else {
            CT_FALSE
        },
        runner_up_alignment: match (
            detection.decode.runner_up_transform,
            detection.decode.runner_up_origin_col,
            detection.decode.runner_up_origin_row,
        ) {
            (Some(transform), Some(origin_col), Some(origin_row)) => {
                alignment_to_ffi(calib_targets::core::GridAlignment {
                    transform,
                    translation: [origin_col, origin_row],
                })
            }
            _ => Default::default(),
        },
        observed_edges_len: detection.observed_edges.len(),
    };

    // SAFETY: output pointers are valid per caller contract; null is handled inside helpers.
    unsafe {
        write_required_len(call.out_corners_len, corners_out.len(), "out_corners_len")?;
        write_optional_result(call.out_result, result);
    }
    let copy_corners = validate_output_buffer(
        call.out_corners,
        call.corners_capacity,
        corners_out.len(),
        "out_corners",
    )?;
    if copy_corners {
        // SAFETY: `out_corners` capacity validated by `validate_output_buffer` above.
        unsafe { copy_output_slice(call.out_corners, &corners_out) };
    }
    Ok(())
}

// ─── Public exported functions ───────────────────────────────────────────────

/// Return a `ct_chessboard_params_t` populated from
/// `DetectorParams::default()`. Exposed as a C symbol so callers don't
/// need to hand-fill 30+ fields.
/// # Safety
/// `out` must be a valid, properly aligned pointer to a writable
/// `ct_chessboard_params_t` storage location. `NULL` is allowed and
/// is a no-op. The caller retains ownership of the storage.
#[no_mangle]
pub unsafe extern "C" fn ct_chessboard_params_init_default(out: *mut ct_chessboard_params_t) {
    // SAFETY: caller contract: if non-null, `out` is a valid writable pointer. NULL → no-op.
    let Some(out) = (unsafe { out.as_mut() }) else {
        return;
    };
    *out = chessboard_params_default_values();
}

/// Create a chessboard detector handle.
///
/// # Safety
///
/// `config` and `out_detector` must be valid non-null pointers. On success,
/// `*out_detector` receives a new handle owned by the caller.
#[no_mangle]
pub unsafe extern "C" fn ct_chessboard_detector_create(
    config: *const ct_chessboard_detector_config_t,
    out_detector: *mut *mut ct_chessboard_detector_t,
) -> ct_status_t {
    ffi_status(|| unsafe { chessboard_detector_create_impl(config, out_detector) })
}

/// Destroy a chessboard detector handle.
///
/// Passing `NULL` is allowed and has no effect.
///
/// # Safety
///
/// `detector` must either be null or a handle returned by
/// [`ct_chessboard_detector_create`] that has not already been destroyed.
#[no_mangle]
pub unsafe extern "C" fn ct_chessboard_detector_destroy(detector: *mut ct_chessboard_detector_t) {
    if let Err(payload) = catch_unwind(AssertUnwindSafe(|| unsafe {
        if !detector.is_null() {
            // SAFETY: caller promises `detector` is either null or a handle
            // created by `ct_chessboard_detector_create`.
            drop(Box::from_raw(detector));
        }
    })) {
        set_last_error_message(format!(
            "panic across FFI boundary: {}",
            panic_message(payload)
        ));
    }
}

/// Run end-to-end chessboard detection on a grayscale image.
///
/// `out_corners_len` is required and always receives the required number of
/// labeled-corner entries. Passing `out_corners = NULL` and
/// `corners_capacity = 0` queries the required length without copying corner
/// data.
///
/// # Safety
///
/// `detector`, `image`, and `out_corners_len` must be valid non-null pointers.
/// If `out_result` is non-null it must be writable. If `out_corners` is
/// non-null it must point to writable storage for at least `corners_capacity`
/// entries.
#[no_mangle]
pub unsafe extern "C" fn ct_chessboard_detector_detect(
    detector: *const ct_chessboard_detector_t,
    image: *const ct_gray_image_u8_t,
    out_result: *mut ct_chessboard_result_t,
    out_corners: *mut ct_labeled_corner_t,
    corners_capacity: usize,
    out_corners_len: *mut usize,
) -> ct_status_t {
    ffi_status(|| unsafe {
        chessboard_detector_detect_impl(
            detector,
            image,
            out_result,
            out_corners,
            corners_capacity,
            out_corners_len,
        )
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) unsafe fn chessboard_detector_detect_all_impl(
    detector: *const ct_chessboard_detector_t,
    image: *const ct_gray_image_u8_t,
    out_results: *mut ct_chessboard_result_t,
    results_capacity: usize,
    out_results_len: *mut usize,
    out_corners: *mut ct_labeled_corner_t,
    all_corners_capacity: usize,
    out_all_corners_len: *mut usize,
) -> FfiResult<()> {
    // SAFETY: caller contract (from `ct_chessboard_detector_detect_all`): `detector` is a
    // valid handle created by `ct_chessboard_detector_create`, not yet destroyed.
    let detector = unsafe { require_ref(detector, "detector")? };
    // SAFETY: caller contract: `image` points to a valid `ct_gray_image_u8_t` struct.
    let image = unsafe { require_ref(image, "image")? };
    let prepared = PreparedGrayImage::from_descriptor(image)?;
    let corners = prepared.detect_corners(&detector.chess)?;

    let detections = detector.detector.detect_all(&corners);

    let results_out: Vec<ct_chessboard_result_t> = detections
        .iter()
        .map(|d| ct_chessboard_result_t {
            detection: build_detection_header(&d.target),
            grid_direction_0_rad: d.grid_directions[0],
            grid_direction_1_rad: d.grid_directions[1],
            cell_size: d.cell_size,
        })
        .collect();
    let corners_out: Vec<ct_labeled_corner_t> = detections
        .iter()
        .flat_map(|d| d.target.corners.iter().map(labeled_corner_to_ffi))
        .collect();

    // SAFETY: `out_results_len` and `out_all_corners_len` are valid writable pointers per
    // caller contract from `ct_chessboard_detector_detect_all`.
    unsafe {
        write_required_len(out_results_len, results_out.len(), "out_results_len")?;
        write_required_len(
            out_all_corners_len,
            corners_out.len(),
            "out_all_corners_len",
        )?;
    }
    let copy_results = validate_output_buffer(
        out_results,
        results_capacity,
        results_out.len(),
        "out_results",
    )?;
    let copy_corners = validate_output_buffer(
        out_corners,
        all_corners_capacity,
        corners_out.len(),
        "out_corners",
    )?;
    if copy_results {
        // SAFETY: `out_results` capacity validated by `validate_output_buffer` above.
        unsafe { copy_output_slice(out_results, &results_out) };
    }
    if copy_corners {
        // SAFETY: `out_corners` capacity validated by `validate_output_buffer` above.
        unsafe { copy_output_slice(out_corners, &corners_out) };
    }
    Ok(())
}

/// Run end-to-end multi-component chessboard detection on a grayscale image.
///
/// Returns every same-board component the detector recovers, up to
/// `DetectorParams::max_components`. The `out_corners` buffer receives all
/// corners from all components concatenated; use `result[i].detection.corners_len`
/// to slice each component's contribution.
///
/// Both `out_results_len` and `out_all_corners_len` are required and always
/// receive the required array lengths. Passing `NULL` output arrays with
/// capacity `0` queries the required lengths without copying data.
///
/// # Safety
///
/// `detector`, `image`, `out_results_len`, and `out_all_corners_len` must be
/// valid non-null pointers. If `out_results` is non-null it must point to
/// writable storage for at least `results_capacity` entries. If `out_corners`
/// is non-null it must point to writable storage for at least
/// `all_corners_capacity` entries.
#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn ct_chessboard_detector_detect_all(
    detector: *const ct_chessboard_detector_t,
    image: *const ct_gray_image_u8_t,
    out_results: *mut ct_chessboard_result_t,
    results_capacity: usize,
    out_results_len: *mut usize,
    out_corners: *mut ct_labeled_corner_t,
    all_corners_capacity: usize,
    out_all_corners_len: *mut usize,
) -> ct_status_t {
    ffi_status(|| unsafe {
        chessboard_detector_detect_all_impl(
            detector,
            image,
            out_results,
            results_capacity,
            out_results_len,
            out_corners,
            all_corners_capacity,
            out_all_corners_len,
        )
    })
}

/// Create a ChArUco detector handle.
///
/// # Safety
///
/// `config` and `out_detector` must be valid non-null pointers. On success,
/// `*out_detector` receives a new handle owned by the caller.
#[no_mangle]
pub unsafe extern "C" fn ct_charuco_detector_create(
    config: *const ct_charuco_detector_config_t,
    out_detector: *mut *mut ct_charuco_detector_t,
) -> ct_status_t {
    ffi_status(|| unsafe { charuco_detector_create_impl(config, out_detector) })
}

/// Destroy a ChArUco detector handle.
///
/// Passing `NULL` is allowed and has no effect.
///
/// # Safety
///
/// `detector` must either be null or a handle returned by
/// [`ct_charuco_detector_create`] that has not already been destroyed.
#[no_mangle]
pub unsafe extern "C" fn ct_charuco_detector_destroy(detector: *mut ct_charuco_detector_t) {
    if let Err(payload) = catch_unwind(AssertUnwindSafe(|| unsafe {
        if !detector.is_null() {
            // SAFETY: caller promises `detector` is either null or a handle
            // created by `ct_charuco_detector_create`.
            drop(Box::from_raw(detector));
        }
    })) {
        set_last_error_message(format!(
            "panic across FFI boundary: {}",
            panic_message(payload)
        ));
    }
}

/// Run end-to-end ChArUco detection on a grayscale image.
///
/// `out_corners_len` and `out_markers_len` are required and always receive the
/// required array lengths. Passing a `NULL` output array with capacity `0`
/// queries the corresponding required length without copying array data.
///
/// # Safety
///
/// `detector`, `image`, `out_corners_len`, and `out_markers_len` must be valid
/// non-null pointers. If `out_result` is non-null it must be writable. If
/// `out_corners` or `out_markers` is non-null, each must point to writable
/// storage for at least the matching capacity.
#[no_mangle]
pub unsafe extern "C" fn ct_charuco_detector_detect(
    detector: *const ct_charuco_detector_t,
    image: *const ct_gray_image_u8_t,
    out_result: *mut ct_charuco_result_t,
    out_corners: *mut ct_labeled_corner_t,
    corners_capacity: usize,
    out_corners_len: *mut usize,
    out_markers: *mut ct_marker_detection_t,
    markers_capacity: usize,
    out_markers_len: *mut usize,
) -> ct_status_t {
    ffi_status(|| unsafe {
        charuco_detector_detect_impl(CharucoDetectCall {
            detector,
            image,
            out_result,
            out_corners,
            corners_capacity,
            out_corners_len,
            out_markers,
            markers_capacity,
            out_markers_len,
        })
    })
}

/// Create a marker-board detector handle.
///
/// # Safety
///
/// `config` and `out_detector` must be valid non-null pointers. On success,
/// `*out_detector` receives a new handle owned by the caller.
#[no_mangle]
pub unsafe extern "C" fn ct_marker_board_detector_create(
    config: *const ct_marker_board_detector_config_t,
    out_detector: *mut *mut ct_marker_board_detector_t,
) -> ct_status_t {
    ffi_status(|| unsafe { marker_board_detector_create_impl(config, out_detector) })
}

/// Destroy a marker-board detector handle.
///
/// Passing `NULL` is allowed and has no effect.
///
/// # Safety
///
/// `detector` must either be null or a handle returned by
/// [`ct_marker_board_detector_create`] that has not already been destroyed.
#[no_mangle]
pub unsafe extern "C" fn ct_marker_board_detector_destroy(
    detector: *mut ct_marker_board_detector_t,
) {
    if let Err(payload) = catch_unwind(AssertUnwindSafe(|| unsafe {
        if !detector.is_null() {
            // SAFETY: caller promises `detector` is either null or a handle
            // created by `ct_marker_board_detector_create`.
            drop(Box::from_raw(detector));
        }
    })) {
        set_last_error_message(format!(
            "panic across FFI boundary: {}",
            panic_message(payload)
        ));
    }
}

/// Run end-to-end marker-board detection on a grayscale image.
///
/// The three `*_len` pointers are required and always receive the required
/// lengths for the corresponding output arrays. Passing a `NULL` output array
/// with capacity `0` queries the required length without copying array data.
///
/// # Safety
///
/// `detector`, `image`, and all three `*_len` pointers must be valid non-null
/// pointers. If `out_result` is non-null it must be writable. If any array
/// pointer is non-null it must point to writable storage for at least the
/// corresponding capacity.
#[no_mangle]
pub unsafe extern "C" fn ct_marker_board_detector_detect(
    detector: *const ct_marker_board_detector_t,
    image: *const ct_gray_image_u8_t,
    out_result: *mut ct_marker_board_result_t,
    out_corners: *mut ct_labeled_corner_t,
    corners_capacity: usize,
    out_corners_len: *mut usize,
    out_circle_candidates: *mut ct_circle_candidate_t,
    circle_candidates_capacity: usize,
    out_circle_candidates_len: *mut usize,
    out_circle_matches: *mut ct_circle_match_t,
    circle_matches_capacity: usize,
    out_circle_matches_len: *mut usize,
) -> ct_status_t {
    ffi_status(|| unsafe {
        marker_board_detector_detect_impl(MarkerBoardDetectCall {
            detector,
            image,
            out_result,
            out_corners,
            corners_capacity,
            out_corners_len,
            out_circle_candidates,
            circle_candidates_capacity,
            out_circle_candidates_len,
            out_circle_matches,
            circle_matches_capacity,
            out_circle_matches_len,
        })
    })
}

/// Create a PuzzleBoard detector handle.
///
/// # Safety
///
/// `config` and `out_detector` must be valid non-null pointers. On success,
/// `*out_detector` receives a new handle owned by the caller.
#[no_mangle]
pub unsafe extern "C" fn ct_puzzleboard_detector_create(
    config: *const ct_puzzleboard_detector_config_t,
    out_detector: *mut *mut ct_puzzleboard_detector_t,
) -> ct_status_t {
    ffi_status(|| unsafe { puzzleboard_detector_create_impl(config, out_detector) })
}

/// Destroy a PuzzleBoard detector handle.
///
/// Passing `NULL` is allowed and has no effect.
///
/// # Safety
///
/// `detector` must either be null or a handle returned by
/// [`ct_puzzleboard_detector_create`] that has not already been destroyed.
#[no_mangle]
pub unsafe extern "C" fn ct_puzzleboard_detector_destroy(detector: *mut ct_puzzleboard_detector_t) {
    if let Err(payload) = catch_unwind(AssertUnwindSafe(|| unsafe {
        if !detector.is_null() {
            // SAFETY: caller promises `detector` is either null or a handle
            // created by `ct_puzzleboard_detector_create`.
            drop(Box::from_raw(detector));
        }
    })) {
        set_last_error_message(format!(
            "panic across FFI boundary: {}",
            panic_message(payload)
        ));
    }
}

/// Run end-to-end PuzzleBoard detection on a grayscale image.
///
/// `out_corners_len` is required and always receives the required number of
/// labeled-corner entries. Passing `out_corners = NULL` and
/// `corners_capacity = 0` queries the required length without copying corner
/// data. The returned corner grid coordinates are master-board `(I, J)` labels.
///
/// # Safety
///
/// `detector`, `image`, and `out_corners_len` must be valid non-null pointers.
/// If `out_result` is non-null it must be writable. If `out_corners` is
/// non-null it must point to writable storage for at least `corners_capacity`
/// entries.
#[no_mangle]
pub unsafe extern "C" fn ct_puzzleboard_detector_detect(
    detector: *const ct_puzzleboard_detector_t,
    image: *const ct_gray_image_u8_t,
    out_result: *mut ct_puzzleboard_result_t,
    out_corners: *mut ct_labeled_corner_t,
    corners_capacity: usize,
    out_corners_len: *mut usize,
) -> ct_status_t {
    ffi_status(|| unsafe {
        puzzleboard_detector_detect_impl(PuzzleBoardDetectCall {
            detector,
            image,
            out_result,
            out_corners,
            corners_capacity,
            out_corners_len,
        })
    })
}
