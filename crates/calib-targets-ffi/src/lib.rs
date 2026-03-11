//! Shared C ABI scaffold for `calib-targets`.
//!
//! Ownership rules:
//! - pointers returned by [`ct_version_string`] are static and must not be freed,
//! - [`ct_last_error_message`] writes into a caller-owned buffer,
//! - future detector/result APIs use caller-owned buffers and explicit destroy calls.
//!
//! Error handling rules:
//! - all exported functions use explicit [`ct_status_t`] values,
//! - panics are trapped before crossing the FFI boundary,
//! - the most recent failure message is stored per-thread and exposed through
//!   [`ct_last_error_message`].

#![allow(non_camel_case_types)]
#![deny(unsafe_op_in_unsafe_fn)]

use std::any::Any;
use std::cell::RefCell;
use std::ffi::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;

/// ABI boolean false.
pub const CT_FALSE: u32 = 0;
/// ABI boolean true.
pub const CT_TRUE: u32 = 1;

const VERSION_CSTR: &[u8] = concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes();

thread_local! {
    static LAST_ERROR_MESSAGE: RefCell<Vec<u8>> = RefCell::new(vec![0]);
}

/// Explicit status codes returned by all exported functions.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ct_status_t {
    CT_STATUS_OK = 0,
    CT_STATUS_NOT_FOUND = 1,
    CT_STATUS_INVALID_ARGUMENT = 2,
    CT_STATUS_BUFFER_TOO_SMALL = 3,
    CT_STATUS_CONFIG_ERROR = 4,
    CT_STATUS_INTERNAL_ERROR = 255,
}

/// Optional `uint32_t` convention used by fixed ABI structs.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ct_optional_u32_t {
    pub has_value: u32,
    pub value: u32,
}

impl ct_optional_u32_t {
    pub const fn none() -> Self {
        Self {
            has_value: CT_FALSE,
            value: 0,
        }
    }

    pub const fn some(value: u32) -> Self {
        Self {
            has_value: CT_TRUE,
            value,
        }
    }
}

/// Optional boolean convention used by fixed ABI structs.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ct_optional_bool_t {
    pub has_value: u32,
    pub value: u32,
}

impl ct_optional_bool_t {
    pub const fn none() -> Self {
        Self {
            has_value: CT_FALSE,
            value: CT_FALSE,
        }
    }

    pub const fn some(value: bool) -> Self {
        Self {
            has_value: CT_TRUE,
            value: if value { CT_TRUE } else { CT_FALSE },
        }
    }
}

/// Optional `float` convention used by fixed ABI structs.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ct_optional_f32_t {
    pub has_value: u32,
    pub value: f32,
}

impl ct_optional_f32_t {
    pub const fn none() -> Self {
        Self {
            has_value: CT_FALSE,
            value: 0.0,
        }
    }

    pub const fn some(value: f32) -> Self {
        Self {
            has_value: CT_TRUE,
            value,
        }
    }
}

/// Shared grayscale image descriptor for `u8` image input.
///
/// `data` points to row-major pixels. `stride_bytes` may be greater than
/// `width` when rows are padded, but it must never be smaller than `width`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ct_gray_image_u8_t {
    pub width: u32,
    pub height: u32,
    pub stride_bytes: usize,
    pub data: *const u8,
}

impl ct_gray_image_u8_t {
    /// Validate the shared image descriptor before converting it into Rust data.
    pub fn validate(&self) -> Result<(), ct_status_t> {
        let width =
            usize::try_from(self.width).map_err(|_| ct_status_t::CT_STATUS_INVALID_ARGUMENT)?;
        let height =
            usize::try_from(self.height).map_err(|_| ct_status_t::CT_STATUS_INVALID_ARGUMENT)?;
        if width == 0 || height == 0 {
            return Err(ct_status_t::CT_STATUS_INVALID_ARGUMENT);
        }
        if self.data.is_null() {
            return Err(ct_status_t::CT_STATUS_INVALID_ARGUMENT);
        }
        if self.stride_bytes < width {
            return Err(ct_status_t::CT_STATUS_INVALID_ARGUMENT);
        }
        self.stride_bytes
            .checked_mul(height)
            .ok_or(ct_status_t::CT_STATUS_INVALID_ARGUMENT)?;
        width
            .checked_mul(height)
            .ok_or(ct_status_t::CT_STATUS_INVALID_ARGUMENT)?;
        Ok(())
    }
}

#[derive(Debug)]
struct FfiError {
    status: ct_status_t,
    message: String,
}

impl FfiError {
    fn invalid_argument(message: impl Into<String>) -> Self {
        Self {
            status: ct_status_t::CT_STATUS_INVALID_ARGUMENT,
            message: message.into(),
        }
    }

    fn buffer_too_small(message: impl Into<String>) -> Self {
        Self {
            status: ct_status_t::CT_STATUS_BUFFER_TOO_SMALL,
            message: message.into(),
        }
    }
}

type FfiResult<T> = Result<T, FfiError>;

fn set_last_error_message(message: impl Into<String>) {
    let mut bytes = message.into().into_bytes();
    bytes.retain(|byte| *byte != 0);
    bytes.push(0);
    LAST_ERROR_MESSAGE.with(|slot| {
        *slot.borrow_mut() = bytes;
    });
}

fn last_error_bytes() -> Vec<u8> {
    LAST_ERROR_MESSAGE.with(|slot| slot.borrow().clone())
}

#[allow(dead_code)]
fn clear_last_error_message() {
    set_last_error_message("");
}

fn panic_message(payload: Box<dyn Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&'static str>() {
        return (*message).to_string();
    }
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    "unknown panic payload".to_string()
}

#[allow(dead_code)]
fn ffi_status(operation: impl FnOnce() -> FfiResult<()>) -> ct_status_t {
    clear_last_error_message();
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(Ok(())) => ct_status_t::CT_STATUS_OK,
        Ok(Err(error)) => {
            set_last_error_message(error.message);
            error.status
        }
        Err(payload) => {
            set_last_error_message(format!(
                "panic across FFI boundary: {}",
                panic_message(payload)
            ));
            ct_status_t::CT_STATUS_INTERNAL_ERROR
        }
    }
}

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
    // SAFETY: out_len is validated to be non-null above.
    unsafe {
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

    // SAFETY: out_utf8 is non-null, capacity is large enough, and `bytes`
    // remains alive for the duration of the copy.
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), out_utf8.cast::<u8>(), bytes.len());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;

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
}
