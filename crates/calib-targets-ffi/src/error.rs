//! FFI error types, panic trapping, and thread-local error message storage.

use crate::types::ct_status_t;
use std::any::Any;
use std::cell::RefCell;
use std::panic::{catch_unwind, AssertUnwindSafe};

#[derive(Debug)]
pub(crate) struct FfiError {
    pub(crate) status: ct_status_t,
    pub(crate) message: String,
}

impl FfiError {
    pub(crate) fn invalid_argument(message: impl Into<String>) -> Self {
        Self {
            status: ct_status_t::CT_STATUS_INVALID_ARGUMENT,
            message: message.into(),
        }
    }

    pub(crate) fn buffer_too_small(message: impl Into<String>) -> Self {
        Self {
            status: ct_status_t::CT_STATUS_BUFFER_TOO_SMALL,
            message: message.into(),
        }
    }

    pub(crate) fn config_error(message: impl Into<String>) -> Self {
        Self {
            status: ct_status_t::CT_STATUS_CONFIG_ERROR,
            message: message.into(),
        }
    }

    pub(crate) fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: ct_status_t::CT_STATUS_NOT_FOUND,
            message: message.into(),
        }
    }

    pub(crate) fn internal(message: impl Into<String>) -> Self {
        Self {
            status: ct_status_t::CT_STATUS_INTERNAL_ERROR,
            message: message.into(),
        }
    }
}

pub(crate) type FfiResult<T> = Result<T, FfiError>;

thread_local! {
    pub(crate) static LAST_ERROR_MESSAGE: RefCell<Vec<u8>> = RefCell::new(vec![0]);
}

pub(crate) fn set_last_error_message(message: impl Into<String>) {
    let mut bytes = message.into().into_bytes();
    bytes.retain(|byte| *byte != 0);
    bytes.push(0);
    LAST_ERROR_MESSAGE.with(|slot| {
        *slot.borrow_mut() = bytes;
    });
}

pub(crate) fn last_error_bytes() -> Vec<u8> {
    LAST_ERROR_MESSAGE.with(|slot| slot.borrow().clone())
}

pub(crate) fn clear_last_error_message() {
    set_last_error_message("");
}

pub(crate) fn panic_message(payload: Box<dyn Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&'static str>() {
        return (*message).to_string();
    }
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    "unknown panic payload".to_string()
}

pub(crate) fn ffi_status(operation: impl FnOnce() -> FfiResult<()>) -> ct_status_t {
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
