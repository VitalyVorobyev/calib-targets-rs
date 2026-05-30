//! Private detector create/destroy/detect/diagnostics implementations.
//!
//! Every function here is `pub(super)` and is wrapped by an exported
//! `#[no_mangle]` C symbol in [`super::exports`]. They call into the shared
//! helpers defined in `lib.rs` (accessible because `detectors` is a child
//! module): `require_ref`, `require_mut_ref`, `write_required_len`,
//! `write_optional_result`, `validate_output_buffer`, `copy_output_slice`,
//! `write_json_string`, and `PreparedGrayImage`.

// Handle types (ct_*_detector_t) are defined in the parent lib.rs and accessed
// via the crate root. Config, result, and data types come from the types module.
use crate::types::{
    ct_charuco_detector_config_t, ct_charuco_result_t, ct_chessboard_corner_t,
    ct_chessboard_detector_config_t, ct_chessboard_result_t, ct_labeled_corner_t,
    ct_marker_board_detector_config_t, ct_marker_board_result_t, ct_marker_detection_t,
    ct_puzzleboard_detector_config_t, ct_puzzleboard_result_t, ct_target_detection_t, CT_FALSE,
    CT_TARGET_KIND_CHARUCO, CT_TARGET_KIND_CHECKERBOARD_MARKER, CT_TARGET_KIND_PUZZLEBOARD,
    CT_TRUE,
};
use crate::{
    alignment_to_ffi, chessboard_corner_to_ffi, convert_charuco_detector_params,
    convert_chess_config, convert_chessboard_params, convert_marker_board_params,
    convert_puzzleboard_params, copy_output_slice, ct_charuco_detector_t, ct_chessboard_detector_t,
    ct_marker_board_detector_t, ct_puzzleboard_detector_t, labeled_corner_to_ffi,
    map_charuco_create_error, map_charuco_detect_error, map_puzzleboard_create_error,
    map_puzzleboard_detect_error, marker_detection_to_ffi, option_f32_to_ffi, require_mut_ref,
    require_ref, validate_output_buffer, write_json_string, write_optional_result,
    write_required_len, CharucoDetector, ChessboardDetector, FfiError, FfiResult,
    MarkerBoardDetector, PreparedGrayImage, PuzzleBoardDetector,
};
use std::ffi::c_char;

use super::{
    ct_charuco_detect_args_t, ct_charuco_detect_buffers_t, ct_chessboard_detect_all_args_t,
    ct_chessboard_detect_all_buffers_t, ct_chessboard_detect_args_t,
    ct_chessboard_detect_buffers_t, ct_marker_board_detect_args_t,
    ct_marker_board_detect_buffers_t, ct_puzzleboard_detect_args_t,
    ct_puzzleboard_detect_buffers_t,
};

// ─── Detector create impls ────────────────────────────────────────────────────

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

// ─── Detector detect impls ────────────────────────────────────────────────────

pub(super) unsafe fn chessboard_detector_detect_impl(
    args: *const ct_chessboard_detect_args_t,
    bufs: *mut ct_chessboard_detect_buffers_t,
) -> FfiResult<()> {
    // SAFETY: caller contract (from `ct_chessboard_detector_detect`): `args` and `bufs`
    // point to valid struct instances with valid sub-pointers per the per-field rules.
    let args = unsafe { require_ref(args, "args")? };
    let bufs = unsafe { require_mut_ref(bufs, "bufs")? };
    // SAFETY: caller contract: `args.detector` is a valid handle returned by
    // `ct_chessboard_detector_create`, alive for this call.
    let detector = unsafe { require_ref(args.detector, "args.detector")? };
    // SAFETY: caller contract: `args.image` points to a valid `ct_gray_image_u8_t` struct.
    let image = unsafe { require_ref(args.image, "args.image")? };
    let prepared = PreparedGrayImage::from_descriptor(image)?;
    let corners = prepared.detect_corners(&detector.chess)?;

    let Some(detection) = detector.detector.detect(&corners) else {
        // SAFETY: `bufs.out_corners_len` and `bufs.out_result` are valid writable
        // pointers per the caller contract; null is handled inside the helpers.
        unsafe {
            write_required_len(bufs.out_corners_len, 0, "out_corners_len")?;
            write_optional_result(bufs.out_result, ct_chessboard_result_t::default());
        }
        return Err(FfiError::not_found("chessboard not detected"));
    };

    let corners_out: Vec<ct_chessboard_corner_t> = detection
        .corners
        .iter()
        .map(chessboard_corner_to_ffi)
        .collect();
    let result = ct_chessboard_result_t {
        corners_len: corners_out.len(),
        cell_size: option_f32_to_ffi(detection.cell_size),
    };

    // SAFETY: `bufs.out_corners_len` and `bufs.out_result` are valid writable
    // pointers per the caller contract; null is handled inside the helpers.
    unsafe {
        write_required_len(bufs.out_corners_len, corners_out.len(), "out_corners_len")?;
        write_optional_result(bufs.out_result, result);
    }
    let copy_corners = validate_output_buffer(
        bufs.out_corners,
        bufs.corners_capacity,
        corners_out.len(),
        "out_corners",
    )?;
    if copy_corners {
        // SAFETY: `out_corners` has been validated to be non-null with sufficient
        // capacity by `validate_output_buffer` above.
        unsafe { copy_output_slice(bufs.out_corners, &corners_out) };
    }
    Ok(())
}

pub(super) unsafe fn charuco_detector_detect_impl(
    args: *const ct_charuco_detect_args_t,
    bufs: *mut ct_charuco_detect_buffers_t,
) -> FfiResult<()> {
    // SAFETY: caller contract (from `ct_charuco_detector_detect`): `args` and `bufs`
    // point to valid struct instances with valid sub-pointers per the per-field rules.
    let args = unsafe { require_ref(args, "args")? };
    let bufs = unsafe { require_mut_ref(bufs, "bufs")? };
    // SAFETY: caller contract: `args.detector` is a valid `ct_charuco_detector_t` handle.
    let detector = unsafe { require_ref(args.detector, "args.detector")? };
    // SAFETY: caller contract: `args.image` points to a valid `ct_gray_image_u8_t` struct.
    let image = unsafe { require_ref(args.image, "args.image")? };
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
                write_required_len(bufs.out_corners_len, 0, "out_corners_len")?;
                write_required_len(bufs.out_markers_len, 0, "out_markers_len")?;
                write_optional_result(bufs.out_result, ct_charuco_result_t::default());
            }
            return Err(err);
        }
    };

    let corners_out: Vec<ct_labeled_corner_t> = detection
        .corners
        .iter()
        .map(|corner| labeled_corner_to_ffi(&corner.to_labeled()))
        .collect();
    let markers_out: Vec<ct_marker_detection_t> = detection
        .markers
        .iter()
        .map(marker_detection_to_ffi)
        .collect();
    let result = ct_charuco_result_t {
        detection: ct_target_detection_t {
            kind: CT_TARGET_KIND_CHARUCO,
            corners_len: corners_out.len(),
        },
        markers_len: markers_out.len(),
        alignment: alignment_to_ffi(detection.alignment),
    };

    // SAFETY: output pointers are valid per caller contract; null is handled inside helpers.
    unsafe {
        write_required_len(bufs.out_corners_len, corners_out.len(), "out_corners_len")?;
        write_required_len(bufs.out_markers_len, markers_out.len(), "out_markers_len")?;
        write_optional_result(bufs.out_result, result);
    }

    let copy_corners = validate_output_buffer(
        bufs.out_corners,
        bufs.corners_capacity,
        corners_out.len(),
        "out_corners",
    )?;
    let copy_markers = validate_output_buffer(
        bufs.out_markers,
        bufs.markers_capacity,
        markers_out.len(),
        "out_markers",
    )?;

    if copy_corners {
        // SAFETY: `out_corners` capacity validated by `validate_output_buffer` above.
        unsafe { copy_output_slice(bufs.out_corners, &corners_out) };
    }
    if copy_markers {
        // SAFETY: `out_markers` capacity validated by `validate_output_buffer` above.
        unsafe { copy_output_slice(bufs.out_markers, &markers_out) };
    }
    Ok(())
}

pub(super) unsafe fn marker_board_detector_detect_impl(
    args: *const ct_marker_board_detect_args_t,
    bufs: *mut ct_marker_board_detect_buffers_t,
) -> FfiResult<()> {
    // SAFETY: caller contract (from `ct_marker_board_detector_detect`): `args` and `bufs`
    // point to valid struct instances with valid sub-pointers per the per-field rules.
    let args = unsafe { require_ref(args, "args")? };
    let bufs = unsafe { require_mut_ref(bufs, "bufs")? };
    // SAFETY: caller contract: `args.detector` is a valid `ct_marker_board_detector_t` handle.
    let detector = unsafe { require_ref(args.detector, "args.detector")? };
    // SAFETY: caller contract: `args.image` points to a valid `ct_gray_image_u8_t` struct.
    let image = unsafe { require_ref(args.image, "args.image")? };
    let prepared = PreparedGrayImage::from_descriptor(image)?;
    let corners = prepared.detect_corners(&detector.chess)?;
    let view = prepared.view();

    let Some(detection) = detector
        .detector
        .detect_from_image_and_corners(&view, &corners)
    else {
        // SAFETY: output pointers are valid per caller contract; null is handled inside helpers.
        unsafe {
            write_required_len(bufs.out_corners_len, 0, "out_corners_len")?;
            write_optional_result(bufs.out_result, ct_marker_board_result_t::default());
        }
        return Err(FfiError::not_found("marker board not detected"));
    };

    let corners_out: Vec<ct_labeled_corner_t> = detection
        .corners
        .iter()
        .map(|corner| labeled_corner_to_ffi(&corner.to_labeled()))
        .collect();
    let result = ct_marker_board_result_t {
        detection: ct_target_detection_t {
            kind: CT_TARGET_KIND_CHECKERBOARD_MARKER,
            corners_len: corners_out.len(),
        },
        has_alignment: if detection.alignment.is_some() {
            CT_TRUE
        } else {
            CT_FALSE
        },
        alignment: detection
            .alignment
            .map(alignment_to_ffi)
            .unwrap_or_default(),
    };

    // SAFETY: output pointers are valid per caller contract; null is handled inside helpers.
    unsafe {
        write_required_len(bufs.out_corners_len, corners_out.len(), "out_corners_len")?;
        write_optional_result(bufs.out_result, result);
    }

    let copy_corners = validate_output_buffer(
        bufs.out_corners,
        bufs.corners_capacity,
        corners_out.len(),
        "out_corners",
    )?;

    if copy_corners {
        // SAFETY: `out_corners` capacity validated by `validate_output_buffer` above.
        unsafe { copy_output_slice(bufs.out_corners, &corners_out) };
    }
    Ok(())
}

pub(super) unsafe fn puzzleboard_detector_detect_impl(
    args: *const ct_puzzleboard_detect_args_t,
    bufs: *mut ct_puzzleboard_detect_buffers_t,
) -> FfiResult<()> {
    // SAFETY: caller contract (from `ct_puzzleboard_detector_detect`): `args` and `bufs`
    // point to valid struct instances with valid sub-pointers per the per-field rules.
    let args = unsafe { require_ref(args, "args")? };
    let bufs = unsafe { require_mut_ref(bufs, "bufs")? };
    // SAFETY: caller contract: `args.detector` is a valid `ct_puzzleboard_detector_t` handle.
    let detector = unsafe { require_ref(args.detector, "args.detector")? };
    // SAFETY: caller contract: `args.image` points to a valid `ct_gray_image_u8_t` struct.
    let image = unsafe { require_ref(args.image, "args.image")? };
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
                write_required_len(bufs.out_corners_len, 0, "out_corners_len")?;
                write_optional_result(bufs.out_result, ct_puzzleboard_result_t::default());
            }
            return Err(err);
        }
    };

    let corners_out: Vec<ct_labeled_corner_t> = detection
        .corners
        .iter()
        .map(|corner| labeled_corner_to_ffi(&corner.to_labeled()))
        .collect();
    let result = ct_puzzleboard_result_t {
        detection: ct_target_detection_t {
            kind: CT_TARGET_KIND_PUZZLEBOARD,
            corners_len: corners_out.len(),
        },
        alignment: alignment_to_ffi(detection.alignment),
        edges_observed: detection.decode.edges_observed,
        edges_matched: detection.decode.edges_matched,
        mean_bit_confidence: detection.decode.mean_confidence,
        bit_error_rate: detection.decode.bit_error_rate,
        master_origin_row: detection.decode.master_origin_row,
        master_origin_col: detection.decode.master_origin_col,
    };

    // SAFETY: output pointers are valid per caller contract; null is handled inside helpers.
    unsafe {
        write_required_len(bufs.out_corners_len, corners_out.len(), "out_corners_len")?;
        write_optional_result(bufs.out_result, result);
    }
    let copy_corners = validate_output_buffer(
        bufs.out_corners,
        bufs.corners_capacity,
        corners_out.len(),
        "out_corners",
    )?;
    if copy_corners {
        // SAFETY: `out_corners` capacity validated by `validate_output_buffer` above.
        unsafe { copy_output_slice(bufs.out_corners, &corners_out) };
    }
    Ok(())
}

pub(super) unsafe fn chessboard_detector_detect_all_impl(
    args: *const ct_chessboard_detect_all_args_t,
    bufs: *mut ct_chessboard_detect_all_buffers_t,
) -> FfiResult<()> {
    // SAFETY: caller contract: `args` and `bufs` point to valid struct
    // instances with valid sub-pointers per the per-field rules.
    let args = unsafe { require_ref(args, "args")? };
    let bufs = unsafe { require_mut_ref(bufs, "bufs")? };
    let detector = unsafe { require_ref(args.detector, "args.detector")? };
    let image = unsafe { require_ref(args.image, "args.image")? };
    let prepared = PreparedGrayImage::from_descriptor(image)?;
    let corners = prepared.detect_corners(&detector.chess)?;

    let detections = detector.detector.detect_all(&corners);

    let results_out: Vec<ct_chessboard_result_t> = detections
        .iter()
        .map(|d| ct_chessboard_result_t {
            corners_len: d.corners.len(),
            cell_size: option_f32_to_ffi(d.cell_size),
        })
        .collect();
    let corners_out: Vec<ct_chessboard_corner_t> = detections
        .iter()
        .flat_map(|d| d.corners.iter().map(chessboard_corner_to_ffi))
        .collect();

    // SAFETY: `bufs.results_len_out` and `bufs.corners_len_out` are valid
    // writable pointers per caller contract.
    unsafe {
        write_required_len(bufs.results_len_out, results_out.len(), "results_len_out")?;
        write_required_len(bufs.corners_len_out, corners_out.len(), "corners_len_out")?;
    }
    let copy_results = validate_output_buffer(
        bufs.results_buf,
        bufs.results_capacity,
        results_out.len(),
        "results_buf",
    )?;
    let copy_corners = validate_output_buffer(
        bufs.corners_buf,
        bufs.corners_capacity,
        corners_out.len(),
        "corners_buf",
    )?;
    if copy_results {
        // SAFETY: `results_buf` capacity validated by `validate_output_buffer`.
        unsafe { copy_output_slice(bufs.results_buf, &results_out) };
    }
    if copy_corners {
        // SAFETY: `corners_buf` capacity validated by `validate_output_buffer`.
        unsafe { copy_output_slice(bufs.corners_buf, &corners_out) };
    }
    Ok(())
}

// ─── Diagnostics JSON-string accessor impls ───────────────────────────────────
//
// The detector diagnostics types are deeply nested `Vec`-of-struct trees with
// an explicitly looser stability promise than the typed result API. Rather
// than freeze them into flat C structs, each `*_detect_diagnostics_json` entry
// point runs detection and renders `serde_json::to_string` of the diagnostics
// struct into a caller-owned UTF-8 buffer, reusing the `ct_last_error_message`
// query/fill discipline (NULL + capacity 0 queries the required length).

fn diagnostics_json<T: serde::Serialize>(value: &T) -> FfiResult<String> {
    serde_json::to_string(value)
        .map_err(|err| FfiError::internal(format!("failed to serialize diagnostics: {err}")))
}

pub(super) unsafe fn chessboard_detector_detect_diagnostics_impl(
    args: *const ct_chessboard_detect_args_t,
    out_utf8: *mut c_char,
    out_capacity: usize,
    out_len: *mut usize,
) -> FfiResult<()> {
    // SAFETY: caller contract: `args` points to a valid struct with valid sub-pointers.
    let args = unsafe { require_ref(args, "args")? };
    // SAFETY: caller contract: `args.detector` is a valid chessboard handle.
    let detector = unsafe { require_ref(args.detector, "args.detector")? };
    // SAFETY: caller contract: `args.image` points to a valid `ct_gray_image_u8_t`.
    let image = unsafe { require_ref(args.image, "args.image")? };
    let prepared = PreparedGrayImage::from_descriptor(image)?;
    let corners = prepared.detect_corners(&detector.chess)?;

    let frame = detector.detector.detect_with_diagnostics(&corners);
    let json = diagnostics_json(&frame)?;
    // SAFETY: `out_utf8` / `out_len` validity is the caller's contract; the
    // helper rejects the null/capacity-mismatch cases internally.
    unsafe { write_json_string(&json, out_utf8, out_capacity, out_len) }
}

pub(super) unsafe fn charuco_detector_detect_diagnostics_impl(
    args: *const ct_charuco_detect_args_t,
    out_utf8: *mut c_char,
    out_capacity: usize,
    out_len: *mut usize,
) -> FfiResult<()> {
    // SAFETY: caller contract: `args` points to a valid struct with valid sub-pointers.
    let args = unsafe { require_ref(args, "args")? };
    // SAFETY: caller contract: `args.detector` is a valid ChArUco handle.
    let detector = unsafe { require_ref(args.detector, "args.detector")? };
    // SAFETY: caller contract: `args.image` points to a valid `ct_gray_image_u8_t`.
    let image = unsafe { require_ref(args.image, "args.image")? };
    let prepared = PreparedGrayImage::from_descriptor(image)?;
    let corners = prepared.detect_corners(&detector.chess)?;
    let view = prepared.view();

    // `detect_with_diagnostics` returns diagnostics even when detection fails,
    // so a failed detection still produces a well-formed JSON payload.
    let (_result, diagnostics) = detector.detector.detect_with_diagnostics(&view, &corners);
    let json = diagnostics_json(&diagnostics)?;
    // SAFETY: see `chessboard_detector_detect_diagnostics_impl`.
    unsafe { write_json_string(&json, out_utf8, out_capacity, out_len) }
}

pub(super) unsafe fn marker_board_detector_detect_diagnostics_impl(
    args: *const ct_marker_board_detect_args_t,
    out_utf8: *mut c_char,
    out_capacity: usize,
    out_len: *mut usize,
) -> FfiResult<()> {
    // SAFETY: caller contract: `args` points to a valid struct with valid sub-pointers.
    let args = unsafe { require_ref(args, "args")? };
    // SAFETY: caller contract: `args.detector` is a valid marker-board handle.
    let detector = unsafe { require_ref(args.detector, "args.detector")? };
    // SAFETY: caller contract: `args.image` points to a valid `ct_gray_image_u8_t`.
    let image = unsafe { require_ref(args.image, "args.image")? };
    let prepared = PreparedGrayImage::from_descriptor(image)?;
    let corners = prepared.detect_corners(&detector.chess)?;
    let view = prepared.view();

    // The marker-board diagnostics channel only yields evidence on a successful
    // detection; a failed detection is reported as `CT_STATUS_NOT_FOUND`.
    let Some((_result, diagnostics)) = detector
        .detector
        .detect_from_image_and_corners_with_diagnostics(&view, &corners)
    else {
        return Err(FfiError::not_found("marker board not detected"));
    };
    let json = diagnostics_json(&diagnostics)?;
    // SAFETY: see `chessboard_detector_detect_diagnostics_impl`.
    unsafe { write_json_string(&json, out_utf8, out_capacity, out_len) }
}

pub(super) unsafe fn puzzleboard_detector_detect_diagnostics_impl(
    args: *const ct_puzzleboard_detect_args_t,
    out_utf8: *mut c_char,
    out_capacity: usize,
    out_len: *mut usize,
) -> FfiResult<()> {
    // SAFETY: caller contract: `args` points to a valid struct with valid sub-pointers.
    let args = unsafe { require_ref(args, "args")? };
    // SAFETY: caller contract: `args.detector` is a valid PuzzleBoard handle.
    let detector = unsafe { require_ref(args.detector, "args.detector")? };
    // SAFETY: caller contract: `args.image` points to a valid `ct_gray_image_u8_t`.
    let image = unsafe { require_ref(args.image, "args.image")? };
    let prepared = PreparedGrayImage::from_descriptor(image)?;
    let corners = prepared.detect_corners(&detector.chess)?;
    let view = prepared.view();

    // `detect_with_diagnostics` returns diagnostics even when detection fails,
    // so a failed decode still produces a well-formed JSON payload.
    let (_result, diagnostics) = detector.detector.detect_with_diagnostics(&view, &corners);
    let json = diagnostics_json(&diagnostics)?;
    // SAFETY: see `chessboard_detector_detect_diagnostics_impl`.
    unsafe { write_json_string(&json, out_utf8, out_capacity, out_len) }
}
