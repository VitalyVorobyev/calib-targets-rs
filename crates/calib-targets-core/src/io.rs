//! Shared JSON IO helpers for config and report types.

use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fs;
use std::path::Path;

/// Errors from JSON file I/O.
#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
pub enum IoError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Load a JSON-serialized value from a file.
pub fn load_json<T: DeserializeOwned>(path: impl AsRef<Path>) -> Result<T, IoError> {
    let raw = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&raw)?)
}

/// Write a value to a file as pretty-printed JSON.
pub fn write_json<T: Serialize>(value: &T, path: impl AsRef<Path>) -> Result<(), IoError> {
    let json = serde_json::to_string_pretty(value)?;
    fs::write(path, json)?;
    Ok(())
}
