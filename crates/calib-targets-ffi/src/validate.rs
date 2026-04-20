//! Scalar validation helpers for the converter layer.

use crate::error::{FfiError, FfiResult};
use crate::types::{CT_FALSE, CT_TRUE};

pub(crate) fn flag_to_bool(flag: u32, field: &str) -> FfiResult<bool> {
    match flag {
        CT_FALSE => Ok(false),
        CT_TRUE => Ok(true),
        other => Err(FfiError::invalid_argument(format!(
            "{field} must be CT_FALSE or CT_TRUE, got {other}"
        ))),
    }
}

pub(crate) fn require_finite(value: f32, field: &str) -> FfiResult<f32> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(FfiError::config_error(format!("{field} must be finite")))
    }
}

pub(crate) fn require_nonnegative(value: f32, field: &str) -> FfiResult<f32> {
    let value = require_finite(value, field)?;
    if value < 0.0 {
        return Err(FfiError::config_error(format!("{field} must be >= 0")));
    }
    Ok(value)
}

pub(crate) fn require_positive(value: f32, field: &str) -> FfiResult<f32> {
    let value = require_finite(value, field)?;
    if value <= 0.0 {
        return Err(FfiError::config_error(format!("{field} must be > 0")));
    }
    Ok(value)
}

pub(crate) fn require_fraction(value: f32, field: &str) -> FfiResult<f32> {
    let value = require_finite(value, field)?;
    if !(0.0..=1.0).contains(&value) {
        return Err(FfiError::config_error(format!(
            "{field} must be in the inclusive range [0, 1]"
        )));
    }
    Ok(value)
}

#[allow(dead_code)]
pub(crate) fn require_optional_positive_u32(
    value: Option<u32>,
    field: &str,
) -> FfiResult<Option<u32>> {
    if let Some(value) = value {
        if value == 0 {
            return Err(FfiError::config_error(format!(
                "{field} must be > 0 when present"
            )));
        }
        return Ok(Some(value));
    }
    Ok(None)
}
