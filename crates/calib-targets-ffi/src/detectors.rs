//! Per-detector create/destroy/detect implementations and exported C symbols.
//!
//! All functions here call into the private helpers defined in `lib.rs`
//! (accessible because this is a child module):
//! `require_ref`, `require_mut_ref`, `write_required_len`, `write_optional_result`,
//! `validate_output_buffer`, `copy_output_slice`, and `PreparedGrayImage`.

// Handle types (ct_*_detector_t) are defined in the parent lib.rs and accessed
// via `super::`. Config, result, and data types come from the types module.
use super::{
    alignment_to_ffi, build_detection_header, chessboard_corner_to_ffi,
    chessboard_params_default_values, convert_charuco_detector_params, convert_chess_config,
    convert_chessboard_params, convert_marker_board_params, convert_puzzleboard_params,
    copy_output_slice, ct_charuco_detector_t, ct_chessboard_detector_t, ct_marker_board_detector_t,
    ct_puzzleboard_detector_t, labeled_corner_to_ffi, map_charuco_create_error,
    map_charuco_detect_error, map_puzzleboard_create_error, map_puzzleboard_detect_error,
    marker_detection_to_ffi, panic_message, require_mut_ref, require_ref, set_last_error_message,
    validate_output_buffer, write_json_string, write_optional_result, write_required_len,
    CharucoDetector, ChessboardDetector, FfiError, FfiResult, MarkerBoardDetector,
    PreparedGrayImage, PuzzleBoardDetector,
};
use crate::error::ffi_status;
use crate::types::{
    ct_charuco_detector_config_t, ct_charuco_result_t, ct_chessboard_corner_t,
    ct_chessboard_detector_config_t, ct_chessboard_params_t, ct_chessboard_result_t,
    ct_gray_image_u8_t, ct_labeled_corner_t, ct_marker_board_detector_config_t,
    ct_marker_board_result_t, ct_marker_detection_t, ct_puzzleboard_detector_config_t,
    ct_puzzleboard_result_t, ct_status_t, CT_FALSE, CT_TRUE,
};
use std::ffi::c_char;
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

/// Input arguments for [`ct_chessboard_detector_detect`].
///
/// Groups the detector handle and image pointer so the entry point's
/// signature stays compact even as future fields are added.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ct_chessboard_detect_args_t {
    /// Detector handle (from [`ct_chessboard_detector_create`]).
    pub detector: *const ct_chessboard_detector_t,
    /// Grayscale image to scan.
    pub image: *const ct_gray_image_u8_t,
}

/// Caller-provided output buffers for [`ct_chessboard_detector_detect`].
///
/// `out_corners_len` is required and always receives the required number of
/// labelled-corner entries. Passing `out_corners = NULL` with
/// `corners_capacity = 0` queries the required length without copying
/// corner data.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ct_chessboard_detect_buffers_t {
    /// Optional scalar result header. May be null.
    pub out_result: *mut ct_chessboard_result_t,
    /// Output array of labelled chessboard corners. May be null when `corners_capacity = 0`.
    pub out_corners: *mut ct_chessboard_corner_t,
    pub corners_capacity: usize,
    /// Required: always receives the number of corners detected.
    pub out_corners_len: *mut usize,
}

/// Input arguments for [`ct_charuco_detector_detect`].
///
/// Groups the detector handle and image pointer so the entry point's
/// signature stays compact even as future fields are added.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ct_charuco_detect_args_t {
    /// Detector handle (from [`ct_charuco_detector_create`]).
    pub detector: *const ct_charuco_detector_t,
    /// Grayscale image to scan.
    pub image: *const ct_gray_image_u8_t,
}

/// Caller-provided output buffers for [`ct_charuco_detector_detect`].
///
/// `out_corners_len` and `out_markers_len` are required and always receive
/// the required array lengths. Passing a `NULL` output array with
/// `*_capacity = 0` queries the required length without copying array data.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ct_charuco_detect_buffers_t {
    /// Optional scalar result header. May be null.
    pub out_result: *mut ct_charuco_result_t,
    /// Output array of labelled corners. May be null when `corners_capacity = 0`.
    pub out_corners: *mut ct_labeled_corner_t,
    pub corners_capacity: usize,
    /// Required: always receives the number of corners detected.
    pub out_corners_len: *mut usize,
    /// Output array of decoded marker detections. May be null when `markers_capacity = 0`.
    pub out_markers: *mut ct_marker_detection_t,
    pub markers_capacity: usize,
    /// Required: always receives the number of markers detected.
    pub out_markers_len: *mut usize,
}

/// Input arguments for [`ct_marker_board_detector_detect`].
///
/// Groups the detector handle and image pointer so the entry point's
/// signature stays compact even as future fields are added.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ct_marker_board_detect_args_t {
    /// Detector handle (from [`ct_marker_board_detector_create`]).
    pub detector: *const ct_marker_board_detector_t,
    /// Grayscale image to scan.
    pub image: *const ct_gray_image_u8_t,
}

/// Caller-provided output buffers for [`ct_marker_board_detector_detect`].
///
/// `out_corners_len` is required and always receives the required number of
/// labelled-corner entries. Passing `out_corners = NULL` with
/// `corners_capacity = 0` queries the required length without copying
/// corner data. Detection evidence (scored circle hypotheses, circle
/// matches) is not surfaced over the C ABI — see
/// [`ct_marker_board_result_t`].
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ct_marker_board_detect_buffers_t {
    /// Optional scalar result header. May be null.
    pub out_result: *mut ct_marker_board_result_t,
    /// Output array of labelled corners. May be null when `corners_capacity = 0`.
    pub out_corners: *mut ct_labeled_corner_t,
    pub corners_capacity: usize,
    /// Required: always receives the number of corners detected.
    pub out_corners_len: *mut usize,
}

/// Input arguments for [`ct_puzzleboard_detector_detect`].
///
/// Groups the detector handle and image pointer so the entry point's
/// signature stays compact even as future fields are added.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ct_puzzleboard_detect_args_t {
    /// Detector handle (from [`ct_puzzleboard_detector_create`]).
    pub detector: *const ct_puzzleboard_detector_t,
    /// Grayscale image to scan.
    pub image: *const ct_gray_image_u8_t,
}

/// Caller-provided output buffers for [`ct_puzzleboard_detector_detect`].
///
/// `out_corners_len` is required and always receives the required number of
/// labeled-corner entries. Passing `out_corners = NULL` with
/// `corners_capacity = 0` queries the required length without copying
/// corner data. The returned corner grid coordinates are master-board
/// `(I, J)` labels.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ct_puzzleboard_detect_buffers_t {
    /// Optional scalar result header. May be null.
    pub out_result: *mut ct_puzzleboard_result_t,
    /// Output array of labelled corners. May be null when `corners_capacity = 0`.
    pub out_corners: *mut ct_labeled_corner_t,
    pub corners_capacity: usize,
    /// Required: always receives the number of corners detected.
    pub out_corners_len: *mut usize,
}

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
        .detection
        .corners
        .iter()
        .map(labeled_corner_to_ffi)
        .collect();
    let result = ct_marker_board_result_t {
        detection: build_detection_header(&detection.detection),
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

// ─── Diagnostics JSON-string accessors ───────────────────────────────────────
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
/// `bufs.out_corners_len` is required and always receives the required number
/// of `ct_chessboard_corner_t` entries. Passing `bufs.out_corners = NULL` and
/// `bufs.corners_capacity = 0` queries the required length without copying
/// corner data.
///
/// # Safety
///
/// `args` and `bufs` must be valid non-null pointers to populated struct
/// instances. Inside `args`: `detector` and `image` must be valid non-null
/// pointers. Inside `bufs`: `out_corners_len` must be a valid non-null
/// writable pointer; `out_result` may be null; `out_corners` may be null
/// when `corners_capacity = 0`, otherwise it must point to writable storage
/// of at least `corners_capacity` entries.
#[no_mangle]
pub unsafe extern "C" fn ct_chessboard_detector_detect(
    args: *const ct_chessboard_detect_args_t,
    bufs: *mut ct_chessboard_detect_buffers_t,
) -> ct_status_t {
    ffi_status(|| unsafe { chessboard_detector_detect_impl(args, bufs) })
}

/// Run chessboard detection and write the diagnostics channel as a
/// NUL-terminated UTF-8 JSON string into a caller-owned buffer.
///
/// The JSON payload is `serde_json::to_string` of the Rust `DebugFrame`
/// diagnostics struct (every input corner's terminal stage, per-iteration
/// pipeline traces, cluster histograms, geometry-check outcomes). Its
/// schema carries a looser stability promise than the typed result API and
/// may evolve between minor versions.
///
/// `out_len` is required and always receives the JSON length excluding the
/// trailing NUL terminator. Query the required size by passing
/// `out_utf8 = NULL` and `out_capacity = 0`.
///
/// # Safety
///
/// `args` must be a valid non-null pointer whose `detector` and `image`
/// fields are valid non-null pointers. If `out_utf8` is non-null it must
/// point to writable memory of at least `out_capacity` bytes. `out_len`
/// must always be a valid writable pointer.
#[no_mangle]
pub unsafe extern "C" fn ct_chessboard_detector_detect_diagnostics_json(
    args: *const ct_chessboard_detect_args_t,
    out_utf8: *mut c_char,
    out_capacity: usize,
    out_len: *mut usize,
) -> ct_status_t {
    ffi_status(|| unsafe {
        chessboard_detector_detect_diagnostics_impl(args, out_utf8, out_capacity, out_len)
    })
}

/// Input arguments for [`ct_chessboard_detector_detect_all`].
///
/// Groups the detector handle and image pointer so the entry point's
/// signature stays compact even as future fields are added.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ct_chessboard_detect_all_args_t {
    /// Detector handle (from [`ct_chessboard_detector_create`]).
    pub detector: *const ct_chessboard_detector_t,
    /// Grayscale image to scan.
    pub image: *const ct_gray_image_u8_t,
}

/// Caller-provided output buffers for [`ct_chessboard_detector_detect_all`].
///
/// Each `*_buf` is the start of a writable output array, `*_capacity`
/// its allocated entry count, and `*_len_out` a writable destination
/// that receives the *required* number of entries (even if the buffer
/// is too small or null). Passing a `NULL` buffer with capacity `0` is
/// allowed and queries the required length without copying data.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ct_chessboard_detect_all_buffers_t {
    /// Output array of per-component result headers.
    pub results_buf: *mut ct_chessboard_result_t,
    pub results_capacity: usize,
    pub results_len_out: *mut usize,
    /// Output array of all components' labelled chessboard corners concatenated.
    pub corners_buf: *mut ct_chessboard_corner_t,
    pub corners_capacity: usize,
    pub corners_len_out: *mut usize,
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

/// Run end-to-end multi-component chessboard detection on a grayscale image.
///
/// Returns every same-board component the detector recovers, up to
/// `DetectorParams::max_components`. The `corners_buf` buffer receives
/// all corners from all components concatenated; use
/// `result[i].detection.corners_len` to slice each component's contribution.
///
/// Both `results_len_out` and `corners_len_out` inside `bufs` are
/// required and always receive the required array lengths. Passing
/// `NULL` output buffers with capacity `0` queries the required
/// lengths without copying data.
///
/// # Safety
///
/// `args` and `bufs` must be valid non-null pointers to populated
/// struct instances. Inside `args`: `detector` and `image` must be
/// valid non-null pointers. Inside `bufs`: `results_len_out` and
/// `corners_len_out` must be valid non-null writable pointers; each
/// output array buffer is allowed to be null when its capacity is `0`,
/// otherwise it must point to writable storage of at least the
/// declared capacity.
#[no_mangle]
pub unsafe extern "C" fn ct_chessboard_detector_detect_all(
    args: *const ct_chessboard_detect_all_args_t,
    bufs: *mut ct_chessboard_detect_all_buffers_t,
) -> ct_status_t {
    ffi_status(|| unsafe { chessboard_detector_detect_all_impl(args, bufs) })
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
/// `bufs.out_corners_len` and `bufs.out_markers_len` are required and always
/// receive the required array lengths. Passing a `NULL` output array with
/// `*_capacity = 0` queries the required length without copying array data.
///
/// # Safety
///
/// `args` and `bufs` must be valid non-null pointers to populated struct
/// instances. Inside `args`: `detector` and `image` must be valid non-null
/// pointers. Inside `bufs`: `out_corners_len` and `out_markers_len` must be
/// valid non-null writable pointers; `out_result` may be null; each array
/// buffer may be null when its capacity is `0`, otherwise it must point to
/// writable storage of at least the declared capacity.
#[no_mangle]
pub unsafe extern "C" fn ct_charuco_detector_detect(
    args: *const ct_charuco_detect_args_t,
    bufs: *mut ct_charuco_detect_buffers_t,
) -> ct_status_t {
    ffi_status(|| unsafe { charuco_detector_detect_impl(args, bufs) })
}

/// Run ChArUco detection and write the diagnostics channel as a
/// NUL-terminated UTF-8 JSON string into a caller-owned buffer.
///
/// The JSON payload is `serde_json::to_string` of the Rust
/// `CharucoDetectDiagnostics` struct (per-component matcher decisions,
/// per-cell scores, chosen/runner-up hypotheses, rejection reasons).
/// Diagnostics are produced even when detection fails, so this entry point
/// returns `CT_STATUS_OK` with a well-formed payload on failed frames; its
/// schema carries a looser stability promise than the typed result API.
///
/// `out_len` is required and always receives the JSON length excluding the
/// trailing NUL terminator. Query the required size by passing
/// `out_utf8 = NULL` and `out_capacity = 0`.
///
/// # Safety
///
/// `args` must be a valid non-null pointer whose `detector` and `image`
/// fields are valid non-null pointers. If `out_utf8` is non-null it must
/// point to writable memory of at least `out_capacity` bytes. `out_len`
/// must always be a valid writable pointer.
#[no_mangle]
pub unsafe extern "C" fn ct_charuco_detector_detect_diagnostics_json(
    args: *const ct_charuco_detect_args_t,
    out_utf8: *mut c_char,
    out_capacity: usize,
    out_len: *mut usize,
) -> ct_status_t {
    ffi_status(|| unsafe {
        charuco_detector_detect_diagnostics_impl(args, out_utf8, out_capacity, out_len)
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
/// The three `*_len` pointers inside `bufs` are required and always receive
/// the required lengths for the corresponding output arrays. Passing a `NULL`
/// output array with `*_capacity = 0` queries the required length without
/// copying array data.
///
/// # Safety
///
/// `args` and `bufs` must be valid non-null pointers to populated struct
/// instances. Inside `args`: `detector` and `image` must be valid non-null
/// pointers. Inside `bufs`: all three `*_len` pointers must be valid non-null
/// writable pointers; `out_result` may be null; each array buffer may be null
/// when its capacity is `0`, otherwise it must point to writable storage of
/// at least the declared capacity.
#[no_mangle]
pub unsafe extern "C" fn ct_marker_board_detector_detect(
    args: *const ct_marker_board_detect_args_t,
    bufs: *mut ct_marker_board_detect_buffers_t,
) -> ct_status_t {
    ffi_status(|| unsafe { marker_board_detector_detect_impl(args, bufs) })
}

/// Run marker-board detection and write the diagnostics channel as a
/// NUL-terminated UTF-8 JSON string into a caller-owned buffer.
///
/// The JSON payload is `serde_json::to_string` of the Rust
/// `MarkerBoardDiagnostics` struct (every scored circle hypothesis, the
/// expected-to-detected circle matches, per-corner provenance, and the
/// alignment-inlier count). The marker-board diagnostics channel only
/// yields evidence on a successful detection, so a failed detection is
/// reported as `CT_STATUS_NOT_FOUND`; its schema carries a looser
/// stability promise than the typed result API.
///
/// `out_len` is required and always receives the JSON length excluding the
/// trailing NUL terminator. Query the required size by passing
/// `out_utf8 = NULL` and `out_capacity = 0`.
///
/// # Safety
///
/// `args` must be a valid non-null pointer whose `detector` and `image`
/// fields are valid non-null pointers. If `out_utf8` is non-null it must
/// point to writable memory of at least `out_capacity` bytes. `out_len`
/// must always be a valid writable pointer.
#[no_mangle]
pub unsafe extern "C" fn ct_marker_board_detector_detect_diagnostics_json(
    args: *const ct_marker_board_detect_args_t,
    out_utf8: *mut c_char,
    out_capacity: usize,
    out_len: *mut usize,
) -> ct_status_t {
    ffi_status(|| unsafe {
        marker_board_detector_detect_diagnostics_impl(args, out_utf8, out_capacity, out_len)
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
/// `bufs.out_corners_len` is required and always receives the required number
/// of labeled-corner entries. Passing `bufs.out_corners = NULL` with
/// `bufs.corners_capacity = 0` queries the required length without copying
/// corner data. The returned corner grid coordinates are master-board
/// `(I, J)` labels.
///
/// # Safety
///
/// `args` and `bufs` must be valid non-null pointers to populated struct
/// instances. Inside `args`: `detector` and `image` must be valid non-null
/// pointers. Inside `bufs`: `out_corners_len` must be a valid non-null
/// writable pointer; `out_result` may be null; `out_corners` may be null
/// when `corners_capacity = 0`, otherwise it must point to writable storage
/// of at least `corners_capacity` entries.
#[no_mangle]
pub unsafe extern "C" fn ct_puzzleboard_detector_detect(
    args: *const ct_puzzleboard_detect_args_t,
    bufs: *mut ct_puzzleboard_detect_buffers_t,
) -> ct_status_t {
    ffi_status(|| unsafe { puzzleboard_detector_detect_impl(args, bufs) })
}

/// Run PuzzleBoard detection and write the diagnostics channel as a
/// NUL-terminated UTF-8 JSON string into a caller-owned buffer.
///
/// The JSON payload is `serde_json::to_string` of the Rust
/// `PuzzleBoardDiagnostics` struct (the raw pre-alignment per-edge bit
/// observations and the winner-vs-runner-up scoring evidence). Diagnostics
/// are produced even when detection fails, so this entry point returns
/// `CT_STATUS_OK` with a well-formed payload on failed frames; its schema
/// carries a looser stability promise than the typed result API.
///
/// `out_len` is required and always receives the JSON length excluding the
/// trailing NUL terminator. Query the required size by passing
/// `out_utf8 = NULL` and `out_capacity = 0`.
///
/// # Safety
///
/// `args` must be a valid non-null pointer whose `detector` and `image`
/// fields are valid non-null pointers. If `out_utf8` is non-null it must
/// point to writable memory of at least `out_capacity` bytes. `out_len`
/// must always be a valid writable pointer.
#[no_mangle]
pub unsafe extern "C" fn ct_puzzleboard_detector_detect_diagnostics_json(
    args: *const ct_puzzleboard_detect_args_t,
    out_utf8: *mut c_char,
    out_capacity: usize,
    out_len: *mut usize,
) -> ct_status_t {
    ffi_status(|| unsafe {
        puzzleboard_detector_detect_diagnostics_impl(args, out_utf8, out_capacity, out_len)
    })
}
