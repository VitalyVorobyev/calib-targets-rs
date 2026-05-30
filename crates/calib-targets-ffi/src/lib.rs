//! Shared C ABI for `calib-targets`.
//!
//! Ownership rules:
//! - pointers returned by [`ct_version_string`] are static and must not be freed,
//! - [`ct_last_error_message`] writes into a caller-owned buffer,
//! - detector handles returned by `*_create` are owned by the caller and must be
//!   released with the matching `*_destroy`,
//! - variable-length detection outputs use caller-owned arrays with query/fill
//!   semantics (`NULL` + capacity `0` queries the required length).
//!
//! Error handling rules:
//! - exported functions use explicit [`ct_status_t`] values,
//! - panics are trapped before crossing the FFI boundary,
//! - the most recent failure message is stored per-thread and exposed through
//!   [`ct_last_error_message`].

#![allow(non_camel_case_types)]
#![deny(unsafe_op_in_unsafe_fn)]

#[doc(hidden)]
pub mod package_support;

mod convert;
mod detectors;
mod error;
mod types;
mod validate;

// Re-export all public ABI types at crate root so the existing C header and
// cbindgen configuration continue to find them at the expected path.
pub use types::*;

use crate::convert::{
    alignment_to_ffi, chessboard_corner_to_ffi, chessboard_params_default_values,
    convert_charuco_detector_params, convert_chess_config, convert_chessboard_params,
    convert_marker_board_params, convert_puzzleboard_params, labeled_corner_to_ffi,
    map_charuco_create_error, map_charuco_detect_error, map_puzzleboard_create_error,
    map_puzzleboard_detect_error, marker_detection_to_ffi, option_f32_to_ffi,
};
use crate::error::{last_error_bytes, panic_message, set_last_error_message, FfiError, FfiResult};

use calib_targets::charuco::CharucoDetector;
use calib_targets::chessboard::Detector as ChessboardDetector;
use calib_targets::core::GrayImageView;
use calib_targets::detect::{self, DetectorConfig};
use calib_targets::marker::MarkerBoardDetector;
use calib_targets::puzzleboard::PuzzleBoardDetector;
use std::ffi::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::slice;

const VERSION_CSTR: &[u8] = concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes();

// ─── Opaque detector handle types ───────────────────────────────────────────

/// Opaque chessboard detector handle.
pub struct ct_chessboard_detector_t {
    chess: DetectorConfig,
    detector: ChessboardDetector,
}

/// Opaque ChArUco detector handle.
pub struct ct_charuco_detector_t {
    chess: DetectorConfig,
    detector: CharucoDetector,
}

/// Opaque marker-board detector handle.
pub struct ct_marker_board_detector_t {
    chess: DetectorConfig,
    detector: MarkerBoardDetector,
}

/// Opaque PuzzleBoard detector handle.
pub struct ct_puzzleboard_detector_t {
    chess: DetectorConfig,
    detector: PuzzleBoardDetector,
}

// ─── Image preparation ───────────────────────────────────────────────────────

struct PreparedGrayImage {
    width: u32,
    height: u32,
    width_usize: usize,
    height_usize: usize,
    pixels: Vec<u8>,
}

impl PreparedGrayImage {
    fn from_descriptor(image: &ct_gray_image_u8_t) -> FfiResult<Self> {
        image
            .validate()
            .map_err(|_| FfiError::invalid_argument("invalid ct_gray_image_u8_t"))?;

        let width = usize::try_from(image.width).map_err(|_| {
            FfiError::invalid_argument("image width does not fit into usize on this platform")
        })?;
        let height = usize::try_from(image.height).map_err(|_| {
            FfiError::invalid_argument("image height does not fit into usize on this platform")
        })?;
        let stride = image.stride_bytes;
        let source_len = stride.checked_mul(height).ok_or_else(|| {
            FfiError::invalid_argument("image stride_bytes * height overflows usize")
        })?;

        let source = unsafe {
            // SAFETY: `validate` above guarantees `data` is non-null and that
            // `stride * height` does not overflow. The caller owns the backing
            // memory for the duration of the FFI call.
            slice::from_raw_parts(image.data, source_len)
        };

        let pixel_count = width
            .checked_mul(height)
            .ok_or_else(|| FfiError::invalid_argument("image width * height overflows usize"))?;
        let mut pixels = Vec::with_capacity(pixel_count);
        for row in 0..height {
            let start = row * stride;
            pixels.extend_from_slice(&source[start..start + width]);
        }

        Ok(Self {
            width: image.width,
            height: image.height,
            width_usize: width,
            height_usize: height,
            pixels,
        })
    }

    fn detect_corners(
        &self,
        chess: &DetectorConfig,
    ) -> FfiResult<Vec<calib_targets::chessboard::ChessCorner>> {
        let gray = detect::gray_image_from_slice(self.width, self.height, &self.pixels)
            .map_err(|err| FfiError::internal(format!("failed to build grayscale image: {err}")))?;
        // Pre-blur is not exposed via the C ABI; callers requiring it must
        // pre-process the image before submitting it.
        Ok(detect::detect_corners(&gray, chess))
    }

    fn view(&self) -> GrayImageView<'_> {
        GrayImageView {
            width: self.width_usize,
            height: self.height_usize,
            data: &self.pixels,
        }
    }
}

// ─── Shared output-buffer helpers ────────────────────────────────────────────

unsafe fn require_ref<'a, T>(ptr: *const T, name: &str) -> FfiResult<&'a T> {
    if ptr.is_null() {
        return Err(FfiError::invalid_argument(format!(
            "{name} must not be null"
        )));
    }
    Ok(unsafe {
        // SAFETY: The caller guarantees the pointer is valid for the duration
        // of the FFI call; null is rejected above.
        &*ptr
    })
}

unsafe fn require_mut_ref<'a, T>(ptr: *mut T, name: &str) -> FfiResult<&'a mut T> {
    if ptr.is_null() {
        return Err(FfiError::invalid_argument(format!(
            "{name} must not be null"
        )));
    }
    Ok(unsafe {
        // SAFETY: The caller guarantees the pointer is valid for the duration
        // of the FFI call; null is rejected above.
        &mut *ptr
    })
}

unsafe fn write_required_len(out_len: *mut usize, len: usize, name: &str) -> FfiResult<()> {
    if out_len.is_null() {
        return Err(FfiError::invalid_argument(format!(
            "{name} must not be null"
        )));
    }
    unsafe {
        // SAFETY: null is rejected above.
        *out_len = len;
    }
    Ok(())
}

fn validate_output_buffer<T>(
    ptr: *mut T,
    capacity: usize,
    required_len: usize,
    name: &str,
) -> FfiResult<bool> {
    if ptr.is_null() {
        if capacity != 0 {
            return Err(FfiError::invalid_argument(format!(
                "{name} is null but capacity is {capacity}"
            )));
        }
        return Ok(false);
    }
    if capacity < required_len {
        return Err(FfiError::buffer_too_small(format!(
            "{name} needs {required_len} entries, capacity is {capacity}"
        )));
    }
    Ok(required_len != 0)
}

unsafe fn copy_output_slice<T: Copy>(out: *mut T, values: &[T]) {
    if out.is_null() || values.is_empty() {
        return;
    }
    unsafe {
        // SAFETY: caller validated capacity before copying and `values`
        // remains alive for the duration of the copy.
        ptr::copy_nonoverlapping(values.as_ptr(), out, values.len());
    }
}

unsafe fn write_optional_result<T: Copy>(out: *mut T, value: T) {
    if out.is_null() {
        return;
    }
    unsafe {
        // SAFETY: the caller owns `out` and provided it as a writable pointer.
        *out = value;
    }
}

/// Write a UTF-8 JSON string into a caller-owned buffer using the same
/// query/fill semantics as [`ct_last_error_message`].
///
/// `json` is the already-rendered diagnostics payload. `out_len` always
/// receives the message length **excluding** the trailing NUL terminator.
/// Passing `out_utf8 = NULL` with `out_capacity = 0` queries the required
/// length without copying. The detectors' diagnostics JSON accessors share
/// this helper so the C ABI exposes exactly one owned-string discipline.
///
/// # Safety
///
/// If `out_utf8` is non-null it must point to writable memory of at least
/// `out_capacity` bytes. `out_len` must always be a valid writable pointer.
unsafe fn write_json_string(
    json: &str,
    out_utf8: *mut c_char,
    out_capacity: usize,
    out_len: *mut usize,
) -> FfiResult<()> {
    if out_len.is_null() {
        return Err(FfiError::invalid_argument("out_len must not be null"));
    }
    if out_utf8.is_null() && out_capacity != 0 {
        return Err(FfiError::invalid_argument(
            "out_utf8 is null but out_capacity is non-zero",
        ));
    }

    let message_len = json.len();
    let with_nul = message_len + 1;
    unsafe {
        // SAFETY: null is rejected above.
        *out_len = message_len;
    }

    if out_utf8.is_null() {
        return Ok(());
    }
    if out_capacity < with_nul {
        return Err(FfiError::buffer_too_small(format!(
            "out_utf8 needs {with_nul} bytes including the trailing NUL terminator"
        )));
    }

    unsafe {
        // SAFETY: `out_utf8` is non-null, the capacity check above guarantees
        // room for `message_len` bytes plus the NUL, and `json` outlives the copy.
        ptr::copy_nonoverlapping(json.as_ptr(), out_utf8.cast::<u8>(), message_len);
        *out_utf8.add(message_len) = 0;
    }
    Ok(())
}

// ─── Public exported functions ───────────────────────────────────────────────

/// Return the shared library version string.
///
/// The returned pointer is static storage and must not be freed by the caller.
#[no_mangle]
pub extern "C" fn ct_version_string() -> *const c_char {
    VERSION_CSTR.as_ptr().cast()
}

/// Copy the most recent thread-local FFI error message into a caller-owned buffer.
///
/// `out_len` is required and always receives the message length excluding the
/// trailing NUL terminator. Callers may query the required size by passing
/// `out_utf8 = NULL` and `out_capacity = 0`.
/// This function does not overwrite the stored thread-local error message if
/// the retrieval call itself fails.
///
/// # Safety
///
/// If `out_utf8` is non-null, it must point to writable memory of at least
/// `out_capacity` bytes. `out_len` must always be a valid writable pointer.
#[no_mangle]
pub unsafe extern "C" fn ct_last_error_message(
    out_utf8: *mut c_char,
    out_capacity: usize,
    out_len: *mut usize,
) -> ct_status_t {
    match catch_unwind(AssertUnwindSafe(|| unsafe {
        last_error_message_impl(out_utf8, out_capacity, out_len)
    })) {
        Ok(Ok(())) => ct_status_t::CT_STATUS_OK,
        Ok(Err(error)) => error.status,
        Err(_) => ct_status_t::CT_STATUS_INTERNAL_ERROR,
    }
}

unsafe fn last_error_message_impl(
    out_utf8: *mut c_char,
    out_capacity: usize,
    out_len: *mut usize,
) -> FfiResult<()> {
    if out_len.is_null() {
        return Err(FfiError::invalid_argument(
            "ct_last_error_message requires a non-null out_len pointer",
        ));
    }
    if out_utf8.is_null() && out_capacity != 0 {
        return Err(FfiError::invalid_argument(
            "ct_last_error_message received a null output buffer with non-zero capacity",
        ));
    }

    let bytes = last_error_bytes();
    let message_len = bytes.len().saturating_sub(1);
    unsafe {
        // SAFETY: null is rejected above.
        *out_len = message_len;
    }

    if out_utf8.is_null() {
        return Ok(());
    }
    if out_capacity < bytes.len() {
        return Err(FfiError::buffer_too_small(format!(
            "ct_last_error_message needs {} bytes including the trailing NUL terminator",
            bytes.len()
        )));
    }

    unsafe {
        // SAFETY: `out_utf8` is non-null, the capacity check above ensures the
        // destination is large enough, and `bytes` remains alive for the copy.
        ptr::copy_nonoverlapping(bytes.as_ptr(), out_utf8.cast::<u8>(), bytes.len());
    }
    Ok(())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
