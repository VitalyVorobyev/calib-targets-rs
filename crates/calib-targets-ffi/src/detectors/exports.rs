//! Public C ABI surface for the detectors: the `#[repr(C)]` arg/buffer
//! structs and the exported `#[no_mangle]` create/destroy/detect/diagnostics
//! symbols. Each exported function is a thin `ffi_status(|| …)` wrapper around
//! a `pub(super)` implementation in [`super::impls`].

use super::impls::{
    charuco_detector_create_impl, charuco_detector_detect_diagnostics_impl,
    charuco_detector_detect_impl, chessboard_detector_create_impl,
    chessboard_detector_detect_all_impl, chessboard_detector_detect_diagnostics_impl,
    chessboard_detector_detect_impl, marker_board_detector_create_impl,
    marker_board_detector_detect_diagnostics_impl, marker_board_detector_detect_impl,
    puzzleboard_detector_create_impl, puzzleboard_detector_detect_diagnostics_impl,
    puzzleboard_detector_detect_impl,
};
use crate::error::ffi_status;
use crate::types::{
    ct_charuco_detector_config_t, ct_charuco_result_t, ct_chessboard_corner_t,
    ct_chessboard_detector_config_t, ct_chessboard_params_t, ct_chessboard_result_t,
    ct_gray_image_u8_t, ct_labeled_corner_t, ct_marker_board_detector_config_t,
    ct_marker_board_result_t, ct_marker_detection_t, ct_puzzleboard_detector_config_t,
    ct_puzzleboard_result_t, ct_status_t,
};
use crate::{
    chessboard_params_default_values, ct_charuco_detector_t, ct_chessboard_detector_t,
    ct_marker_board_detector_t, ct_puzzleboard_detector_t, panic_message, set_last_error_message,
};
use std::ffi::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};

// ─── Detect arg/buffer structs ────────────────────────────────────────────────

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

// ─── Chessboard exported functions ────────────────────────────────────────────

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

/// Run end-to-end multi-component chessboard detection on a grayscale image.
///
/// Returns every same-board component the detector recovers, up to
/// `DetectorParams::max_components`. The `corners_buf` buffer receives
/// all corners from all components concatenated; use
/// `result[i].corners_len` to slice each component's contribution.
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

// ─── ChArUco exported functions ───────────────────────────────────────────────

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

// ─── Marker-board exported functions ──────────────────────────────────────────

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

// ─── PuzzleBoard exported functions ───────────────────────────────────────────

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
